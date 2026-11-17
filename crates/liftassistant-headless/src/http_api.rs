//! Minimal experimental HTTP boundary for read-only external clients.

use std::{
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    http::{HeaderMap, HeaderValue, header},
    routing::get,
};

use crate::read_api::ExperimentalReadModel;

pub(super) const SNAPSHOT_PATH: &str = "/api/experimental/v0/snapshot";

#[derive(Clone)]
struct ApiState {
    read_model: Arc<ExperimentalReadModel>,
}

pub(super) fn serve(
    listener: TcpListener,
    read_model: ExperimentalReadModel,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure experimental API listener: {error}"))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to create experimental API runtime: {error}"))?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(|error| format!("failed to adopt experimental API listener: {error}"))?;
        axum::serve(listener, router(read_model))
            .with_graceful_shutdown(wait_for_shutdown(shutdown))
            .await
            .map_err(|error| format!("experimental API server failed: {error}"))
    })
}

fn router(read_model: ExperimentalReadModel) -> Router {
    Router::new()
        .route(SNAPSHOT_PATH, get(snapshot))
        .with_state(ApiState {
            read_model: Arc::new(read_model),
        })
}

async fn snapshot(
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> (HeaderMap, Json<ExperimentalReadModel>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, Json((*state.read_model).clone()))
}

async fn wait_for_shutdown(shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpStream},
        thread,
        time::Duration,
    };

    use liftassistant_model::{BuildingTopology, CarId};
    use serde_json::Value;

    use super::*;
    use crate::{ObservationBindingRole, ReadOnlyHost};

    fn request(address: SocketAddr, request: &str) -> String {
        let mut stream = (0..50)
            .find_map(|_| match TcpStream::connect(address) {
                Ok(stream) => Some(stream),
                Err(_) => {
                    thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("real experimental API listener accepts a connection");
        stream
            .write_all(request.as_bytes())
            .expect("request writes");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("response reads");
        response
    }

    #[test]
    fn real_listener_serves_no_store_snapshot_and_rejects_mutation_methods() {
        let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());
        host.bind_observation(
            crate::ObservationSourceId::new("fixture/http"),
            CarId::new("A"),
            ObservationBindingRole::Operational,
        )
        .expect("binding succeeds");
        let expected_revision = host.read_model().revision();
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener binds");
        let address = listener.local_addr().expect("listener has an address");
        let shutdown = Arc::new(AtomicBool::new(false));
        let server_shutdown = Arc::clone(&shutdown);
        let server = thread::spawn(move || serve(listener, host.read_model(), server_shutdown));

        let response = request(
            address,
            &format!(
                "GET {SNAPSHOT_PATH} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(
            response
                .to_ascii_lowercase()
                .contains("cache-control: no-store")
        );
        assert!(
            response
                .to_ascii_lowercase()
                .contains("content-type: application/json")
        );
        assert!(
            !response
                .to_ascii_lowercase()
                .contains("access-control-allow-origin")
        );
        let (_, body) = response
            .split_once("\r\n\r\n")
            .expect("response has a body separator");
        let value: Value = serde_json::from_str(body).expect("snapshot body is JSON");
        assert_eq!(value["api_version"], "experimental/v0");
        assert_eq!(value["revision"], expected_revision);

        let post = request(
            address,
            &format!(
                "POST {SNAPSHOT_PATH} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(post.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"));

        shutdown.store(true, Ordering::Relaxed);
        server
            .join()
            .expect("server thread joins")
            .expect("server shuts down cleanly");
    }
}
