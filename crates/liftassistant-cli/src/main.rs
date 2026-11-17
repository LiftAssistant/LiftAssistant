#![forbid(unsafe_code)]

//! Minimal independent client for the experimental read-only Host API.

use std::{
    env,
    io::{BufRead, BufReader},
    process::ExitCode,
    time::Duration,
};

use reqwest::{
    Url,
    blocking::{Client, Response},
    header::{ACCEPT, CONTENT_TYPE},
};
use serde::Deserialize;

const API_VERSION: &str = "experimental/v0";
const SNAPSHOT_PATH: &str = "/api/experimental/v0/snapshot";
const EVENTS_PATH: &str = "/api/experimental/v0/events";

#[derive(Debug, Eq, PartialEq)]
enum CliCommand {
    Snapshot {
        base_url: Url,
    },
    Watch {
        base_url: Url,
        max_snapshots: Option<usize>,
    },
}

#[derive(Debug, Deserialize)]
struct ReadModel {
    api_version: String,
    revision: u64,
    lifecycle: String,
    readiness: String,
    operational_cars: Vec<Car>,
    sources: Vec<Source>,
    recent_diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Deserialize)]
struct Car {
    id: String,
    location: Location,
    motion: Motion,
    door: String,
    operating_mode: String,
    reported_destination: ReportedDestination,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Location {
    Floor {
        floor: i16,
    },
    Continuous {
        floor_position: f64,
        nearest_floor: i16,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Motion {
    Stopped,
    Moving { direction: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ReportedDestination {
    Known { floor: i16 },
    None,
    Unknown,
}

#[derive(Debug, Deserialize)]
struct Source {
    source: String,
    session: SourceSession,
    transport: String,
    payload: String,
    observations: Vec<SourceObservation>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum SourceSession {
    Inactive,
    Active { session: String },
}

#[derive(Debug, Deserialize)]
struct SourceObservation {
    car: String,
    role: String,
    availability: String,
}

#[derive(Debug, Deserialize)]
struct Diagnostic {
    sequence: u64,
    layer: String,
    id: String,
    source: Option<String>,
    session: Option<String>,
    car: Option<String>,
    role: Option<String>,
    native_entity: Option<String>,
}

fn main() -> ExitCode {
    match parse_command(env::args().skip(1)).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: CliCommand) -> Result<(), String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| format!("failed to create HTTP client: {error}"))?;
    match command {
        CliCommand::Snapshot { base_url } => {
            let model = fetch_snapshot(&client, &base_url)?;
            println!("{}", render(&model));
            Ok(())
        }
        CliCommand::Watch {
            base_url,
            max_snapshots,
        } => watch(&client, &base_url, max_snapshots),
    }
}

fn parse_command(args: impl Iterator<Item = String>) -> Result<CliCommand, String> {
    let args = args.collect::<Vec<_>>();
    match args.as_slice() {
        [command, base_url] if command == "snapshot" => Ok(CliCommand::Snapshot {
            base_url: parse_base_url(base_url)?,
        }),
        [command, base_url] if command == "watch" => Ok(CliCommand::Watch {
            base_url: parse_base_url(base_url)?,
            max_snapshots: None,
        }),
        [command, base_url, flag, count] if command == "watch" && flag == "--max-snapshots" => {
            let max_snapshots = count
                .parse::<usize>()
                .map_err(|error| format!("--max-snapshots must be a positive integer: {error}"))?;
            if max_snapshots == 0 {
                return Err("--max-snapshots must be greater than zero".into());
            }
            Ok(CliCommand::Watch {
                base_url: parse_base_url(base_url)?,
                max_snapshots: Some(max_snapshots),
            })
        }
        _ => Err(usage()),
    }
}

fn parse_base_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|error| format!("invalid Host base URL: {error}"))?;
    if url.scheme() != "http" {
        return Err("Host base URL must use explicit http://".into());
    }
    if url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Host base URL must contain only an http scheme, host, and optional port".into(),
        );
    }
    Ok(url)
}

fn endpoint(base_url: &Url, path: &str) -> Url {
    let mut url = base_url.clone();
    url.set_path(path);
    url
}

fn fetch_snapshot(client: &Client, base_url: &Url) -> Result<ReadModel, String> {
    let response = client
        .get(endpoint(base_url, SNAPSHOT_PATH))
        .header(ACCEPT, "application/json")
        .timeout(Duration::from_secs(10))
        .send()
        .map_err(|error| format!("snapshot request failed: {error}"))?;
    require_success_and_content_type(response, "application/json", "snapshot")?
        .json::<ReadModel>()
        .map_err(|error| format!("snapshot JSON was invalid: {error}"))
        .and_then(validate_model)
}

fn watch(client: &Client, base_url: &Url, max_snapshots: Option<usize>) -> Result<(), String> {
    let response = client
        .get(endpoint(base_url, EVENTS_PATH))
        .header(ACCEPT, "text/event-stream")
        .send()
        .map_err(|error| format!("SSE request failed: {error}"))?;
    let response = require_success_and_content_type(response, "text/event-stream", "SSE")?;
    consume_sse(BufReader::new(response), max_snapshots, |model| {
        println!("{}", render(&model));
    })?;
    Ok(())
}

fn require_success_and_content_type(
    response: Response,
    expected: &str,
    operation: &str,
) -> Result<Response, String> {
    let response = response
        .error_for_status()
        .map_err(|error| format!("{operation} endpoint rejected the request: {error}"))?;
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type.starts_with(expected) {
        return Err(format!(
            "{operation} endpoint returned unexpected content type {content_type:?}"
        ));
    }
    Ok(response)
}

fn consume_sse(
    reader: impl BufRead,
    max_snapshots: Option<usize>,
    mut consume: impl FnMut(ReadModel),
) -> Result<usize, String> {
    let mut event = None;
    let mut data = Vec::new();
    let mut snapshots = 0;
    for line in reader.lines() {
        let line = line.map_err(|error| format!("failed to read SSE stream: {error}"))?;
        if line.is_empty() {
            if event.as_deref() == Some("snapshot") && !data.is_empty() {
                let model = serde_json::from_str::<ReadModel>(&data.join("\n"))
                    .map_err(|error| format!("SSE snapshot JSON was invalid: {error}"))?;
                let model = validate_model(model)?;
                snapshots += 1;
                consume(model);
                if max_snapshots == Some(snapshots) {
                    return Ok(snapshots);
                }
            }
            event = None;
            data.clear();
        } else if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.strip_prefix(' ').unwrap_or(value).to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value).to_owned());
        }
    }
    Err(format!(
        "SSE stream ended after {snapshots} snapshot(s) before the requested watch completed"
    ))
}

fn validate_model(model: ReadModel) -> Result<ReadModel, String> {
    if model.api_version != API_VERSION {
        return Err(format!(
            "unsupported API version {:?}; this CLI expects {API_VERSION}",
            model.api_version
        ));
    }
    Ok(model)
}

fn render(model: &ReadModel) -> String {
    let mut lines = vec![
        format!(
            "revision={} lifecycle={} readiness={}",
            model.revision, model.lifecycle, model.readiness
        ),
        format!("cars={}", model.operational_cars.len()),
    ];
    for car in &model.operational_cars {
        lines.push(format!(
            "  car={} location={} motion={} door={} mode={} destination={}",
            car.id,
            render_location(&car.location),
            render_motion(&car.motion),
            car.door,
            car.operating_mode,
            render_destination(&car.reported_destination)
        ));
    }
    lines.push(format!("sources={}", model.sources.len()));
    for source in &model.sources {
        lines.push(format!(
            "  source={} session={} transport={} payload={}",
            source.source,
            render_session(&source.session),
            source.transport,
            source.payload
        ));
        for observation in &source.observations {
            lines.push(format!(
                "    car={} role={} availability={}",
                observation.car, observation.role, observation.availability
            ));
        }
    }
    lines.push(format!(
        "recent_diagnostics={}",
        model.recent_diagnostics.len()
    ));
    for diagnostic in &model.recent_diagnostics {
        lines.push(format!(
            "  diagnostic={} layer={} id={} source={} session={} car={} role={} native_entity={}",
            diagnostic.sequence,
            diagnostic.layer,
            diagnostic.id,
            diagnostic.source.as_deref().unwrap_or("-"),
            diagnostic.session.as_deref().unwrap_or("-"),
            diagnostic.car.as_deref().unwrap_or("-"),
            diagnostic.role.as_deref().unwrap_or("-"),
            diagnostic.native_entity.as_deref().unwrap_or("-")
        ));
    }
    lines.join("\n")
}

fn render_location(location: &Location) -> String {
    match location {
        Location::Floor { floor } => format!("floor:{floor}"),
        Location::Continuous {
            floor_position,
            nearest_floor,
        } => format!("continuous:{floor_position} nearest:{nearest_floor}"),
    }
}

fn render_motion(motion: &Motion) -> String {
    match motion {
        Motion::Stopped => "stopped".into(),
        Motion::Moving { direction } => format!("moving:{direction}"),
    }
}

fn render_destination(destination: &ReportedDestination) -> String {
    match destination {
        ReportedDestination::Known { floor } => format!("known:{floor}"),
        ReportedDestination::None => "none".into(),
        ReportedDestination::Unknown => "unknown".into(),
    }
}

fn render_session(session: &SourceSession) -> String {
    match session {
        SourceSession::Inactive => "inactive".into(),
        SourceSession::Active { session } => format!("active:{session}"),
    }
}

fn usage() -> String {
    "usage: liftassistant-cli snapshot <http://host:port>\n       liftassistant-cli watch <http://host:port> [--max-snapshots <positive-integer>]".into()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const FIXTURE: &str = r#"{"api_version":"experimental/v0","revision":7,"lifecycle":"ready_read_only","readiness":"read_only","operational_cars":[{"id":"A","location":{"kind":"floor","floor":3},"motion":{"state":"stopped"},"door":"closed","operating_mode":"normal","reported_destination":{"kind":"known","floor":3}}],"sources":[{"source":"rmf/site","session":{"state":"active","session":"session-A"},"transport":"discovery_observed","payload":"active","observations":[{"car":"A","role":"operational","availability":"resolved"}]}],"recent_diagnostics":[{"sequence":4,"layer":"readiness","id":"readiness.changed","source":null,"session":null,"car":null,"role":null,"native_entity":null}]}"#;

    #[test]
    fn parser_and_renderer_cover_the_required_client_view() {
        let model = validate_model(serde_json::from_str(FIXTURE).expect("fixture parses"))
            .expect("version is supported");
        let rendered = render(&model);
        for expected in [
            "revision=7 lifecycle=ready_read_only readiness=read_only",
            "car=A location=floor:3",
            "source=rmf/site session=active:session-A",
            "car=A role=operational availability=resolved",
            "diagnostic=4 layer=readiness id=readiness.changed",
        ] {
            assert!(rendered.contains(expected), "missing {expected:?}");
        }
    }

    #[test]
    fn sse_parser_ignores_keepalive_and_consumes_complete_snapshots() {
        let stream = format!(
            ": keepalive\n\nevent: snapshot\ndata: {FIXTURE}\n\nevent: snapshot\ndata: {}\n\n",
            FIXTURE.replace("\"revision\":7", "\"revision\":8")
        );
        let mut revisions = Vec::new();
        let count = consume_sse(Cursor::new(stream), Some(2), |model| {
            revisions.push(model.revision);
        })
        .expect("two snapshots complete the bounded watch");
        assert_eq!(count, 2);
        assert_eq!(revisions, vec![7, 8]);
    }

    #[test]
    fn cli_requires_explicit_http_host_and_positive_watch_bound() {
        assert_eq!(
            parse_command(
                ["snapshot", "http://127.0.0.1:8080"]
                    .into_iter()
                    .map(str::to_owned),
            )
            .expect("snapshot command parses"),
            CliCommand::Snapshot {
                base_url: Url::parse("http://127.0.0.1:8080").expect("URL parses")
            }
        );
        assert!(
            parse_command(
                ["watch", "https://example.test"]
                    .into_iter()
                    .map(str::to_owned),
            )
            .expect_err("TLS is not claimed")
            .contains("http://")
        );
        assert!(
            parse_command(
                ["watch", "http://127.0.0.1:8080", "--max-snapshots", "0"]
                    .into_iter()
                    .map(str::to_owned),
            )
            .expect_err("zero is not a useful watch bound")
            .contains("greater than zero")
        );
    }
}
