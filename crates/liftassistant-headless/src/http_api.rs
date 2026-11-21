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
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use tokio::sync::watch;
use tokio_stream::{StreamExt, wrappers::WatchStream};

use crate::read_api::ExperimentalReadModel;

pub(super) const SNAPSHOT_PATH: &str = "/api/experimental/v0/snapshot";
pub(super) const EVENTS_PATH: &str = "/api/experimental/v0/events";
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub(super) struct ReadModelPublisher {
    models: watch::Sender<ExperimentalReadModel>,
}

impl ReadModelPublisher {
    pub(super) fn new(initial: ExperimentalReadModel) -> Self {
        let (models, _) = watch::channel(initial);
        Self { models }
    }

    pub(super) fn publish(&self, model: ExperimentalReadModel) -> bool {
        if model.revision() <= self.models.borrow().revision() {
            return false;
        }
        self.models.send_replace(model);
        true
    }

    fn subscribe(&self) -> watch::Receiver<ExperimentalReadModel> {
        self.models.subscribe()
    }
}

#[derive(Clone)]
struct ApiState {
    models: watch::Receiver<ExperimentalReadModel>,
}

pub(super) fn serve(
    listener: TcpListener,
    publisher: ReadModelPublisher,
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
        let mut upstream = publisher.subscribe();
        let (models_tx, models_rx) = watch::channel(upstream.borrow_and_update().clone());
        let relay_shutdown = Arc::clone(&shutdown);
        tokio::spawn(async move {
            let _publisher_guard = publisher;
            loop {
                tokio::select! {
                    () = wait_for_shutdown(Arc::clone(&relay_shutdown)) => break,
                    changed = upstream.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        let latest = upstream.borrow_and_update().clone();
                        let _ = models_tx.send_replace(latest);
                    }
                }
            }
        });
        let drain_deadline = async {
            wait_for_shutdown(Arc::clone(&shutdown)).await;
            tokio::time::sleep(SHUTDOWN_DRAIN_TIMEOUT).await;
        };
        let server = axum::serve(listener, router(models_rx))
            .with_graceful_shutdown(wait_for_shutdown(Arc::clone(&shutdown)));
        tokio::select! {
            result = server.into_future() =>
                result.map_err(|error| format!("experimental API server failed: {error}")),
            () = drain_deadline => {
                // Returning drops this API runtime and its remaining connection tasks.
                crate::report_host_warning(format_args!(
                    "warning: experimental API shutdown drain timed out; closing remaining connections"
                ));
                Ok(())
            }
        }
    })
}

fn router(models: watch::Receiver<ExperimentalReadModel>) -> Router {
    Router::new()
        .route(SNAPSHOT_PATH, get(snapshot))
        .route(EVENTS_PATH, get(events))
        .with_state(ApiState { models })
}

async fn snapshot(
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> (HeaderMap, Json<ExperimentalReadModel>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, Json(state.models.borrow().clone()))
}

async fn events(
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> (
    HeaderMap,
    Sse<impl tokio_stream::Stream<Item = Result<Event, serde_json::Error>> + Send + 'static>,
) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let snapshots = WatchStream::new(state.models).map(|model| {
        serde_json::to_string(&model).map(|json| Event::default().event("snapshot").data(json))
    });
    (
        headers,
        Sse::new(snapshots).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        ),
    )
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
        let publisher = ReadModelPublisher::new(host.read_model());
        let server_publisher = publisher.clone();
        let server = thread::spawn(move || serve(listener, server_publisher, server_shutdown));

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

    fn read_until(stream: &mut TcpStream, response: &mut Vec<u8>, needle: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !String::from_utf8_lossy(response).contains(needle) {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out reading SSE"
            );
            let mut chunk = [0_u8; 4096];
            match stream.read(&mut chunk) {
                Ok(0) => panic!("SSE connection closed before {needle:?}"),
                Ok(read) => response.extend_from_slice(&chunk[..read]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => panic!("failed reading SSE response: {error}"),
            }
        }
    }

    #[test]
    fn sse_pushes_complete_latest_snapshots_and_reconnects_without_backlog() {
        let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());
        let source = liftassistant_model::ObservationSourceId::new("fixture/sse");
        host.bind_observation(
            source.clone(),
            CarId::new("A"),
            ObservationBindingRole::Operational,
        )
        .expect("binding succeeds");
        let publisher = ReadModelPublisher::new(host.read_model());
        let initial_revision = host.read_model().revision();
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener binds");
        let address = listener.local_addr().expect("listener has an address");
        let shutdown = Arc::new(AtomicBool::new(false));
        let server_shutdown = Arc::clone(&shutdown);
        let server_publisher = publisher.clone();
        let server = thread::spawn(move || serve(listener, server_publisher, server_shutdown));

        let mut stream = (0..50)
            .find_map(|_| match TcpStream::connect(address) {
                Ok(stream) => Some(stream),
                Err(_) => {
                    thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("SSE listener accepts a connection");
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .expect("read timeout configures");
        write!(
            stream,
            "GET {EVENTS_PATH} HTTP/1.1\r\nHost: {address}\r\nAccept: text/event-stream\r\n\r\n"
        )
        .expect("SSE request writes");
        let mut response = Vec::new();
        read_until(
            &mut stream,
            &mut response,
            &format!("\"revision\":{initial_revision}"),
        );
        let initial_text = String::from_utf8_lossy(&response);
        assert!(initial_text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(
            initial_text
                .to_ascii_lowercase()
                .contains("text/event-stream")
        );
        assert!(initial_text.contains("event: snapshot"));

        for sequence in 1..=20 {
            host.record_transport_failure(
                source.clone(),
                liftassistant_model::ObservationSessionId::new("sse-session"),
                format!("coalesced fixture {sequence}"),
            );
            assert!(publisher.publish(host.read_model()));
        }
        let latest_revision = host.read_model().revision();
        read_until(
            &mut stream,
            &mut response,
            &format!("\"revision\":{latest_revision}"),
        );

        let mut reconnected = TcpStream::connect(address).expect("SSE reconnect succeeds");
        reconnected
            .set_read_timeout(Some(Duration::from_millis(100)))
            .expect("read timeout configures");
        write!(
            reconnected,
            "GET {EVENTS_PATH} HTTP/1.1\r\nHost: {address}\r\nAccept: text/event-stream\r\n\r\n"
        )
        .expect("reconnect request writes");
        let mut reconnect_response = Vec::new();
        read_until(
            &mut reconnected,
            &mut reconnect_response,
            &format!("\"revision\":{latest_revision}"),
        );
        assert!(String::from_utf8_lossy(&reconnect_response).contains("event: snapshot"));

        let mut slow_receiver = publisher.subscribe();
        let _ = slow_receiver.borrow_and_update().clone();
        for sequence in 21..=40 {
            host.record_transport_failure(
                source.clone(),
                liftassistant_model::ObservationSessionId::new("sse-session"),
                format!("latest-only fixture {sequence}"),
            );
            assert!(publisher.publish(host.read_model()));
        }
        assert!(slow_receiver.has_changed().expect("publisher remains open"));
        assert_eq!(
            slow_receiver.borrow_and_update().revision(),
            host.read_model().revision()
        );

        shutdown.store(true, Ordering::Relaxed);
        drop(stream);
        drop(reconnected);
        server
            .join()
            .expect("server thread joins")
            .expect("server shuts down cleanly");
    }

    #[test]
    fn sse_stays_open_after_external_publisher_drop_and_closes_on_shutdown() {
        let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());
        let source = liftassistant_model::ObservationSourceId::new("fixture/sse-lifetime");
        host.bind_observation(
            source.clone(),
            CarId::new("A"),
            ObservationBindingRole::Operational,
        )
        .expect("binding succeeds");
        let publisher = ReadModelPublisher::new(host.read_model());
        let external_publisher = publisher.clone();
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener binds");
        let address = listener.local_addr().expect("listener has an address");
        let shutdown = Arc::new(AtomicBool::new(false));
        let server_shutdown = Arc::clone(&shutdown);
        let server = thread::spawn(move || serve(listener, publisher, server_shutdown));

        let mut stream = (0..50)
            .find_map(|_| match TcpStream::connect(address) {
                Ok(stream) => Some(stream),
                Err(_) => {
                    thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("SSE listener accepts a connection");
        stream
            .set_read_timeout(Some(Duration::from_millis(120)))
            .expect("read timeout configures");
        write!(
            stream,
            "GET {EVENTS_PATH} HTTP/1.1\r\nHost: {address}\r\nAccept: text/event-stream\r\n\r\n"
        )
        .expect("SSE request writes");

        host.record_transport_failure(
            source,
            liftassistant_model::ObservationSessionId::new("sse-lifetime-session"),
            "final fixture update".into(),
        );
        let final_revision = host.read_model().revision();
        assert!(external_publisher.publish(host.read_model()));
        drop(external_publisher);

        let mut response = Vec::new();
        read_until(
            &mut stream,
            &mut response,
            &format!("\"revision\":{final_revision}"),
        );

        // Stream must remain open (WouldBlock/TimedOut, not EOF 0) while shutdown is false.
        let mut probe = [0_u8; 64];
        match stream.read(&mut probe) {
            Ok(0) => panic!("SSE stream closed prematurely before shutdown"),
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => panic!("unexpected SSE read error: {error}"),
        }

        // Cooperative shutdown must terminate open SSE streams without waiting for client drop.
        shutdown.store(true, Ordering::Relaxed);
        server
            .join()
            .expect("server thread joins")
            .expect("server shuts down cleanly with active SSE client");
    }

    #[test]
    fn shutdown_releases_listener_even_if_an_sse_peer_stops_reading() {
        let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());
        // A response larger than the loopback send buffers makes the response genuinely pending.
        host.record_mapping_rejection(
            crate::ObservationSourceId::new("fixture/backpressure"),
            crate::ObservationSessionId::new("backpressure-session"),
            "x".repeat(16 * 1024 * 1024),
            "synthetic evidence".into(),
        );
        let publisher = ReadModelPublisher::new(host.read_model());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));
        let server_shutdown = Arc::clone(&shutdown);
        let (finished, outcome) = std::sync::mpsc::sync_channel(1);
        let server = thread::spawn(move || {
            finished
                .send(serve(listener, publisher, server_shutdown))
                .unwrap();
        });
        let mut stalled = TcpStream::connect(address).unwrap();
        stalled
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        write!(
            stalled,
            "GET {EVENTS_PATH} HTTP/1.1\r\nHost: {address}\r\n\r\n"
        )
        .unwrap();
        read_until(&mut stalled, &mut Vec::new(), "event: snapshot");

        shutdown.store(true, Ordering::Relaxed);
        let result = outcome.recv_timeout(Duration::from_secs(4));
        // Allow the unfixed server to stop too, so a failed regression cannot leak a thread.
        drop(stalled);
        server.join().unwrap();
        result
            .expect("shutdown must finish while the SSE peer is still open")
            .unwrap();
        assert!(TcpStream::connect(address).is_err());
    }
}
