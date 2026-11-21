//! HTTP/SSE transport for the isolated Simulator development server.
//!
//! - `/api/experimental/v0/snapshot` and `/api/experimental/v0/events` remain strictly read-only GET.
//! - `/api/dev-sim/v0/*` routes are mounted ONLY when explicitly opted in via
//!   `--enable-dev-sim-control` on a verified loopback address.

use std::{
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_stream::{StreamExt, wrappers::WatchStream};

use crate::{
    control::{
        DEV_CONTROL_API_VERSION, SimDevControl, SubmitError, ValidationError, validate_operation,
    },
    read_projection::SimReadModel,
};

pub(crate) const SNAPSHOT_PATH: &str = "/api/experimental/v0/snapshot";
pub(crate) const EVENTS_PATH: &str = "/api/experimental/v0/events";
pub(crate) const DEV_CAPABILITIES_PATH: &str = "/api/dev-sim/v0/capabilities";
pub(crate) const DEV_SERVE_FLOOR_PREVIEW_PATH: &str = "/api/dev-sim/v0/serve-floor/preview";
pub(crate) const DEV_SERVE_FLOOR_SUBMIT_PATH: &str = "/api/dev-sim/v0/serve-floor/submit";
pub(crate) const DEV_SUBMISSION_QUERY_PATH: &str = "/api/dev-sim/v0/submissions/{submission_key}";

const MAX_CONTROL_BODY_BYTES: usize = 4_096;
const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub(crate) struct ReadModelPublisher {
    models: watch::Sender<SimReadModel>,
}

impl ReadModelPublisher {
    pub(crate) fn new(initial: SimReadModel) -> Self {
        let (models, _) = watch::channel(initial);
        Self { models }
    }

    pub(crate) fn publish(&self, model: SimReadModel) -> bool {
        if model.revision() <= self.models.borrow().revision() {
            return false;
        }
        self.models.send_replace(model);
        true
    }

    fn subscribe(&self) -> watch::Receiver<SimReadModel> {
        self.models.subscribe()
    }
}

#[derive(Clone)]
struct ReadApiState {
    models: watch::Receiver<SimReadModel>,
}

#[derive(Clone)]
struct DevControlApiState {
    sim: Arc<Mutex<SimDevControl>>,
    publisher: ReadModelPublisher,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServeFloorPreviewRequest {
    operation: String,
    car: String,
    floor: i16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServeFloorSubmitRequest {
    submission_key: String,
    operation: String,
    car: String,
    floor: i16,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmissionQueryParams {
    expected_instance_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct WireControlError {
    control_api_version: &'static str,
    development_only: bool,
    control_instance_id: String,
    error_code: &'static str,
    submission_key: Option<String>,
    message: String,
}

type JsonControlResponse = (StatusCode, HeaderMap, Json<serde_json::Value>);

fn no_store_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers
}

pub(crate) fn serve(
    listener: TcpListener,
    sim: Arc<Mutex<SimDevControl>>,
    publisher: ReadModelPublisher,
    control_enabled: bool,
    shutdown: Arc<AtomicBool>,
) -> Result<(), String> {
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect listener address: {error}"))?;
    if control_enabled && !local_addr.ip().is_loopback() {
        return Err(format!(
            "refusing to expose development-only Simulator control surface on non-loopback address {local_addr}"
        ));
    }
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("failed to configure listener: {error}"))?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to create API runtime: {error}"))?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(|error| format!("failed to adopt TCP listener: {error}"))?;
        let mut upstream = publisher.subscribe();
        let (models_tx, models_rx) = watch::channel(upstream.borrow_and_update().clone());
        let relay_shutdown = Arc::clone(&shutdown);
        let relay_publisher = publisher.clone();
        tokio::spawn(async move {
            let _publisher_guard = relay_publisher;
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

        let app = build_router(models_rx, sim, publisher, control_enabled);
        let drain_deadline = async {
            wait_for_shutdown(Arc::clone(&shutdown)).await;
            tokio::time::sleep(SHUTDOWN_DRAIN_TIMEOUT).await;
        };
        let server = axum::serve(listener, app)
            .with_graceful_shutdown(wait_for_shutdown(Arc::clone(&shutdown)));
        tokio::select! {
            result = server.into_future() =>
                result.map_err(|error| format!("simulator server failed: {error}")),
            () = drain_deadline => {
                Ok(())
            }
        }
    })
}

fn build_router(
    models: watch::Receiver<SimReadModel>,
    sim: Arc<Mutex<SimDevControl>>,
    publisher: ReadModelPublisher,
    control_enabled: bool,
) -> Router {
    let read_router = Router::new()
        .route(SNAPSHOT_PATH, get(snapshot))
        .route(EVENTS_PATH, get(events))
        .with_state(ReadApiState { models });

    if !control_enabled {
        return read_router;
    }

    let control_router = Router::new()
        .route(DEV_CAPABILITIES_PATH, get(capabilities))
        .route(DEV_SERVE_FLOOR_PREVIEW_PATH, post(serve_floor_preview))
        .route(DEV_SERVE_FLOOR_SUBMIT_PATH, post(serve_floor_submit))
        .route(DEV_SUBMISSION_QUERY_PATH, get(query_submission))
        .layer(DefaultBodyLimit::max(MAX_CONTROL_BODY_BYTES))
        .with_state(DevControlApiState { sim, publisher });

    read_router.merge(control_router)
}

async fn snapshot(State(state): State<ReadApiState>) -> (HeaderMap, Json<SimReadModel>) {
    (no_store_headers(), Json(state.models.borrow().clone()))
}

async fn events(
    State(state): State<ReadApiState>,
) -> (
    HeaderMap,
    Sse<impl tokio_stream::Stream<Item = Result<Event, serde_json::Error>> + Send + 'static>,
) {
    let snapshots = WatchStream::new(state.models).map(|model| {
        serde_json::to_string(&model).map(|json| Event::default().event("snapshot").data(json))
    });
    (
        no_store_headers(),
        Sse::new(snapshots).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        ),
    )
}

async fn capabilities(State(state): State<DevControlApiState>) -> JsonControlResponse {
    let guard = state
        .sim
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let response = guard.capabilities();
    ok_json(&response)
}

async fn serve_floor_preview(
    State(state): State<DevControlApiState>,
    payload: Result<Json<ServeFloorPreviewRequest>, JsonRejection>,
) -> JsonControlResponse {
    let guard = state
        .sim
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let instance_id = guard.control_instance_id().to_owned();
    let Json(body) = match payload {
        Ok(body) => body,
        Err(rejection) => return json_rejection_response(&instance_id, rejection),
    };
    if let Err(error) = validate_operation(&body.operation) {
        return validation_error_response(&instance_id, error);
    }
    match guard.preview_serve_floor(&body.car, body.floor) {
        Ok(preview) => ok_json(&preview),
        Err(error) => validation_error_response(&instance_id, error),
    }
}

async fn serve_floor_submit(
    State(state): State<DevControlApiState>,
    payload: Result<Json<ServeFloorSubmitRequest>, JsonRejection>,
) -> JsonControlResponse {
    let mut guard = state
        .sim
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let instance_id = guard.control_instance_id().to_owned();
    let Json(body) = match payload {
        Ok(body) => body,
        Err(rejection) => return json_rejection_response(&instance_id, rejection),
    };
    if let Err(error) = validate_operation(&body.operation) {
        return validation_error_response(&instance_id, error);
    }
    match guard.submit_serve_floor(&body.submission_key, &body.car, body.floor) {
        Ok(projected) => {
            let _ = state.publisher.publish(guard.read_model());
            ok_json(&projected)
        }
        Err(SubmitError::Validation(error)) => validation_error_response(&instance_id, error),
        Err(SubmitError::Conflict {
            submission_key,
            message,
        }) => error_json(
            StatusCode::CONFLICT,
            &instance_id,
            "submission_key_conflict",
            Some(submission_key),
            message,
        ),
    }
}

async fn query_submission(
    State(state): State<DevControlApiState>,
    Path(submission_key): Path<String>,
    query: Result<Query<SubmissionQueryParams>, axum::extract::rejection::QueryRejection>,
) -> JsonControlResponse {
    let guard = state
        .sim
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let instance_id = guard.control_instance_id().to_owned();
    let Query(params) = match query {
        Ok(params) => params,
        Err(rejection) => {
            return error_json(
                StatusCode::BAD_REQUEST,
                &instance_id,
                "invalid_query",
                Some(submission_key),
                rejection.body_text(),
            );
        }
    };
    match guard.query_submission(&submission_key, params.expected_instance_id.as_deref()) {
        Ok(projected) => ok_json(&projected),
        Err(error) => validation_error_response(&instance_id, error),
    }
}

fn ok_json<T: Serialize>(value: &T) -> JsonControlResponse {
    (
        StatusCode::OK,
        no_store_headers(),
        Json(serde_json::to_value(value).expect("control DTO serializes to JSON")),
    )
}

fn validation_error_response(instance_id: &str, error: ValidationError) -> JsonControlResponse {
    error_json(
        StatusCode::BAD_REQUEST,
        instance_id,
        error.code,
        None,
        error.message,
    )
}

fn json_rejection_response(instance_id: &str, rejection: JsonRejection) -> JsonControlResponse {
    let status = rejection.status();
    let code = if status == StatusCode::PAYLOAD_TOO_LARGE {
        "payload_too_large"
    } else if status == StatusCode::UNSUPPORTED_MEDIA_TYPE {
        "unsupported_media_type"
    } else {
        "invalid_request_json"
    };
    error_json(status, instance_id, code, None, rejection.body_text())
}

fn error_json(
    status: StatusCode,
    instance_id: &str,
    error_code: &'static str,
    submission_key: Option<String>,
    message: String,
) -> JsonControlResponse {
    let body = WireControlError {
        control_api_version: DEV_CONTROL_API_VERSION,
        development_only: true,
        control_instance_id: instance_id.to_owned(),
        error_code,
        submission_key,
        message,
    };
    (
        status,
        no_store_headers(),
        Json(serde_json::to_value(body).expect("error DTO serializes to JSON")),
    )
}

async fn wait_for_shutdown(shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
