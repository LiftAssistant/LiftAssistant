#![forbid(unsafe_code)]

#[cfg(test)]
mod failure_matrix;
mod http_api;
mod incident;
mod read_api;
mod rmf_host;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, io,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    process::{Command, ExitCode},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use liftassistant_engine::{
    DiagnosticAnomaly, DiagnosticEvent, Engine, ObservationBindingError, ObservationBindingRole,
    ObservationIngestError, ObservationIngestResult, SourceObservationInvalidation,
    SourceObservationInvalidationError, SourceSessionError, SourceSessionTransition,
};
use liftassistant_model::{
    BuildingTopology, CarId, CarLocation, CarObservation, CarObservationUpdate, DoorEvidence,
    DoorState, Floor, MotionEvidence, MotionState, ObservationProvenance, ObservationSequence,
    ObservationSessionId, ObservationSourceId, OperatingMode, OperatingModeEvidence, Position,
    PositionEvidence, ReportedDestination,
};
use liftassistant_rmf::{
    RmfFreshnessPolicy, RmfInteractionModeProjection, RmfLiftBinding, RmfLiftState,
    RmfLiveSubscriber, RmfObservationAdapter, RmfReceiveControl, RmfReceiveOutcome,
    RmfSourceContext,
};
use liftassistant_runtime::{CarStateResolution, UnresolvedCarStateReason};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    flag,
};

use incident::{CapsuleTrigger, IncidentCapsule, IncidentRecorder};

const LIFECYCLE_SCENARIO: &str = "headless-read-only-lifecycle";
const RMF_LIVE_SCENARIO: &str = "headless-rmf-live-proof";
const RMF_FRESHNESS_SCENARIO: &str = "headless-rmf-freshness-proof";
const INCIDENT_CAPSULE_SCENARIO: &str = "headless-incident-capsule-replay";
const SANITIZED_CAPSULE_SCENARIO: &str = "headless-sanitized-capsule-replay";
const READ_MODEL_SCENARIO: &str = "headless-experimental-read-model";
const RMF_READ_ONLY_HOST_COMMAND: &str = "--run-rmf-read-only";
const REPLAY_INCIDENT_CAPSULE_COMMAND: &str = "--replay-incident-capsule";
const SANITIZE_INCIDENT_CAPSULE_COMMAND: &str = "--sanitize-incident-capsule";
const SERVE_SNAPSHOT_FIXTURE_COMMAND: &str = "--serve-experimental-snapshot-fixture";
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(100);
const HOST_DIAGNOSTIC_CAPACITY: usize = 256;
const INCIDENT_ACTION_CAPACITY: usize = 128;
const MAX_DDS_DOMAIN_ID: u16 = 232;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Readiness {
    NotReady,
    ReadOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DegradedReason {
    ObservationAnomaly(DiagnosticAnomaly),
    ObservationRejected(ObservationIngestError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HostLifecycle {
    Starting,
    Synchronizing,
    ReadyReadOnly,
    Degraded(DegradedReason),
}

impl HostLifecycle {
    fn readiness(&self) -> Readiness {
        match self {
            Self::ReadyReadOnly => Readiness::ReadOnly,
            Self::Starting | Self::Synchronizing | Self::Degraded(_) => Readiness::NotReady,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum HostEvent {
    LifecycleChanged(HostLifecycle),
    TransportSubscriberCreated {
        source: ObservationSourceId,
        session: ObservationSessionId,
    },
    TransportDiscoveryObserved {
        source: ObservationSourceId,
        session: ObservationSessionId,
    },
    TransportFailed {
        source: ObservationSourceId,
        session: ObservationSessionId,
        message: String,
    },
    SourceSessionActivated {
        source: ObservationSourceId,
        session: ObservationSessionId,
        previous_session: Option<ObservationSessionId>,
        invalidated_cars: Vec<CarId>,
    },
    MappingRejected {
        source: ObservationSourceId,
        session: ObservationSessionId,
        native_entity: String,
        message: String,
    },
    ObservationIncomplete {
        source: ObservationSourceId,
        session: ObservationSessionId,
        car: CarId,
        role: ObservationBindingRole,
    },
    ObservationUnresolved {
        source: ObservationSourceId,
        session: ObservationSessionId,
        car: CarId,
        reasons: Vec<UnresolvedCarStateReason>,
        role: ObservationBindingRole,
    },
    StateUpdated {
        source: ObservationSourceId,
        session: ObservationSessionId,
        state: liftassistant_model::CarState,
    },
    ObservationRejected {
        source: ObservationSourceId,
        session: ObservationSessionId,
        car: CarId,
        role: Option<ObservationBindingRole>,
        anomaly: DiagnosticAnomaly,
    },
    IngestionRejected {
        source: ObservationSourceId,
        session: ObservationSessionId,
        car: CarId,
        role: Option<ObservationBindingRole>,
        error: ObservationIngestError,
    },
    AuxiliaryObservationAccepted {
        source: ObservationSourceId,
        session: ObservationSessionId,
        car: CarId,
        availability: ObservationAvailability,
    },
    SourcePayloadStale {
        source: ObservationSourceId,
        session: ObservationSessionId,
        invalidated_cars: Vec<CarId>,
    },
    CarObservationsRevoked {
        source: ObservationSourceId,
        session: ObservationSessionId,
        car: CarId,
        role: ObservationBindingRole,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostDiagnosticLayer {
    Transport,
    Source,
    Mapping,
    Observation,
    Readiness,
}

impl HostDiagnosticLayer {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::Source => "source",
            Self::Mapping => "mapping",
            Self::Observation => "observation",
            Self::Readiness => "readiness",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HostDiagnosticId {
    TransportSubscriberCreated,
    TransportDiscoveryObserved,
    TransportFailed,
    SourceSessionActivated,
    SourcePayloadStale,
    CarObservationsRevoked,
    MappingRejected,
    ObservationIncomplete,
    ObservationUnresolved,
    ObservationResolved,
    ObservationRejected,
    AuxiliaryObservationAccepted,
    ReadinessChanged,
}

impl HostDiagnosticId {
    const fn as_str(self) -> &'static str {
        match self {
            Self::TransportSubscriberCreated => "transport.subscriber-created",
            Self::TransportDiscoveryObserved => "transport.discovery-observed",
            Self::TransportFailed => "transport.receive-failed",
            Self::SourceSessionActivated => "source.session-activated",
            Self::SourcePayloadStale => "source.payload-stale",
            Self::CarObservationsRevoked => "observation.revoked",
            Self::MappingRejected => "mapping.rejected",
            Self::ObservationIncomplete => "observation.incomplete",
            Self::ObservationUnresolved => "observation.unresolved",
            Self::ObservationResolved => "observation.resolved",
            Self::ObservationRejected => "observation.rejected",
            Self::AuxiliaryObservationAccepted => "observation.auxiliary-accepted",
            Self::ReadinessChanged => "readiness.changed",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct HostDiagnosticContext {
    source: Option<ObservationSourceId>,
    session: Option<ObservationSessionId>,
    car: Option<CarId>,
    role: Option<ObservationBindingRole>,
    native_entity: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct HostDiagnosticEntry {
    sequence: u64,
    layer: HostDiagnosticLayer,
    id: HostDiagnosticId,
    context: HostDiagnosticContext,
    event: HostEvent,
}

#[derive(Clone, Debug, PartialEq)]
struct HostDiagnostics {
    capacity: usize,
    next_sequence: u64,
    entries: VecDeque<HostDiagnosticEntry>,
}

impl HostDiagnostics {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            next_sequence: 1,
            entries: VecDeque::new(),
        }
    }

    fn record(&mut self, event: HostEvent) {
        let (layer, id, context) = classify_host_event(&event);
        let entry = HostDiagnosticEntry {
            sequence: self.next_sequence,
            layer,
            id,
            context,
            event,
        };
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("host diagnostic sequence space exhausted");
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    fn entries(&self) -> impl Iterator<Item = &HostDiagnosticEntry> {
        self.entries.iter()
    }
}

fn classify_host_event(
    event: &HostEvent,
) -> (HostDiagnosticLayer, HostDiagnosticId, HostDiagnosticContext) {
    let mut context = HostDiagnosticContext::default();
    let (layer, id) = match event {
        HostEvent::LifecycleChanged(_) => (
            HostDiagnosticLayer::Readiness,
            HostDiagnosticId::ReadinessChanged,
        ),
        HostEvent::TransportSubscriberCreated { source, session } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            (
                HostDiagnosticLayer::Transport,
                HostDiagnosticId::TransportSubscriberCreated,
            )
        }
        HostEvent::TransportDiscoveryObserved { source, session } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            (
                HostDiagnosticLayer::Transport,
                HostDiagnosticId::TransportDiscoveryObserved,
            )
        }
        HostEvent::TransportFailed {
            source, session, ..
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            (
                HostDiagnosticLayer::Transport,
                HostDiagnosticId::TransportFailed,
            )
        }
        HostEvent::SourceSessionActivated {
            source, session, ..
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            (
                HostDiagnosticLayer::Source,
                HostDiagnosticId::SourceSessionActivated,
            )
        }
        HostEvent::SourcePayloadStale {
            source, session, ..
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            (
                HostDiagnosticLayer::Source,
                HostDiagnosticId::SourcePayloadStale,
            )
        }
        HostEvent::CarObservationsRevoked {
            source,
            session,
            car,
            role,
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            context.car = Some(car.clone());
            context.role = Some(*role);
            (
                HostDiagnosticLayer::Observation,
                HostDiagnosticId::CarObservationsRevoked,
            )
        }
        HostEvent::MappingRejected {
            source,
            session,
            native_entity,
            ..
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            context.native_entity = Some(native_entity.clone());
            (
                HostDiagnosticLayer::Mapping,
                HostDiagnosticId::MappingRejected,
            )
        }
        HostEvent::ObservationIncomplete {
            source,
            session,
            car,
            ..
        }
        | HostEvent::ObservationUnresolved {
            source,
            session,
            car,
            ..
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            context.car = Some(car.clone());
            context.role = Some(match event {
                HostEvent::ObservationIncomplete { role, .. }
                | HostEvent::ObservationUnresolved { role, .. } => *role,
                _ => unreachable!(),
            });
            (
                HostDiagnosticLayer::Observation,
                match event {
                    HostEvent::ObservationIncomplete { .. } => {
                        HostDiagnosticId::ObservationIncomplete
                    }
                    _ => HostDiagnosticId::ObservationUnresolved,
                },
            )
        }
        HostEvent::StateUpdated {
            source,
            session,
            state,
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            context.car = Some(state.id.clone());
            context.role = Some(ObservationBindingRole::Operational);
            (
                HostDiagnosticLayer::Observation,
                HostDiagnosticId::ObservationResolved,
            )
        }
        HostEvent::ObservationRejected {
            source,
            session,
            car,
            role,
            ..
        }
        | HostEvent::IngestionRejected {
            source,
            session,
            car,
            role,
            ..
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            context.car = Some(car.clone());
            context.role = *role;
            (
                HostDiagnosticLayer::Observation,
                HostDiagnosticId::ObservationRejected,
            )
        }
        HostEvent::AuxiliaryObservationAccepted {
            source,
            session,
            car,
            ..
        } => {
            context.source = Some(source.clone());
            context.session = Some(session.clone());
            context.car = Some(car.clone());
            context.role = Some(ObservationBindingRole::Auxiliary);
            (
                HostDiagnosticLayer::Observation,
                HostDiagnosticId::AuxiliaryObservationAccepted,
            )
        }
    };
    (layer, id, context)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransportDiscoveryState {
    Unknown,
    SubscriberCreated,
    DiscoveryObserved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PayloadActivity {
    Awaiting,
    Active,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SourceSessionActivity {
    Inactive,
    Active(ObservationSessionId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservationAvailability {
    Awaiting,
    Incomplete,
    Unresolved,
    Resolved,
    Revoked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceHealth {
    transport_discovery: TransportDiscoveryState,
    payload: PayloadActivity,
    session: SourceSessionActivity,
    observations: BTreeMap<CarId, ObservationAvailability>,
}

impl SourceHealth {
    fn new() -> Self {
        Self {
            transport_discovery: TransportDiscoveryState::Unknown,
            payload: PayloadActivity::Awaiting,
            session: SourceSessionActivity::Inactive,
            observations: BTreeMap::new(),
        }
    }
}

/// Internal in-memory source for the headless proof. It exposes observations only.
#[derive(Debug)]
struct RecordedObservationFeed {
    frames: VecDeque<CarObservationUpdate>,
}

impl RecordedObservationFeed {
    fn new<T>(frames: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<CarObservationUpdate>,
    {
        Self {
            frames: frames.into_iter().map(Into::into).collect(),
        }
    }

    fn next_observation(&mut self) -> Option<CarObservationUpdate> {
        self.frames.pop_front()
    }
}

#[derive(Debug)]
struct ReadOnlyHost {
    engine: Engine,
    required_operational_cars: BTreeSet<CarId>,
    source_health: BTreeMap<ObservationSourceId, SourceHealth>,
    lifecycle: HostLifecycle,
    diagnostics: HostDiagnostics,
    incident_recorder: IncidentRecorder,
    read_model_revision: u64,
}

impl ReadOnlyHost {
    fn new(topology: BuildingTopology) -> Self {
        Self {
            engine: Engine::new(topology),
            required_operational_cars: BTreeSet::new(),
            source_health: BTreeMap::new(),
            lifecycle: HostLifecycle::Starting,
            diagnostics: {
                let mut diagnostics = HostDiagnostics::new(HOST_DIAGNOSTIC_CAPACITY);
                diagnostics.record(HostEvent::LifecycleChanged(HostLifecycle::Starting));
                diagnostics
            },
            incident_recorder: IncidentRecorder::new(INCIDENT_ACTION_CAPACITY),
            read_model_revision: 1,
        }
    }

    fn lifecycle(&self) -> &HostLifecycle {
        &self.lifecycle
    }

    fn readiness(&self) -> Readiness {
        self.lifecycle.readiness()
    }

    fn source_health(&self, source: &ObservationSourceId) -> Option<&SourceHealth> {
        self.source_health.get(source)
    }

    fn diagnostics(&self) -> &HostDiagnostics {
        &self.diagnostics
    }

    fn read_model(&self) -> read_api::ExperimentalReadModel {
        read_api::ExperimentalReadModel::project(self)
    }

    fn bump_read_model_revision(&mut self) {
        self.read_model_revision = self
            .read_model_revision
            .checked_add(1)
            .expect("experimental read-model revision space exhausted");
    }

    fn bind_observation(
        &mut self,
        source: ObservationSourceId,
        car: CarId,
        role: ObservationBindingRole,
    ) -> Result<(), ObservationBindingError> {
        self.engine
            .bind_observation(source.clone(), car.clone(), role)?;
        self.incident_recorder.record_binding(&source, &car, role);
        self.source_health
            .entry(source)
            .or_insert_with(SourceHealth::new)
            .observations
            .insert(car.clone(), ObservationAvailability::Awaiting);
        if role == ObservationBindingRole::Operational {
            self.required_operational_cars.insert(car);
        }
        self.bump_read_model_revision();
        Ok(())
    }

    fn poll(&mut self, source: &mut RecordedObservationFeed) -> bool {
        let Some(observation) = source.next_observation() else {
            return false;
        };
        self.ingest_observation(observation);
        true
    }

    fn ingest_observation(&mut self, observation: CarObservationUpdate) {
        self.bump_read_model_revision();
        let observation_source = observation.provenance.source.clone();
        let observation_session = observation.provenance.session.clone();
        let car = observation.car.clone();
        let role = self
            .engine
            .observation_binding_role(&observation_source, &car);
        if role.is_some()
            && self
                .engine
                .active_source_session(&observation_source)
                .is_some()
        {
            self.incident_recorder.record_observation(&observation);
        }
        let is_operational = role == Some(ObservationBindingRole::Operational);

        if is_operational
            && matches!(
                self.lifecycle,
                HostLifecycle::Starting | HostLifecycle::Degraded(_)
            )
        {
            self.transition(HostLifecycle::Synchronizing);
        }

        let incident_marker = latest_incident_marker(&self.engine);
        let resolution = self.engine.ingest_observation_update(observation);
        if latest_incident_marker(&self.engine) != incident_marker {
            let anomaly = self
                .engine
                .diagnostics()
                .incidents()
                .last()
                .expect("a new incident was counted")
                .trigger
                .clone();
            self.diagnostics.record(HostEvent::ObservationRejected {
                source: observation_source,
                session: observation_session,
                car,
                role,
                anomaly: anomaly.clone(),
            });
            if is_operational {
                self.transition(HostLifecycle::Degraded(DegradedReason::ObservationAnomaly(
                    anomaly,
                )));
            }
            return;
        }

        let accepted_availability = observation_availability(&resolution);
        if let Some((availability, health)) =
            accepted_availability.zip(self.source_health.get_mut(&observation_source))
        {
            health.payload = PayloadActivity::Active;
            health.observations.insert(car.clone(), availability);
        }

        if !is_operational {
            match resolution {
                ObservationIngestResult::Rejected(error) => {
                    self.diagnostics.record(HostEvent::IngestionRejected {
                        source: observation_source,
                        session: observation_session,
                        car,
                        role,
                        error,
                    });
                }
                ObservationIngestResult::Incomplete(_) | ObservationIngestResult::Resolution(_) => {
                    self.diagnostics
                        .record(HostEvent::AuxiliaryObservationAccepted {
                            source: observation_source,
                            session: observation_session,
                            car,
                            availability: accepted_availability
                                .expect("accepted auxiliary observation has availability"),
                        });
                }
            }
            return;
        }

        match resolution {
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(state)) => {
                self.diagnostics.record(HostEvent::StateUpdated {
                    source: observation_source,
                    session: observation_session,
                    state,
                });
                let all_operational_cars_resolved = self
                    .required_operational_cars
                    .iter()
                    .all(|car| self.engine.operational_car_state(car).is_some());
                if all_operational_cars_resolved {
                    self.transition(HostLifecycle::ReadyReadOnly);
                } else if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
            }
            ObservationIngestResult::Resolution(CarStateResolution::Unresolved {
                car,
                reasons,
            }) => {
                self.diagnostics.record(HostEvent::ObservationUnresolved {
                    source: observation_source,
                    session: observation_session,
                    car,
                    reasons,
                    role: ObservationBindingRole::Operational,
                });
                if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
            }
            ObservationIngestResult::Incomplete(assembled) => {
                self.diagnostics.record(HostEvent::ObservationIncomplete {
                    source: observation_source,
                    session: observation_session,
                    car,
                    role: ObservationBindingRole::Operational,
                });
                if !matches!(self.lifecycle, HostLifecycle::Synchronizing) {
                    self.transition(HostLifecycle::Synchronizing);
                }
                debug_assert!(assembled.snapshot().is_none());
            }
            ObservationIngestResult::Rejected(error) => {
                self.diagnostics.record(HostEvent::IngestionRejected {
                    source: observation_source,
                    session: observation_session,
                    car,
                    role,
                    error: error.clone(),
                });
                self.transition(HostLifecycle::Degraded(
                    DegradedReason::ObservationRejected(error),
                ));
            }
        }
    }

    fn begin_source_session(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
    ) -> Result<SourceSessionTransition, SourceSessionError> {
        let transition = self.engine.begin_source_session(source, session)?;
        self.incident_recorder
            .record_begin_session(&transition.source, &transition.active_session);
        let health = self
            .source_health
            .get_mut(&transition.source)
            .expect("Engine accepts sessions only for a bound source");
        health.session = SourceSessionActivity::Active(transition.active_session.clone());
        health.payload = PayloadActivity::Awaiting;
        for availability in health.observations.values_mut() {
            *availability = ObservationAvailability::Awaiting;
        }
        let invalidated_operational_state = transition.invalidated_cars.iter().any(|car| {
            self.engine
                .observation_binding_role(&transition.source, car)
                == Some(ObservationBindingRole::Operational)
        });
        self.diagnostics.record(HostEvent::SourceSessionActivated {
            source: transition.source.clone(),
            session: transition.active_session.clone(),
            previous_session: transition.previous_session.clone(),
            invalidated_cars: transition.invalidated_cars.clone(),
        });
        if invalidated_operational_state {
            self.transition(HostLifecycle::Synchronizing);
        }
        self.bump_read_model_revision();
        Ok(transition)
    }

    fn note_transport_subscriber_created(&mut self, source: &ObservationSourceId) {
        let changed = if let Some(health) = self.source_health.get_mut(source) {
            if health.transport_discovery != TransportDiscoveryState::Unknown {
                return;
            }
            health.transport_discovery = TransportDiscoveryState::SubscriberCreated;
            let SourceSessionActivity::Active(session) = &health.session else {
                return self.bump_read_model_revision();
            };
            self.diagnostics
                .record(HostEvent::TransportSubscriberCreated {
                    source: source.clone(),
                    session: session.clone(),
                });
            true
        } else {
            false
        };
        if changed {
            self.bump_read_model_revision();
        }
    }

    fn note_transport_discovery(&mut self, source: &ObservationSourceId) {
        let changed = if let Some(health) = self.source_health.get_mut(source) {
            if health.transport_discovery == TransportDiscoveryState::DiscoveryObserved {
                return;
            }
            health.transport_discovery = TransportDiscoveryState::DiscoveryObserved;
            let SourceSessionActivity::Active(session) = &health.session else {
                return self.bump_read_model_revision();
            };
            self.diagnostics
                .record(HostEvent::TransportDiscoveryObserved {
                    source: source.clone(),
                    session: session.clone(),
                });
            true
        } else {
            false
        };
        if changed {
            self.bump_read_model_revision();
        }
    }

    fn record_transport_failure(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
        message: String,
    ) {
        self.diagnostics.record(HostEvent::TransportFailed {
            source,
            session,
            message,
        });
        self.bump_read_model_revision();
    }

    fn record_mapping_rejection(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
        native_entity: String,
        message: String,
    ) {
        self.diagnostics.record(HostEvent::MappingRejected {
            source,
            session,
            native_entity,
            message,
        });
        self.bump_read_model_revision();
    }

    fn mark_source_payload_stale(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
    ) -> Result<SourceObservationInvalidation, SourceObservationInvalidationError> {
        let invalidation = self
            .engine
            .invalidate_source_observations(source.clone(), session.clone())?;
        self.incident_recorder
            .record_invalidation(&source, &session);
        let health = self
            .source_health
            .get_mut(&source)
            .expect("Engine invalidates only a bound source");
        health.payload = PayloadActivity::Stale;
        for availability in health.observations.values_mut() {
            *availability = ObservationAvailability::Revoked;
        }
        self.diagnostics.record(HostEvent::SourcePayloadStale {
            source: source.clone(),
            session,
            invalidated_cars: invalidation.invalidated_cars.clone(),
        });
        let source_has_operational_binding = self
            .required_operational_cars
            .iter()
            .any(|car| self.engine.operational_observation_source(car) == Some(&source));
        if source_has_operational_binding {
            self.transition(HostLifecycle::Synchronizing);
        }
        self.bump_read_model_revision();
        Ok(invalidation)
    }

    fn transition(&mut self, lifecycle: HostLifecycle) {
        if self.lifecycle != lifecycle {
            self.lifecycle = lifecycle.clone();
            self.diagnostics
                .record(HostEvent::LifecycleChanged(lifecycle));
            self.bump_read_model_revision();
        }
    }

    fn revoke_car_observations(
        &mut self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
        car: &CarId,
    ) -> Result<(), SourceObservationInvalidationError> {
        let role = self
            .engine
            .invalidate_car_observations(source, session, car)?;
        self.incident_recorder
            .record_car_invalidation(source, session, car);
        let health = self
            .source_health
            .get_mut(source)
            .expect("bound car has source health");
        health
            .observations
            .insert(car.clone(), ObservationAvailability::Revoked);
        self.diagnostics.record(HostEvent::CarObservationsRevoked {
            source: source.clone(),
            session: session.clone(),
            car: car.clone(),
            role,
        });
        if role == ObservationBindingRole::Operational {
            self.transition(HostLifecycle::Synchronizing);
        }
        self.bump_read_model_revision();
        Ok(())
    }

    fn capture_incident(&self, trigger: CapsuleTrigger) -> IncidentCapsule {
        self.incident_recorder.capture(
            self.engine.topology(),
            &self.lifecycle,
            self.readiness(),
            trigger,
        )
    }
}

fn observation_availability(result: &ObservationIngestResult) -> Option<ObservationAvailability> {
    match result {
        ObservationIngestResult::Incomplete(_) => Some(ObservationAvailability::Incomplete),
        ObservationIngestResult::Resolution(CarStateResolution::Resolved(_)) => {
            Some(ObservationAvailability::Resolved)
        }
        ObservationIngestResult::Resolution(CarStateResolution::Unresolved { .. }) => {
            Some(ObservationAvailability::Unresolved)
        }
        ObservationIngestResult::Rejected(_) => None,
    }
}

fn latest_incident_marker(engine: &Engine) -> Option<u64> {
    engine
        .diagnostics()
        .incidents()
        .last()
        .and_then(|incident| {
            incident.evidence.iter().rev().find_map(|entry| {
                matches!(entry.event, DiagnosticEvent::Anomaly(_)).then_some(entry.sequence.value())
            })
        })
}

#[derive(Clone, Debug, PartialEq)]
struct LifecycleSnapshot {
    lifecycle: HostLifecycle,
    readiness: Readiness,
}

#[derive(Clone, Debug, PartialEq)]
struct ProofRun {
    snapshots: Vec<LifecycleSnapshot>,
    events: Vec<HostEvent>,
    latest_state: Option<liftassistant_model::CarState>,
    diagnostic_entries: usize,
    incidents: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RmfHostBinding {
    native_lift_name: String,
    logical_car: CarId,
    role: ObservationBindingRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RmfHostConfig {
    domain_id: u16,
    source: ObservationSourceId,
    max_payload_silence: Duration,
    floor_labels: BTreeMap<String, Floor>,
    bindings: Vec<RmfHostBinding>,
    agv_as_normal: bool,
    human_as_normal: bool,
    incident_capsule_out: Option<PathBuf>,
    api_listen: Option<SocketAddr>,
}

struct ExperimentalApiServer {
    publisher: http_api::ReadModelPublisher,
    shutdown: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ExperimentalApiServer {
    fn start(
        listen: SocketAddr,
        host: &ReadOnlyHost,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind(listen)
            .map_err(|error| format!("failed to bind experimental read-only API: {error}"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|error| format!("failed to inspect experimental API listener: {error}"))?;
        if !local_addr.ip().is_loopback() {
            eprintln!(
                "warning: experimental unauthenticated read-only API is exposed on {local_addr}"
            );
        }
        println!("experimental read-only API listening on http://{local_addr}");
        let publisher = http_api::ReadModelPublisher::new(host.read_model());
        let server_publisher = publisher.clone();
        let server_shutdown = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("liftassistant-experimental-api".into())
            .spawn(move || {
                if let Err(error) = http_api::serve(listener, server_publisher, server_shutdown) {
                    eprintln!("warning: experimental read-only API stopped: {error}");
                }
            })
            .map_err(|error| format!("failed to start experimental API thread: {error}"))?;
        Ok(Self {
            publisher,
            shutdown,
            thread: Some(thread),
        })
    }

    fn publish(&self, host: &ReadOnlyHost) {
        let _ = self.publisher.publish(host.read_model());
    }
}

impl Drop for ExperimentalApiServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                eprintln!("warning: experimental read-only API thread panicked");
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HeadlessCommand {
    Scenario(String),
    RunRmfReadOnly(RmfHostConfig),
    ReplayIncidentCapsule(PathBuf),
    SanitizeIncidentCapsule { input: PathBuf, output: PathBuf },
    ServeSnapshotFixture(SocketAddr),
}

fn main() -> ExitCode {
    match parse_command(env::args().skip(1)).and_then(run_command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_command(command: HeadlessCommand) -> Result<(), String> {
    match command {
        HeadlessCommand::Scenario(name) => {
            println!("{}", run_scenario(&name)?);
            Ok(())
        }
        HeadlessCommand::RunRmfReadOnly(config) => {
            let shutdown = Arc::new(AtomicBool::new(false));
            flag::register(SIGINT, Arc::clone(&shutdown))
                .map_err(|error| format!("failed to register SIGINT handler: {error}"))?;
            flag::register(SIGTERM, Arc::clone(&shutdown))
                .map_err(|error| format!("failed to register SIGTERM handler: {error}"))?;
            run_rmf_read_only_host(config, shutdown)
        }
        HeadlessCommand::ReplayIncidentCapsule(path) => {
            let report = IncidentCapsule::read(&path)?.replay()?;
            println!("{}", report.render());
            Ok(())
        }
        HeadlessCommand::SanitizeIncidentCapsule { input, output } => {
            let private = IncidentCapsule::read(&input)?;
            let private_replay = private.replay()?;
            let (candidate, report) = private.sanitize_candidate()?;
            let sanitized_replay = candidate.replay()?;
            if private_replay.trigger_id != sanitized_replay.trigger_id {
                return Err("sanitized candidate changed the deterministic trigger".into());
            }
            candidate.write_new(&output)?;
            println!("{}", report.render());
            println!("{}", sanitized_replay.render());
            println!("shareable candidate path={output:?} manual_review=REQUIRED");
            Ok(())
        }
        HeadlessCommand::ServeSnapshotFixture(listen) => {
            let shutdown = Arc::new(AtomicBool::new(false));
            flag::register(SIGINT, Arc::clone(&shutdown))
                .map_err(|error| format!("failed to register SIGINT handler: {error}"))?;
            flag::register(SIGTERM, Arc::clone(&shutdown))
                .map_err(|error| format!("failed to register SIGTERM handler: {error}"))?;
            let mut host = reference_host();
            host.ingest_observation(
                complete_observation(
                    reference_source_id(),
                    reference_session_id(),
                    1,
                    Floor::new(4),
                )
                .into(),
            );
            let listener = TcpListener::bind(listen).map_err(|error| {
                format!("failed to bind experimental snapshot fixture: {error}")
            })?;
            let local_addr = listener
                .local_addr()
                .map_err(|error| format!("failed to inspect experimental API listener: {error}"))?;
            let publisher = http_api::ReadModelPublisher::new(host.read_model());
            let fixture_publisher = publisher.clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(300));
                host.record_transport_failure(
                    reference_source_id(),
                    reference_session_id(),
                    "fixture read-model change".into(),
                );
                let _ = fixture_publisher.publish(host.read_model());
            });
            println!("experimental read-only API fixture listening on http://{local_addr}");
            http_api::serve(listener, publisher, shutdown)
        }
    }
}

fn parse_command(args: impl Iterator<Item = String>) -> Result<HeadlessCommand, String> {
    let args = args.collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("--scenario") => parse_scenario(args.into_iter()).map(HeadlessCommand::Scenario),
        Some(RMF_READ_ONLY_HOST_COMMAND) => {
            parse_rmf_host_config(args.into_iter().skip(1)).map(HeadlessCommand::RunRmfReadOnly)
        }
        Some(REPLAY_INCIDENT_CAPSULE_COMMAND) if args.len() == 2 => Ok(
            HeadlessCommand::ReplayIncidentCapsule(PathBuf::from(&args[1])),
        ),
        Some(SANITIZE_INCIDENT_CAPSULE_COMMAND) if args.len() == 3 => {
            Ok(HeadlessCommand::SanitizeIncidentCapsule {
                input: PathBuf::from(&args[1]),
                output: PathBuf::from(&args[2]),
            })
        }
        Some(SERVE_SNAPSHOT_FIXTURE_COMMAND) if args.len() == 2 => args[1]
            .parse::<SocketAddr>()
            .map(HeadlessCommand::ServeSnapshotFixture)
            .map_err(|error| format!("invalid experimental API listen address: {error}")),
        _ => Err(headless_usage()),
    }
}

fn headless_usage() -> String {
    format!(
        "usage: liftassistant-headless --scenario <{LIFECYCLE_SCENARIO}|{RMF_LIVE_SCENARIO}|{RMF_FRESHNESS_SCENARIO}|{INCIDENT_CAPSULE_SCENARIO}|{SANITIZED_CAPSULE_SCENARIO}|{READ_MODEL_SCENARIO}>\n       liftassistant-headless {RMF_READ_ONLY_HOST_COMMAND} --domain-id <u16> --source-id <id> --max-silence-ms <u64> --floor <label=i16>... --bind <native=logical:operational|auxiliary>... [--agv-as-normal] [--human-as-normal] [--incident-capsule-out <path>] [--api-listen <ip:port>]\n       liftassistant-headless {REPLAY_INCIDENT_CAPSULE_COMMAND} <internal-capsule.json>\n       liftassistant-headless {SANITIZE_INCIDENT_CAPSULE_COMMAND} <private-capsule.json> <candidate.json>\n       liftassistant-headless {SERVE_SNAPSHOT_FIXTURE_COMMAND} <ip:port>"
    )
}

fn parse_scenario(mut args: impl Iterator<Item = String>) -> Result<String, String> {
    match (args.next(), args.next(), args.next()) {
        (Some(flag), Some(name), None) if flag == "--scenario" => Ok(name),
        _ => Err(headless_usage()),
    }
}

fn parse_rmf_host_config(mut args: impl Iterator<Item = String>) -> Result<RmfHostConfig, String> {
    let mut domain_id = None;
    let mut source = None;
    let mut max_payload_silence = None;
    let mut floor_labels = BTreeMap::new();
    let mut bindings = Vec::new();
    let mut native_names = BTreeSet::new();
    let mut agv_as_normal = false;
    let mut human_as_normal = false;
    let mut incident_capsule_out = None;
    let mut api_listen = None;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--domain-id" => {
                set_once(
                    &mut domain_id,
                    parse_rmf_domain_id(&parse_value(&mut args, "--domain-id")?, "--domain-id")?,
                    "--domain-id",
                )?;
            }
            "--source-id" => {
                let value = parse_value(&mut args, "--source-id")?;
                if value.is_empty() {
                    return Err("--source-id cannot be empty".into());
                }
                set_once(&mut source, ObservationSourceId::new(value), "--source-id")?;
            }
            "--max-silence-ms" => {
                let milliseconds = parse_value(&mut args, "--max-silence-ms")?
                    .parse::<u64>()
                    .map_err(|error| format!("--max-silence-ms must fit in u64: {error}"))?;
                let policy = RmfFreshnessPolicy::new(Duration::from_millis(milliseconds))
                    .map_err(|error| format!("invalid --max-silence-ms: {error}"))?;
                set_once(
                    &mut max_payload_silence,
                    policy.max_payload_silence(),
                    "--max-silence-ms",
                )?;
            }
            "--floor" => {
                let value = parse_value(&mut args, "--floor")?;
                let (label, floor) = value
                    .split_once('=')
                    .ok_or_else(|| format!("--floor {value:?} must be <label=i16>"))?;
                if label.is_empty() {
                    return Err("--floor label cannot be empty".into());
                }
                let floor = floor
                    .parse::<i16>()
                    .map(Floor::new)
                    .map_err(|error| format!("--floor {value:?} has invalid i16 index: {error}"))?;
                if floor_labels.insert(label.to_owned(), floor).is_some() {
                    return Err(format!("duplicate --floor label {label:?}"));
                }
            }
            "--bind" => {
                let binding = parse_rmf_host_binding(&parse_value(&mut args, "--bind")?)?;
                if !native_names.insert(binding.native_lift_name.clone()) {
                    return Err(format!(
                        "duplicate --bind native lift name {:?}",
                        binding.native_lift_name
                    ));
                }
                bindings.push(binding);
            }
            "--agv-as-normal" => {
                if std::mem::replace(&mut agv_as_normal, true) {
                    return Err("duplicate --agv-as-normal".into());
                }
            }
            "--human-as-normal" => {
                if std::mem::replace(&mut human_as_normal, true) {
                    return Err("duplicate --human-as-normal".into());
                }
            }
            "--incident-capsule-out" => {
                let value = parse_value(&mut args, "--incident-capsule-out")?;
                if value.is_empty() {
                    return Err("--incident-capsule-out cannot be empty".into());
                }
                set_once(
                    &mut incident_capsule_out,
                    PathBuf::from(value),
                    "--incident-capsule-out",
                )?;
            }
            "--api-listen" => {
                let value = parse_value(&mut args, "--api-listen")?;
                let listen = value
                    .parse::<SocketAddr>()
                    .map_err(|error| format!("invalid --api-listen address {value:?}: {error}"))?;
                set_once(&mut api_listen, listen, "--api-listen")?;
            }
            other => {
                return Err(format!(
                    "unknown RMF host option {other:?}\n{}",
                    headless_usage()
                ));
            }
        }
    }

    let domain_id = domain_id.ok_or("missing required --domain-id")?;
    let source = source.ok_or("missing required --source-id")?;
    let max_payload_silence = max_payload_silence.ok_or("missing required --max-silence-ms")?;
    if floor_labels.is_empty() {
        return Err("at least one --floor is required".into());
    }
    if bindings.is_empty() {
        return Err("at least one --bind is required".into());
    }
    if !bindings
        .iter()
        .any(|binding| binding.role == ObservationBindingRole::Operational)
    {
        return Err("at least one Operational --bind is required for host readiness".into());
    }

    Ok(RmfHostConfig {
        domain_id,
        source,
        max_payload_silence,
        floor_labels,
        bindings,
        agv_as_normal,
        human_as_normal,
        incident_capsule_out,
        api_listen,
    })
}

fn parse_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_rmf_domain_id(value: &str, origin: &str) -> Result<u16, String> {
    let domain_id = value
        .parse::<u16>()
        .map_err(|error| format!("{origin} must be an integer from 0 through 232: {error}"))?;
    if domain_id > MAX_DDS_DOMAIN_ID {
        return Err(format!(
            "{origin} {domain_id} exceeds the DDS/ROS 2 maximum domain ID {MAX_DDS_DOMAIN_ID}"
        ));
    }
    Ok(domain_id)
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!("duplicate {flag}"));
    }
    Ok(())
}

fn parse_rmf_host_binding(value: &str) -> Result<RmfHostBinding, String> {
    let (native_lift_name, logical_and_role) = value
        .split_once('=')
        .ok_or_else(|| format!("--bind {value:?} must be <native=logical:role>"))?;
    let (logical_car, role) = logical_and_role
        .rsplit_once(':')
        .ok_or_else(|| format!("--bind {value:?} must be <native=logical:role>"))?;
    if native_lift_name.is_empty() || logical_car.is_empty() {
        return Err(format!("--bind {value:?} contains an empty identity"));
    }
    let role = match role {
        "operational" => ObservationBindingRole::Operational,
        "auxiliary" => ObservationBindingRole::Auxiliary,
        _ => {
            return Err(format!(
                "--bind {value:?} role must be operational or auxiliary"
            ));
        }
    };
    Ok(RmfHostBinding {
        native_lift_name: native_lift_name.to_owned(),
        logical_car: CarId::new(logical_car),
        role,
    })
}

fn run_rmf_read_only_host(config: RmfHostConfig, shutdown: Arc<AtomicBool>) -> Result<(), String> {
    let topology = BuildingTopology::new(config.floor_labels.values().copied())
        .map_err(|error| format!("invalid configured topology: {error:?}"))?;
    let mut host = ReadOnlyHost::new(topology.clone());
    let mut adapter_bindings = Vec::new();
    for binding in &config.bindings {
        host.bind_observation(
            config.source.clone(),
            binding.logical_car.clone(),
            binding.role,
        )
        .map_err(|error| {
            format!(
                "invalid observation binding for native {:?}: {error:?}",
                binding.native_lift_name
            )
        })?;
        adapter_bindings.push((
            binding.native_lift_name.clone(),
            RmfLiftBinding::new(binding.logical_car.clone(), config.floor_labels.clone()),
        ));
    }

    let mut projection = RmfInteractionModeProjection::conservative();
    if config.agv_as_normal {
        projection = projection.with_agv_as_normal();
    }
    if config.human_as_normal {
        projection = projection.with_human_as_normal();
    }
    let session = new_host_source_session()?;
    let mut adapter = RmfObservationAdapter::new(
        config.source.clone(),
        session.clone(),
        projection,
        adapter_bindings,
    )
    .map_err(|error| format!("invalid RMF adapter configuration: {error}"))?;
    host.begin_source_session(config.source.clone(), session.clone())
        .map_err(|error| format!("failed to begin RMF source session: {error:?}"))?;
    let freshness = RmfFreshnessPolicy::new(config.max_payload_silence)
        .map_err(|error| format!("invalid RMF freshness configuration: {error}"))?;
    let api_server = config
        .api_listen
        .map(|listen| ExperimentalApiServer::start(listen, &host, Arc::clone(&shutdown)))
        .transpose()?;

    println!("LiftAssistant RMF read-only host starting");
    println!(
        "source={} session={} domain={} freshness_ms={} bindings={} floors={}",
        config.source,
        session.as_str(),
        config.domain_id,
        config.max_payload_silence.as_millis(),
        config.bindings.len(),
        topology.served_floors().len()
    );
    println!(
        "readiness={:?} lifecycle={:?}",
        host.readiness(),
        host.lifecycle()
    );
    let mut reported_lifecycle = host.lifecycle().clone();
    let mut reported_diagnostic_sequence = 0;
    let mut incident_capsule_out = config.incident_capsule_out.clone();
    emit_new_host_diagnostics(&host, &mut reported_diagnostic_sequence);
    let mut connection_generation = 0_u64;

    while !shutdown.load(Ordering::Relaxed) {
        connection_generation = connection_generation
            .checked_add(1)
            .ok_or("RMF subscriber connection generation exhausted")?;
        let node_name = format!(
            "rmf_read_only_host_{}_{}",
            std::process::id(),
            connection_generation
        );
        let subscriber = match RmfLiveSubscriber::connect(config.domain_id, &node_name) {
            Ok(subscriber) => subscriber,
            Err(error) => {
                host.record_transport_failure(
                    config.source.clone(),
                    session.clone(),
                    error.to_string(),
                );
                publish_read_model(&api_server, &host);
                emit_new_host_diagnostics(&host, &mut reported_diagnostic_sequence);
                return Err(format!(
                    "failed to create RMF read-only subscriber: {error}"
                ));
            }
        };
        host.note_transport_subscriber_created(adapter.source());
        let mut car_freshness = rmf_host::RmfCarFreshness::new(freshness);
        publish_read_model(&api_server, &host);
        emit_new_host_diagnostics(&host, &mut reported_diagnostic_sequence);
        let mut ingestion_error = None;
        let outcome = subscriber.receive_until_stale_or_shutdown(
            freshness,
            SHUTDOWN_POLL_INTERVAL,
            || shutdown.load(Ordering::Relaxed),
            |payload| {
                let before = host.read_model_revision;
                let result = car_freshness.supervise(
                    &mut host,
                    &mut adapter,
                    Instant::now(),
                    payload.as_ref().map(|payload| &payload.state),
                );
                if host.read_model_revision != before {
                    publish_read_model(&api_server, &host);
                    emit_new_host_diagnostics(&host, &mut reported_diagnostic_sequence);
                }
                if let Err(error) = result {
                    ingestion_error = Some(error);
                    return RmfReceiveControl::Stop;
                }
                if host.lifecycle() != &reported_lifecycle {
                    reported_lifecycle = host.lifecycle().clone();
                    println!(
                        "readiness={:?} lifecycle={:?}",
                        host.readiness(),
                        host.lifecycle()
                    );
                }
                RmfReceiveControl::Continue
            },
        );

        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                host.record_transport_failure(
                    config.source.clone(),
                    session.clone(),
                    error.to_string(),
                );
                host.mark_source_payload_stale(config.source.clone(), session.clone())
                    .map_err(|invalidation| {
                        format!(
                            "RMF transport failed ({error}) and source invalidation failed: {invalidation:?}"
                        )
                    })?;
                publish_read_model(&api_server, &host);
                write_source_stale_capsule(
                    &host,
                    &mut incident_capsule_out,
                    &config.source,
                    &session,
                )?;
                emit_new_host_diagnostics(&host, &mut reported_diagnostic_sequence);
                println!(
                    "readiness={:?} lifecycle={:?} reason=transport-error",
                    host.readiness(),
                    host.lifecycle()
                );
                return Err(format!("RMF read-only subscription failed: {error}"));
            }
        };
        match outcome {
            RmfReceiveOutcome::ShutdownRequested => break,
            RmfReceiveOutcome::PayloadStale { max_silence } => {
                host.mark_source_payload_stale(config.source.clone(), session.clone())
                    .map_err(|error| {
                        format!("failed to revoke stale RMF observations: {error:?}")
                    })?;
                publish_read_model(&api_server, &host);
                write_source_stale_capsule(
                    &host,
                    &mut incident_capsule_out,
                    &config.source,
                    &session,
                )?;
                emit_new_host_diagnostics(&host, &mut reported_diagnostic_sequence);
                reported_lifecycle = host.lifecycle().clone();
                println!(
                    "readiness={:?} lifecycle={:?} reason=payload-stale silence_ms={}",
                    host.readiness(),
                    host.lifecycle(),
                    max_silence.as_millis()
                );
            }
            RmfReceiveOutcome::HandlerStopped => {
                let error = ingestion_error
                    .unwrap_or_else(|| "RMF payload handler stopped unexpectedly".into());
                host.mark_source_payload_stale(config.source.clone(), session.clone())
                    .map_err(|invalidation| {
                        format!(
                            "RMF ingestion failed ({error}) and source invalidation failed: {invalidation:?}"
                        )
                    })?;
                publish_read_model(&api_server, &host);
                write_source_stale_capsule(
                    &host,
                    &mut incident_capsule_out,
                    &config.source,
                    &session,
                )?;
                emit_new_host_diagnostics(&host, &mut reported_diagnostic_sequence);
                println!(
                    "readiness={:?} lifecycle={:?} reason=ingestion-error",
                    host.readiness(),
                    host.lifecycle()
                );
                return Err(error);
            }
        }
    }

    println!("LiftAssistant RMF read-only host stopped gracefully");
    Ok(())
}

fn publish_read_model(api_server: &Option<ExperimentalApiServer>, host: &ReadOnlyHost) {
    if let Some(api_server) = api_server {
        api_server.publish(host);
    }
}

fn new_host_source_session() -> Result<ObservationSessionId, String> {
    let mut entropy = [0_u8; 16];
    getrandom::fill(&mut entropy)
        .map_err(|error| format!("failed to generate RMF source incarnation: {error}"))?;
    Ok(ObservationSessionId::new(format!(
        "{:032x}",
        u128::from_be_bytes(entropy)
    )))
}

fn write_source_stale_capsule(
    host: &ReadOnlyHost,
    output: &mut Option<PathBuf>,
    source: &ObservationSourceId,
    session: &ObservationSessionId,
) -> Result<(), String> {
    let Some(path) = output.as_ref() else {
        return Ok(());
    };
    host.capture_incident(CapsuleTrigger::source_payload_stale(source, session))
        .write_new(path)?;
    println!("incident capsule captured path={path:?} trigger=source.payload-stale");
    *output = None;
    Ok(())
}

fn emit_new_host_diagnostics(host: &ReadOnlyHost, reported_sequence: &mut u64) {
    let after_sequence = *reported_sequence;
    for entry in host
        .diagnostics()
        .entries()
        .filter(|entry| entry.sequence > after_sequence)
    {
        println!("{}", render_host_diagnostic(entry));
        *reported_sequence = entry.sequence;
    }
}

fn render_host_diagnostic(entry: &HostDiagnosticEntry) -> String {
    format!(
        "diagnostic sequence={} layer={} id={} source={:?} session={:?} car={:?} role={:?} native_entity={:?} evidence={:?}",
        entry.sequence,
        entry.layer.as_str(),
        entry.id.as_str(),
        entry.context.source,
        entry.context.session,
        entry.context.car,
        entry.context.role,
        entry.context.native_entity,
        entry.event
    )
}

fn run_scenario(name: &str) -> Result<String, String> {
    match name {
        LIFECYCLE_SCENARIO => run_lifecycle_scenario(),
        RMF_LIVE_SCENARIO => run_rmf_live_scenario(),
        RMF_FRESHNESS_SCENARIO => run_rmf_freshness_scenario(),
        INCIDENT_CAPSULE_SCENARIO => run_incident_capsule_scenario(),
        SANITIZED_CAPSULE_SCENARIO => run_sanitized_capsule_scenario(),
        READ_MODEL_SCENARIO => run_read_model_scenario(),
        _ => Err(format!(
            "unknown scenario {name:?}; available scenarios: {LIFECYCLE_SCENARIO}, {RMF_LIVE_SCENARIO}, {RMF_FRESHNESS_SCENARIO}, {INCIDENT_CAPSULE_SCENARIO}, {SANITIZED_CAPSULE_SCENARIO}, {READ_MODEL_SCENARIO}"
        )),
    }
}

fn run_read_model_scenario() -> Result<String, String> {
    let mut host = reference_host();
    host.ingest_observation(
        complete_observation(
            reference_source_id(),
            reference_session_id(),
            1,
            Floor::new(4),
        )
        .into(),
    );
    let model = host.read_model();
    let revision = model.revision();
    let json = serde_json::to_string_pretty(&model)
        .map_err(|error| format!("failed to encode experimental read model: {error}"))?;
    Ok(format!(
        "scenario: {READ_MODEL_SCENARIO}\nstatus: PASS (pure experimental/v0 projection; no socket)\nrevision: {revision}\n{json}"
    ))
}

fn run_sanitized_capsule_scenario() -> Result<String, String> {
    let private = capture_reference_incident_capsule()?;
    let private_json = private.to_json()?;
    let (first_candidate, report) = private.sanitize_candidate()?;
    let (second_candidate, second_report) = private.sanitize_candidate()?;
    if first_candidate != second_candidate || report != second_report {
        return Err("sanitization was not deterministic".into());
    }
    if !report.manual_review_required {
        return Err("sanitized candidate omitted the manual-review requirement".into());
    }
    let candidate_json = first_candidate.to_json()?;
    for private_value in [
        "fixture/capsule-source",
        "capsule-session-A",
        "capsule-session-B",
        "capsule-car",
    ] {
        if candidate_json.contains(private_value) {
            return Err(format!(
                "sanitized candidate retained private identifier {private_value:?}"
            ));
        }
    }
    if !candidate_json.contains("\"manual_review_required\": true") {
        return Err("sanitized candidate did not embed its manual-review marker".into());
    }
    let private_replay = private.replay()?;
    let candidate_replay = first_candidate.replay()?;
    if private_replay.trigger_id != candidate_replay.trigger_id {
        return Err("sanitized candidate changed the replay trigger".into());
    }

    let base = format!("liftassistant-sanitization-{}", std::process::id());
    let private_path = env::temp_dir().join(format!("{base}-private.json"));
    let candidate_path = env::temp_dir().join(format!("{base}-candidate.json"));
    private.write_new(&private_path)?;
    let child_result = (|| {
        let executable = env::current_exe()
            .map_err(|error| format!("failed to locate headless executable: {error}"))?;
        let output = Command::new(executable)
            .arg(SANITIZE_INCIDENT_CAPSULE_COMMAND)
            .arg(&private_path)
            .arg(&candidate_path)
            .output()
            .map_err(|error| format!("failed to start sanitization process: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "sanitization process failed: status={} stdout={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|error| format!("sanitization output was not UTF-8: {error}"))?;
        if !stdout.contains("manual_review=REQUIRED") || !stdout.contains("incident replay: PASS") {
            return Err(format!(
                "sanitization process omitted review/replay evidence: {stdout:?}"
            ));
        }
        let private_after = std::fs::read_to_string(&private_path)
            .map_err(|error| format!("failed to reread private capsule: {error}"))?;
        if private_after != private_json {
            return Err("sanitization mutated the private source capsule".into());
        }
        let candidate = IncidentCapsule::read(&candidate_path)?;
        let replay = candidate.replay()?;
        if replay.trigger_id != private_replay.trigger_id {
            return Err("written shareable candidate changed the replay trigger".into());
        }
        Ok(())
    })();
    let private_cleanup = remove_temporary_capsule(&private_path);
    let candidate_cleanup = remove_temporary_capsule(&candidate_path);
    child_result?;
    private_cleanup?;
    candidate_cleanup?;

    Ok([
        format!("scenario: {SANITIZED_CAPSULE_SCENARIO}"),
        "scope: deterministic pseudonymization candidate; not perfect or security-certified anonymization".into(),
        "source preservation: PASS (private capsule remained byte-identical)".into(),
        "relationship preservation: PASS (source/session/car/floor mappings and action ordering retained)".into(),
        format!(
            "candidate replay: PASS (trigger={})",
            candidate_replay.trigger_id
        ),
        "manual review: REQUIRED before sharing".into(),
        "automatic upload/publication: NONE".into(),
        "temporary artifact cleanup: PASS".into(),
        String::new(),
    ]
    .join("\n"))
}

fn remove_temporary_capsule(path: &std::path::Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to remove temporary capsule {path:?}: {error}"
        )),
    }
}

fn run_incident_capsule_scenario() -> Result<String, String> {
    let first = capture_reference_incident_capsule()?;
    let second = capture_reference_incident_capsule()?;
    let first_json = first.to_json()?;
    if first_json != second.to_json()? {
        return Err("identical incident actions did not produce an identical capsule".into());
    }
    let in_process = IncidentCapsule::from_json(&first_json)?.replay()?;

    let path = env::temp_dir().join(format!(
        "liftassistant-incident-capsule-{}.json",
        std::process::id()
    ));
    first.write_new(&path)?;
    let child_result = (|| {
        let executable = env::current_exe()
            .map_err(|error| format!("failed to locate headless executable: {error}"))?;
        let output = Command::new(executable)
            .arg(REPLAY_INCIDENT_CAPSULE_COMMAND)
            .arg(&path)
            .output()
            .map_err(|error| format!("failed to start fresh incident replay process: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "fresh incident replay process failed: status={} stdout={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|error| format!("fresh replay output was not UTF-8: {error}"))?;
        if !stdout.contains("incident replay: PASS") || !stdout.contains(in_process.trigger_id) {
            return Err(format!(
                "fresh incident replay did not report the captured trigger: {stdout:?}"
            ));
        }
        Ok(())
    })();
    let cleanup_result = std::fs::remove_file(&path)
        .map_err(|error| format!("failed to remove temporary incident capsule {path:?}: {error}"));
    child_result?;
    cleanup_result?;

    Ok([
        format!("scenario: {INCIDENT_CAPSULE_SCENARIO}"),
        "scope: internal/versioned bounded artifact; no stable schema, database, cloud upload, or write authority".into(),
        "capture determinism: PASS (identical bounded actions produced identical JSON)".into(),
        "lifecycle coverage: PASS (freshness invalidation, same-session recovery, explicit session reset, incomplete resolution, readiness recovery)".into(),
        format!(
            "fresh process replay: PASS (trigger={}, actions={})",
            in_process.trigger_id, in_process.action_count
        ),
        "temporary artifact cleanup: PASS".into(),
        String::new(),
    ]
    .join("\n"))
}

fn capture_reference_incident_capsule() -> Result<IncidentCapsule, String> {
    let source = ObservationSourceId::new("fixture/capsule-source");
    let car = CarId::new("capsule-car");
    let session_a = ObservationSessionId::new("capsule-session-A");
    let session_b = ObservationSessionId::new("capsule-session-B");
    let mut host = ReadOnlyHost::new(
        BuildingTopology::new([Floor::new(3), Floor::new(7)])
            .expect("capsule fixture topology is valid"),
    );
    host.bind_observation(
        source.clone(),
        car.clone(),
        ObservationBindingRole::Operational,
    )
    .map_err(|error| format!("capsule fixture binding failed: {error:?}"))?;
    host.begin_source_session(source.clone(), session_a.clone())
        .map_err(|error| format!("capsule fixture session A failed: {error:?}"))?;

    let mut initial = complete_observation(source.clone(), session_a.clone(), 1, Floor::new(3));
    initial.car = car.clone();
    host.ingest_observation(initial.into());
    if host.readiness() != Readiness::ReadOnly {
        return Err("capsule fixture did not reach initial readiness".into());
    }

    host.mark_source_payload_stale(source.clone(), session_a.clone())
        .map_err(|error| format!("capsule fixture invalidation failed: {error:?}"))?;
    if host.readiness() != Readiness::NotReady {
        return Err("capsule fixture freshness invalidation did not revoke readiness".into());
    }
    let mut recovered = complete_observation(source.clone(), session_a, 2, Floor::new(3));
    recovered.car = car.clone();
    host.ingest_observation(recovered.into());
    if host.readiness() != Readiness::ReadOnly {
        return Err("capsule fixture did not recover under the active session".into());
    }

    host.begin_source_session(source.clone(), session_b.clone())
        .map_err(|error| format!("capsule fixture session B failed: {error:?}"))?;
    let partial = CarObservationUpdate {
        provenance: ObservationProvenance {
            source: source.clone(),
            session: session_b.clone(),
            sequence: ObservationSequence::new(1),
        },
        car: car.clone(),
        position: Some(PositionEvidence::FloorReference(Floor::new(7))),
        motion: None,
        door: None,
        mode: None,
        reported_destination: None,
    };
    host.ingest_observation(partial);
    if host.readiness() != Readiness::NotReady {
        return Err("capsule fixture incomplete evidence unexpectedly became ready".into());
    }

    let mut complete = complete_observation(source.clone(), session_b.clone(), 2, Floor::new(7));
    complete.car = car.clone();
    host.ingest_observation(complete.clone().into());
    if host.readiness() != Readiness::ReadOnly {
        return Err("capsule fixture session B did not resolve".into());
    }
    host.ingest_observation(complete.into());

    let anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
        source,
        session: session_b,
        previous: ObservationSequence::new(2),
        received: ObservationSequence::new(2),
    };
    if !matches!(host.lifecycle(), HostLifecycle::Degraded(_)) {
        return Err("capsule fixture duplicate sequence did not degrade readiness".into());
    }
    let trigger = CapsuleTrigger::observation_sequence_not_increasing(
        &car,
        ObservationBindingRole::Operational,
        &anomaly,
    )?;
    Ok(host.capture_incident(trigger))
}

fn run_lifecycle_scenario() -> Result<String, String> {
    let first = execute_proof();
    let second = execute_proof();
    if first != second {
        return Err("headless lifecycle proof was not deterministic".into());
    }
    verify_proof(&first)?;
    Ok(render_report(&first))
}

#[derive(Debug)]
struct LiveRmfRound {
    source_times: Vec<i64>,
    sequences: Vec<u64>,
    source_contexts: BTreeMap<String, RmfSourceContext>,
    last_update: CarObservationUpdate,
}

fn rmf_domain_id() -> Result<u16, String> {
    parse_rmf_domain_id(
        &env::var("ROS_DOMAIN_ID").unwrap_or_else(|_| "47".into()),
        "ROS_DOMAIN_ID",
    )
}

fn hotel_host(source: &ObservationSourceId) -> Result<ReadOnlyHost, String> {
    let mut host = ReadOnlyHost::new(
        BuildingTopology::new([Floor::new(1), Floor::new(2), Floor::new(3)])
            .expect("Hotel proof topology is non-empty"),
    );
    for car in [CarId::new("A"), CarId::new("B")] {
        host.bind_observation(source.clone(), car, ObservationBindingRole::Operational)
            .map_err(|error| format!("failed to bind live RMF source: {error:?}"))?;
    }
    Ok(host)
}

fn run_rmf_live_scenario() -> Result<String, String> {
    let domain_id = rmf_domain_id()?;
    let source = ObservationSourceId::new("rmf/hotel-live");
    let mut host = hotel_host(&source)?;

    let mut first_adapter = hotel_adapter(source.clone(), "hotel-adapter-session-A")?;
    host.begin_source_session(source.clone(), first_adapter.session().clone())
        .map_err(|error| format!("failed to begin first live RMF session: {error:?}"))?;
    let first = receive_live_rmf_round(
        &mut host,
        &mut first_adapter,
        domain_id,
        "A",
        60_000_000_000,
    )?;
    if host.readiness() != Readiness::ReadOnly || host.engine.car_states().len() != 2 {
        return Err("first live RMF session did not establish two-car ReadyReadOnly state".into());
    }

    let second_adapter = hotel_adapter(source.clone(), "hotel-adapter-session-B")?;
    let transition = host
        .begin_source_session(source.clone(), second_adapter.session().clone())
        .map_err(|error| format!("failed to begin second live RMF session: {error:?}"))?;
    if transition.invalidated_cars != [CarId::new("A"), CarId::new("B")]
        || host.readiness() != Readiness::NotReady
        || !host.engine.car_states().is_empty()
    {
        return Err("new live RMF session did not invalidate both operational cars".into());
    }
    match host
        .engine
        .ingest_observation_update(first.last_update.clone())
    {
        ObservationIngestResult::Rejected(ObservationIngestError::InactiveSourceSession {
            active,
            received,
            ..
        }) if active == *second_adapter.session()
            && received == ObservationSessionId::new("hotel-adapter-session-A") => {}
        other => {
            return Err(format!(
                "delayed retired-session update was not rejected as inactive: {other:?}"
            ));
        }
    }

    println!(
        "live-proof: first session ready; restart the official RMF source, wait for it to publish /lift_states, then press Enter"
    );
    let mut acknowledgment = String::new();
    io::stdin()
        .read_line(&mut acknowledgment)
        .map_err(|error| {
            format!("failed while waiting for source-restart acknowledgment: {error}")
        })?;

    let mut second_adapter = second_adapter;
    let second = receive_live_rmf_round(&mut host, &mut second_adapter, domain_id, "B", 0)?;
    if host.readiness() != Readiness::ReadOnly || host.engine.car_states().len() != 2 {
        return Err("second live RMF session did not restore two-car ReadyReadOnly state".into());
    }
    let first_max_time = first
        .source_times
        .iter()
        .max()
        .copied()
        .expect("a completed live round has source times");
    let second_min_time = second
        .source_times
        .iter()
        .min()
        .copied()
        .expect("a completed live round has source times");
    if second_min_time >= first_max_time {
        return Err(format!(
            "RMF lift_time did not reset across the externally acknowledged source restart: first max={first_max_time}, second min={second_min_time}"
        ));
    }

    println!(
        "live-proof: second session ready; issue the independent simulation-only Lift1 -> L2 AGV request now"
    );
    let transition = receive_live_rmf_transition(&mut host, &mut second_adapter, domain_id)?;

    let raw_modes = second
        .source_contexts
        .iter()
        .map(|(lift, context)| format!("{lift}={}", context.source_mode))
        .collect::<Vec<_>>()
        .join(", ");
    Ok([
        format!("scenario: {RMF_LIVE_SCENARIO}"),
        "scope: official Open-RMF simulation; subscription only; no LiftRequest publisher or IntentSink".into(),
        "identity: PASS (Lift1 -> A, Lift2 -> B through explicit configuration)".into(),
        format!(
            "session A: PASS (source-global sequences {:?}; two Engine operational cars; ReadyReadOnly)",
            first.sequences
        ),
        "session transition: PASS (both old operational states invalidated; delayed session-A update rejected)".into(),
        format!(
            "session B: PASS (source-global sequences {:?}; lift_time reset after source restart; ReadyReadOnly restored)",
            second.sequences
        ),
        format!(
            "movement/door: PASS (independent simulation harness observed by LiftAssistant at sequences {:?}; moving and door-moving evidence seen; Lift1 resolved stopped/open at L2)",
            transition.sequences
        ),
        format!("raw RMF context: PASS ({raw_modes}; explicit AGV-as-Normal projection retained raw modes)"),
        "write boundary: PASS (live RMF crate exposes a subscriber and canonical observation adapter only)".into(),
        String::new(),
    ]
    .join("\n"))
}

fn run_rmf_freshness_scenario() -> Result<String, String> {
    let domain_id = rmf_domain_id()?;
    let max_silence_ms = env::var("LIFTASSISTANT_RMF_MAX_SILENCE_MS")
        .map_err(|_| {
            "LIFTASSISTANT_RMF_MAX_SILENCE_MS must explicitly configure this source's freshness proof"
                .to_owned()
        })?
        .parse::<u64>()
        .map_err(|error| {
            format!("LIFTASSISTANT_RMF_MAX_SILENCE_MS must fit in u64: {error}")
        })?;
    let freshness = RmfFreshnessPolicy::new(Duration::from_millis(max_silence_ms))
        .map_err(|error| format!("invalid Hotel freshness policy: {error}"))?;
    let source = ObservationSourceId::new("rmf/hotel-freshness-live");
    let session = ObservationSessionId::new("hotel-freshness-adapter-session-A");
    let mut host = hotel_host(&source)?;
    let mut adapter = hotel_adapter(source.clone(), session.as_str())?;
    host.begin_source_session(source.clone(), session.clone())
        .map_err(|error| format!("failed to begin Hotel freshness session: {error:?}"))?;
    let initial_sequences =
        receive_ready_then_rmf_stale(&mut host, &mut adapter, domain_id, freshness)?;
    let invalidation = host
        .mark_source_payload_stale(source.clone(), session.clone())
        .map_err(|error| format!("failed to revoke stale source observations: {error:?}"))?;
    if invalidation.invalidated_cars != [CarId::new("A"), CarId::new("B")]
        || host.readiness() != Readiness::NotReady
        || !host.engine.car_states().is_empty()
    {
        return Err("stale Operational source did not revoke two-car readiness/truth".into());
    }
    let stale_health = host
        .source_health(&source)
        .expect("configured source health remains available");
    if stale_health.payload != PayloadActivity::Stale
        || stale_health
            .observations
            .values()
            .any(|availability| *availability != ObservationAvailability::Revoked)
    {
        return Err(format!(
            "source health did not separate stale payload from revoked observations: {stale_health:?}"
        ));
    }

    println!(
        "freshness-proof: stale state revoked; restart the official RMF source, wait for /lift_states, then press Enter"
    );
    let mut acknowledgment = String::new();
    io::stdin()
        .read_line(&mut acknowledgment)
        .map_err(|error| format!("failed while waiting for source recovery: {error}"))?;
    let recovered = receive_live_rmf_round(&mut host, &mut adapter, domain_id, "fresh_B", 0)?;
    if host.readiness() != Readiness::ReadOnly || host.engine.car_states().len() != 2 {
        return Err("resumed source did not resynchronize two-car ReadyReadOnly state".into());
    }
    let recovered_health = host
        .source_health(&source)
        .expect("recovered source health remains available");
    if recovered_health.payload != PayloadActivity::Active
        || recovered_health
            .observations
            .values()
            .any(|availability| *availability != ObservationAvailability::Resolved)
    {
        return Err(format!(
            "source health did not recover payload/resolution dimensions: {recovered_health:?}"
        ));
    }

    Ok([
        format!("scenario: {RMF_FRESHNESS_SCENARIO}"),
        format!(
            "policy: PASS (explicit RMF source max payload silence = {} ms; no universal/default timeout)",
            freshness.max_payload_silence().as_millis()
        ),
        "health layers: PASS (subscriber/discovery evidence, payload activity, adapter session, per-car resolution, Engine readiness remain distinct)".into(),
        format!(
            "stale: PASS (continuous subscription sequences {initial_sequences:?}; two operational states revoked; ReadyReadOnly -> Synchronizing/NotReady)"
        ),
        format!(
            "recovery: PASS (same adapter session and increasing source-global sequences {:?}; two complete states restored ReadyReadOnly)",
            recovered.sequences
        ),
        "evidence: PASS (last known canonical observations remain in bounded diagnostics; current evidence/state revoked)".into(),
        "authority: PASS (no Auxiliary promotion, automatic failover, field fusion, or write path)".into(),
        String::new(),
    ]
    .join("\n"))
}

fn receive_ready_then_rmf_stale(
    host: &mut ReadOnlyHost,
    adapter: &mut RmfObservationAdapter,
    domain_id: u16,
    freshness: RmfFreshnessPolicy,
) -> Result<Vec<u64>, String> {
    let node_name = format!("rmf_freshness_proof_{}", std::process::id());
    let subscriber = RmfLiveSubscriber::connect(domain_id, &node_name)
        .map_err(|error| format!("failed to connect freshness observer: {error}"))?;
    host.note_transport_subscriber_created(adapter.source());
    let mut sequences = Vec::new();
    let mut ready_announced = false;
    let mut mapping_error = None;
    let outcome = subscriber
        .receive_until_stale(freshness, |payload| {
            host.note_transport_discovery(adapter.source());
            let mapped = match adapter.map(&payload.state) {
                Ok(mapped) => mapped,
                Err(error) => {
                    mapping_error = Some(error.to_string());
                    return RmfReceiveControl::Stop;
                }
            };
            sequences.push(mapped.update.provenance.sequence.value());
            host.ingest_observation(mapped.update);
            if !ready_announced
                && host.readiness() == Readiness::ReadOnly
                && host.engine.car_states().len() == 2
            {
                ready_announced = true;
                println!(
                    "freshness-proof: initial payloads ready; stop the official RMF source and leave this observer running"
                );
            }
            RmfReceiveControl::Continue
        })
        .map_err(|error| format!("live RMF freshness receive loop failed: {error}"))?;
    if let Some(error) = mapping_error {
        return Err(error);
    }
    if !ready_announced {
        return Err("payload silence occurred before two-car ReadyReadOnly was established".into());
    }
    match outcome {
        RmfReceiveOutcome::PayloadStale { max_silence }
            if max_silence == freshness.max_payload_silence() =>
        {
            Ok(sequences)
        }
        other => Err(format!(
            "freshness observer stopped without the configured payload-silence event: {other:?}"
        )),
    }
}

#[derive(Debug)]
struct LiveRmfTransition {
    sequences: Vec<u64>,
}

fn receive_live_rmf_transition(
    host: &mut ReadOnlyHost,
    adapter: &mut RmfObservationAdapter,
    domain_id: u16,
) -> Result<LiveRmfTransition, String> {
    let node_name = format!("rmf_live_transition_proof_{}", std::process::id());
    let subscriber = RmfLiveSubscriber::connect(domain_id, &node_name)
        .map_err(|error| format!("failed to connect transition observer: {error}"))?;
    host.note_transport_subscriber_created(adapter.source());
    let mut sequences = Vec::new();
    let mut saw_moving = false;
    let mut saw_door_moving = false;
    let mut mapping_error = None;

    subscriber
        .receive_until(|payload| {
            host.note_transport_discovery(adapter.source());
            if payload.state.lift_name != "Lift1" {
                let mapped = match adapter.map(&payload.state) {
                    Ok(mapped) => mapped,
                    Err(error) => {
                        mapping_error = Some(error.to_string());
                        return RmfReceiveControl::Stop;
                    }
                };
                sequences.push(mapped.update.provenance.sequence.value());
                host.ingest_observation(mapped.update);
                return RmfReceiveControl::Continue;
            }

            saw_moving |= matches!(
                payload.state.motion_state,
                RmfLiftState::MOTION_UP | RmfLiftState::MOTION_DOWN
            );
            saw_door_moving |= payload.state.door_state == RmfLiftState::DOOR_MOVING;
            let stopped_open_at_l2 = payload.state.current_floor == "L2"
                && payload.state.motion_state == RmfLiftState::MOTION_STOPPED
                && payload.state.door_state == RmfLiftState::DOOR_OPEN;
            let mapped = match adapter.map(&payload.state) {
                Ok(mapped) => mapped,
                Err(error) => {
                    mapping_error = Some(error.to_string());
                    return RmfReceiveControl::Stop;
                }
            };
            sequences.push(mapped.update.provenance.sequence.value());
            host.ingest_observation(mapped.update);
            if stopped_open_at_l2 && saw_moving && saw_door_moving {
                RmfReceiveControl::Stop
            } else {
                RmfReceiveControl::Continue
            }
        })
        .map_err(|error| format!("live RMF transition receive loop failed: {error}"))?;
    if let Some(error) = mapping_error {
        return Err(error);
    }
    let Some(state) = host.engine.operational_car_state(&CarId::new("A")) else {
        return Err("Lift1 transition did not restore logical car A operational state".into());
    };
    if !saw_moving
        || !saw_door_moving
        || state.location != CarLocation::Floor(Floor::new(2))
        || state.motion != MotionState::Stopped
        || state.door != DoorState::Open
        || state.mode != OperatingMode::Normal
        || host.readiness() != Readiness::ReadOnly
    {
        return Err(format!(
            "Lift1 transition evidence was incomplete: moving={saw_moving}, door_moving={saw_door_moving}, state={state:?}, readiness={:?}",
            host.readiness()
        ));
    }

    Ok(LiveRmfTransition { sequences })
}

fn hotel_adapter(
    source: ObservationSourceId,
    session: &str,
) -> Result<RmfObservationAdapter, String> {
    let floor_labels = [("L1", 1), ("L2", 2), ("L3", 3)]
        .into_iter()
        .map(|(label, floor)| (label.to_owned(), Floor::new(floor)))
        .collect::<BTreeMap<_, _>>();
    RmfObservationAdapter::new(
        source,
        ObservationSessionId::new(session),
        RmfInteractionModeProjection::conservative().with_agv_as_normal(),
        [
            (
                "Lift1".into(),
                RmfLiftBinding::new(CarId::new("A"), floor_labels.clone()),
            ),
            (
                "Lift2".into(),
                RmfLiftBinding::new(CarId::new("B"), floor_labels),
            ),
        ],
    )
    .map_err(|error| format!("invalid Hotel RMF adapter configuration: {error}"))
}

fn receive_live_rmf_round(
    host: &mut ReadOnlyHost,
    adapter: &mut RmfObservationAdapter,
    domain_id: u16,
    round: &str,
    minimum_source_time_nanos: i64,
) -> Result<LiveRmfRound, String> {
    let node_name = format!("rmf_live_proof_{}_{}", std::process::id(), round);
    let subscriber = RmfLiveSubscriber::connect(domain_id, &node_name)
        .map_err(|error| format!("failed to connect live RMF subscriber: {error}"))?;
    host.note_transport_subscriber_created(adapter.source());
    let expected_topology = host.engine.topology().clone();
    let mut source_times = Vec::new();
    let mut sequences = Vec::new();
    let mut source_contexts = BTreeMap::new();
    let mut last_update = None;
    let mut mapping_error = None;

    subscriber
        .receive_until(|payload| {
            host.note_transport_discovery(adapter.source());
            let mapped = match adapter.map(&payload.state) {
                Ok(mapped) => mapped,
                Err(error) => {
                    mapping_error = Some(error.to_string());
                    return RmfReceiveControl::Stop;
                }
            };
            if let Err(error) =
                rmf_host::validate_available_floors(&expected_topology, &mapped.topology)
            {
                mapping_error = Some(error);
                return RmfReceiveControl::Stop;
            }
            source_times.push(payload.source_time_nanos);
            sequences.push(mapped.update.provenance.sequence.value());
            source_contexts.insert(
                mapped.source_context.lift_name.clone(),
                mapped.source_context,
            );
            last_update = Some(mapped.update.clone());
            host.ingest_observation(mapped.update);

            if host.readiness() == Readiness::ReadOnly
                && host.engine.car_states().len() == 2
                && payload.source_time_nanos >= minimum_source_time_nanos
            {
                RmfReceiveControl::Stop
            } else {
                RmfReceiveControl::Continue
            }
        })
        .map_err(|error| format!("live RMF receive loop failed: {error}"))?;
    if let Some(error) = mapping_error {
        return Err(error);
    }
    if source_contexts.keys().cloned().collect::<BTreeSet<_>>()
        != BTreeSet::from(["Lift1".to_owned(), "Lift2".to_owned()])
    {
        return Err(format!(
            "live RMF round did not observe both explicitly configured lifts: {:?}",
            source_contexts.keys().collect::<Vec<_>>()
        ));
    }

    Ok(LiveRmfRound {
        source_times,
        sequences,
        source_contexts,
        last_update: last_update.expect("a completed live round has an update"),
    })
}

fn execute_proof() -> ProofRun {
    let mut source = reference_source();
    let mut host = reference_host();
    let mut snapshots = vec![snapshot(&host)];
    while host.poll(&mut source) {
        snapshots.push(snapshot(&host));
    }

    ProofRun {
        snapshots,
        events: host
            .diagnostics()
            .entries()
            .map(|entry| entry.event.clone())
            .collect(),
        latest_state: host.engine.car_states().into_iter().next(),
        diagnostic_entries: host.engine.diagnostics().entries().count(),
        incidents: host.engine.diagnostics().incidents().count(),
    }
}

fn reference_host() -> ReadOnlyHost {
    let mut host = ReadOnlyHost::new(
        BuildingTopology::new([1, 2, 3, 4].map(Floor::new))
            .expect("reference topology is non-empty"),
    );
    host.bind_observation(
        reference_source_id(),
        reference_car_id(),
        ObservationBindingRole::Operational,
    )
    .expect("headless fixture has one operational observation binding");
    host.begin_source_session(reference_source_id(), reference_session_id())
        .expect("headless fixture starts one source session");
    host
}

fn reference_source_id() -> ObservationSourceId {
    ObservationSourceId::new("fixture/headless-recorded")
}

fn reference_session_id() -> ObservationSessionId {
    ObservationSessionId::new("headless-fixture-session-A")
}

fn reference_car_id() -> CarId {
    CarId::new("lift-A")
}

fn snapshot(host: &ReadOnlyHost) -> LifecycleSnapshot {
    LifecycleSnapshot {
        lifecycle: host.lifecycle().clone(),
        readiness: host.readiness(),
    }
}

fn reference_source() -> RecordedObservationFeed {
    let complete = complete_observation(
        reference_source_id(),
        reference_session_id(),
        2,
        Floor::new(4),
    );
    let duplicate = complete.clone();
    RecordedObservationFeed::new([observation(1), complete, duplicate])
}

fn complete_observation(
    source: ObservationSourceId,
    session: ObservationSessionId,
    sequence: u64,
    floor: Floor,
) -> CarObservation {
    let mut complete = observation(sequence);
    complete.provenance.source = source;
    complete.provenance.session = session;
    complete.position = PositionEvidence::Exact(Position::at_floor(floor));
    complete.motion = MotionEvidence::Known(MotionState::Stopped);
    complete.door = DoorEvidence::Known(DoorState::Open);
    complete.reported_destination = ReportedDestination::None;
    complete
}

fn observation(sequence: u64) -> CarObservation {
    CarObservation {
        provenance: ObservationProvenance {
            source: reference_source_id(),
            session: reference_session_id(),
            sequence: ObservationSequence::new(sequence),
        },
        car: reference_car_id(),
        position: PositionEvidence::FloorReference(Floor::new(3)),
        motion: MotionEvidence::Known(MotionState::Moving(liftassistant_model::Direction::Up)),
        door: DoorEvidence::MovingUnknownDirection,
        mode: OperatingModeEvidence::Known(OperatingMode::FireService),
        reported_destination: ReportedDestination::Known(Floor::new(4)),
    }
}

fn expected_anomaly() -> DiagnosticAnomaly {
    DiagnosticAnomaly::ObservationSequenceNotIncreasing {
        source: ObservationSourceId::new("fixture/headless-recorded"),
        session: reference_session_id(),
        previous: ObservationSequence::new(2),
        received: ObservationSequence::new(2),
    }
}

fn verify_proof(run: &ProofRun) -> Result<(), String> {
    let anomaly = expected_anomaly();
    let expected_snapshots = [
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Starting,
            readiness: Readiness::NotReady,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Synchronizing,
            readiness: Readiness::NotReady,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::ReadyReadOnly,
            readiness: Readiness::ReadOnly,
        },
        LifecycleSnapshot {
            lifecycle: HostLifecycle::Degraded(DegradedReason::ObservationAnomaly(anomaly.clone())),
            readiness: Readiness::NotReady,
        },
    ];
    if run.snapshots != expected_snapshots {
        return Err("headless lifecycle/readiness transitions did not match the proof".into());
    }
    if run.latest_state.as_ref().map(|state| state.location)
        != Some(CarLocation::Continuous(Position::at_floor(Floor::new(4))))
    {
        return Err("headless host did not retain the last resolved read-only state".into());
    }
    if run.diagnostic_entries != 4 || run.incidents != 1 {
        return Err("headless host did not retain the expected diagnostic evidence".into());
    }
    if !run.events.iter().any(|event| {
        matches!(
            event,
            HostEvent::ObservationRejected { anomaly: observed, .. } if observed == &anomaly
        )
    }) {
        return Err("headless host did not preserve the degradation trigger".into());
    }
    Ok(())
}

fn render_report(run: &ProofRun) -> String {
    let transitions = run
        .snapshots
        .iter()
        .map(|snapshot| format!("{:?}/{:?}", snapshot.lifecycle, snapshot.readiness))
        .collect::<Vec<_>>()
        .join(" -> ");
    let anomaly = run.events.iter().find_map(|event| match event {
        HostEvent::ObservationRejected { anomaly, .. } => Some(anomaly),
        _ => None,
    });
    [
        format!("scenario: {LIFECYCLE_SCENARIO}"),
        "scope: internal headless read-only proof; no passenger write or deployment claim".into(),
        "determinism: PASS (2 identical headless executions)".into(),
        format!("lifecycle/readiness: PASS ({transitions})"),
        "process alive != ready: PASS (Starting and Degraded are NotReady)".into(),
        "write boundary: PASS (recorded source exposes observations only; no IntentSink implementation or simulator dependency)".into(),
        format!(
            "engine diagnostics: PASS ({} entries, {} incident, trigger={anomaly:?})",
            run.diagnostic_entries, run.incidents
        ),
        "outcome: PASS (partial observation synchronized, complete state became read-only ready, duplicate sequence revoked readiness)".into(),
        String::new(),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_sim::{
        BacnetDoorStatus, BacnetLiftCarDirection, BacnetLiftCarMode, BacnetMappingError,
        RecordedBacnetLiftProperties, map_bacnet_lift_properties,
    };

    fn command_args(values: &[&str]) -> impl Iterator<Item = String> {
        values
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn production_rmf_configuration_is_explicit_and_preserves_binding_roles() {
        let command = parse_command(command_args(&[
            RMF_READ_ONLY_HOST_COMMAND,
            "--domain-id",
            "48",
            "--source-id",
            "rmf/hotel-production",
            "--max-silence-ms",
            "3000",
            "--floor",
            "L1=1",
            "--floor",
            "L2=2",
            "--bind",
            "Lift1=A:operational",
            "--bind",
            "Lift2=B:auxiliary",
            "--agv-as-normal",
            "--incident-capsule-out",
            "/tmp/liftassistant-test-incident.json",
        ]))
        .expect("minimal production RMF configuration is valid");
        let HeadlessCommand::RunRmfReadOnly(config) = command else {
            panic!("expected production host command");
        };

        assert_eq!(config.domain_id, 48);
        assert_eq!(
            config.source,
            ObservationSourceId::new("rmf/hotel-production")
        );
        assert_eq!(config.max_payload_silence, Duration::from_secs(3));
        assert_eq!(
            config.floor_labels,
            BTreeMap::from([
                ("L1".to_owned(), Floor::new(1)),
                ("L2".to_owned(), Floor::new(2)),
            ])
        );
        assert_eq!(config.bindings.len(), 2);
        assert_eq!(config.bindings[0].logical_car, CarId::new("A"));
        assert_eq!(config.bindings[0].role, ObservationBindingRole::Operational);
        assert_eq!(config.bindings[1].logical_car, CarId::new("B"));
        assert_eq!(config.bindings[1].role, ObservationBindingRole::Auxiliary);
        assert!(config.agv_as_normal);
        assert!(!config.human_as_normal);
        assert_eq!(
            config.incident_capsule_out,
            Some(PathBuf::from("/tmp/liftassistant-test-incident.json"))
        );
        assert_eq!(config.api_listen, None);
    }

    #[test]
    fn production_api_requires_an_explicit_valid_listen_address() {
        let command = parse_command(command_args(&[
            RMF_READ_ONLY_HOST_COMMAND,
            "--domain-id",
            "48",
            "--source-id",
            "rmf/hotel-production",
            "--max-silence-ms",
            "3000",
            "--floor",
            "L1=1",
            "--bind",
            "Lift1=A:operational",
            "--api-listen",
            "127.0.0.1:8080",
        ]))
        .expect("explicit loopback API configuration is valid");
        let HeadlessCommand::RunRmfReadOnly(config) = command else {
            panic!("expected production host command");
        };
        assert_eq!(
            config.api_listen,
            Some("127.0.0.1:8080".parse().expect("fixture address parses"))
        );

        let invalid = parse_command(command_args(&[
            RMF_READ_ONLY_HOST_COMMAND,
            "--domain-id",
            "48",
            "--source-id",
            "rmf/hotel-production",
            "--max-silence-ms",
            "3000",
            "--floor",
            "L1=1",
            "--bind",
            "Lift1=A:operational",
            "--api-listen",
            "localhost:8080",
        ]))
        .expect_err("the API listen address must resolve explicitly to an IP socket");
        assert!(invalid.contains("invalid --api-listen address"));
    }

    #[test]
    fn production_rmf_configuration_rejects_implicit_or_nonoperational_hosts() {
        let missing_freshness = parse_command(command_args(&[
            RMF_READ_ONLY_HOST_COMMAND,
            "--domain-id",
            "48",
            "--source-id",
            "rmf/hotel-production",
            "--floor",
            "L1=1",
            "--bind",
            "Lift1=A:operational",
        ]))
        .expect_err("freshness must be explicit");
        assert!(missing_freshness.contains("missing required --max-silence-ms"));

        let auxiliary_only = parse_command(command_args(&[
            RMF_READ_ONLY_HOST_COMMAND,
            "--domain-id",
            "48",
            "--source-id",
            "rmf/hotel-production",
            "--max-silence-ms",
            "3000",
            "--floor",
            "L1=1",
            "--bind",
            "Lift1=A:auxiliary",
        ]))
        .expect_err("a production host needs configured operational truth");
        assert!(auxiliary_only.contains("at least one Operational --bind"));

        let duplicate_native = parse_command(command_args(&[
            RMF_READ_ONLY_HOST_COMMAND,
            "--domain-id",
            "48",
            "--source-id",
            "rmf/hotel-production",
            "--max-silence-ms",
            "3000",
            "--floor",
            "L1=1",
            "--bind",
            "Lift1=A:operational",
            "--bind",
            "Lift1=B:operational",
        ]))
        .expect_err("native identity mappings must be unique");
        assert!(duplicate_native.contains("duplicate --bind native lift name"));

        let invalid_domain = parse_command(command_args(&[
            RMF_READ_ONLY_HOST_COMMAND,
            "--domain-id",
            "233",
            "--source-id",
            "rmf/hotel-production",
            "--max-silence-ms",
            "3000",
            "--floor",
            "L1=1",
            "--bind",
            "Lift1=A:operational",
        ]))
        .expect_err("DDS port arithmetic limits ROS 2 domain IDs");
        assert!(invalid_domain.contains("maximum domain ID 232"));
    }

    #[test]
    fn production_diagnostics_distinguish_layers_and_preserve_context() {
        let mut host = reference_host();
        let source = reference_source_id();
        let session = reference_session_id();

        host.note_transport_subscriber_created(&source);
        host.note_transport_discovery(&source);
        host.record_mapping_rejection(
            source.clone(),
            session.clone(),
            "Lift-native".into(),
            "fixture mapping evidence".into(),
        );
        host.record_transport_failure(
            source.clone(),
            session.clone(),
            "fixture transport evidence".into(),
        );
        host.ingest_observation(observation(1).into());
        host.ingest_observation(
            complete_observation(source.clone(), session.clone(), 2, Floor::new(4)).into(),
        );

        let entries = host.diagnostics().entries().collect::<Vec<_>>();
        for (layer, id) in [
            (
                HostDiagnosticLayer::Transport,
                HostDiagnosticId::TransportSubscriberCreated,
            ),
            (
                HostDiagnosticLayer::Transport,
                HostDiagnosticId::TransportDiscoveryObserved,
            ),
            (
                HostDiagnosticLayer::Transport,
                HostDiagnosticId::TransportFailed,
            ),
            (
                HostDiagnosticLayer::Source,
                HostDiagnosticId::SourceSessionActivated,
            ),
            (
                HostDiagnosticLayer::Mapping,
                HostDiagnosticId::MappingRejected,
            ),
            (
                HostDiagnosticLayer::Observation,
                HostDiagnosticId::ObservationUnresolved,
            ),
            (
                HostDiagnosticLayer::Observation,
                HostDiagnosticId::ObservationResolved,
            ),
            (
                HostDiagnosticLayer::Readiness,
                HostDiagnosticId::ReadinessChanged,
            ),
        ] {
            assert!(
                entries
                    .iter()
                    .any(|entry| entry.layer == layer && entry.id == id),
                "missing diagnostic layer={layer:?} id={id:?}"
            );
            assert!(!id.as_str().contains(' '));
        }
        let mapping = entries
            .iter()
            .find(|entry| entry.id == HostDiagnosticId::MappingRejected)
            .expect("mapping rejection remains inspectable");
        assert_eq!(mapping.context.source.as_ref(), Some(&source));
        assert_eq!(mapping.context.session.as_ref(), Some(&session));
        assert_eq!(
            mapping.context.native_entity.as_deref(),
            Some("Lift-native")
        );
        let rendered = render_host_diagnostic(mapping);
        assert!(rendered.contains("layer=mapping id=mapping.rejected"));
        assert!(rendered.contains("native_entity=Some(\"Lift-native\")"));
        let unresolved = entries
            .iter()
            .find(|entry| entry.id == HostDiagnosticId::ObservationUnresolved)
            .expect("resolution loss has an explicit diagnostic");
        assert_eq!(unresolved.context.car.as_ref(), Some(&reference_car_id()));
        assert_eq!(
            unresolved.context.role,
            Some(ObservationBindingRole::Operational)
        );
        assert!(
            host.engine
                .diagnostics()
                .entries()
                .any(|entry| matches!(entry.event, DiagnosticEvent::Observation { .. }))
        );
    }

    #[test]
    fn host_diagnostics_are_bounded_without_reusing_machine_sequence() {
        let mut diagnostics = HostDiagnostics::new(3);
        for lifecycle in [
            HostLifecycle::Starting,
            HostLifecycle::Synchronizing,
            HostLifecycle::ReadyReadOnly,
            HostLifecycle::Starting,
            HostLifecycle::Synchronizing,
        ] {
            diagnostics.record(HostEvent::LifecycleChanged(lifecycle));
        }

        let sequences = diagnostics
            .entries()
            .map(|entry| entry.sequence)
            .collect::<Vec<_>>();
        assert_eq!(sequences, vec![3, 4, 5]);
    }

    #[test]
    fn incident_capsule_round_trip_replays_lifecycle_and_sequence_trigger() {
        let capsule = capture_reference_incident_capsule()
            .expect("deterministic fixture produces an incident capsule");
        let json = capsule.to_json().expect("internal capsule encodes");
        assert!(json.contains("liftassistant.incident-capsule/1"));
        assert!(json.contains("invalidate_source"));
        assert!(json.contains("begin_source_session"));
        let decoded = IncidentCapsule::from_json(&json).expect("internal capsule decodes");
        assert_eq!(decoded, capsule);

        let report = decoded
            .replay()
            .expect("fresh host replay reproduces the incident");
        assert_eq!(report.trigger_id, "observation.sequence-not-increasing");
        assert_eq!(report.action_count, 8);
        assert!(report.render().contains("incident replay: PASS"));
    }

    #[test]
    fn incident_capsule_rejects_unknown_version() {
        let capsule = capture_reference_incident_capsule()
            .expect("deterministic fixture produces an incident capsule");
        let json = capsule
            .to_json()
            .expect("internal capsule encodes")
            .replace(
                "liftassistant.incident-capsule/1",
                "liftassistant.incident-capsule/999",
            );

        let error = IncidentCapsule::from_json(&json)
            .expect_err("unknown internal capsule versions fail explicitly");
        assert!(error.contains("unsupported incident capsule version"));
    }

    #[test]
    fn configured_stale_trigger_writes_one_replayable_capsule() {
        let path = env::temp_dir().join(format!(
            "liftassistant-stale-capsule-test-{}.json",
            std::process::id()
        ));
        let mut host = reference_host();
        host.ingest_observation(
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                1,
                Floor::new(4),
            )
            .into(),
        );
        host.mark_source_payload_stale(reference_source_id(), reference_session_id())
            .expect("fixture source can become stale");
        let mut output = Some(path.clone());
        write_source_stale_capsule(
            &host,
            &mut output,
            &reference_source_id(),
            &reference_session_id(),
        )
        .expect("configured incident path captures the first stale trigger");
        assert_eq!(output, None);

        let report = IncidentCapsule::read(&path)
            .and_then(|capsule| capsule.replay())
            .expect("captured stale capsule replays in a fresh host");
        let cleanup = std::fs::remove_file(&path);
        cleanup.expect("fixture capsule is removed");
        assert_eq!(report.trigger_id, "source.payload-stale");
    }

    #[test]
    fn bounded_incident_capsule_replays_after_long_running_action_eviction() {
        let source = ObservationSourceId::new("fixture/long-running-rmf");
        let session = ObservationSessionId::new("long-running-session-A");
        let car_a = CarId::new("A");
        let car_b = CarId::new("B");
        let mut host = ReadOnlyHost::new(
            BuildingTopology::new([Floor::new(1), Floor::new(3), Floor::new(7)])
                .expect("valid topology"),
        );
        for car in [car_a.clone(), car_b.clone()] {
            host.bind_observation(source.clone(), car, ObservationBindingRole::Operational)
                .expect("explicit operational binding");
        }
        host.begin_source_session(source.clone(), session.clone())
            .expect("session starts once at startup");

        let mut initial_b = complete_observation(source.clone(), session.clone(), 1, Floor::new(7));
        initial_b.car = car_b.clone();
        host.ingest_observation(initial_b.into());
        let mut initial_a = complete_observation(source.clone(), session.clone(), 2, Floor::new(1));
        initial_a.car = car_a.clone();
        host.ingest_observation(initial_a.into());
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        // Exceed INCIDENT_ACTION_CAPACITY (128) with incremental updates on car A so
        // BeginSourceSession, car B's snapshot, and car A's base fields are all evicted.
        for sequence in 3..=200 {
            let floor = if sequence % 2 == 0 {
                Floor::new(1)
            } else {
                Floor::new(3)
            };
            host.ingest_observation(CarObservationUpdate {
                provenance: ObservationProvenance {
                    source: source.clone(),
                    session: session.clone(),
                    sequence: ObservationSequence::new(sequence),
                },
                car: car_a.clone(),
                position: Some(PositionEvidence::FloorReference(floor)),
                motion: None,
                door: None,
                mode: None,
                reported_destination: None,
            });
        }
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        // Trigger 1: incremental resolution loss on car A after window rollover.
        host.ingest_observation(CarObservationUpdate {
            provenance: ObservationProvenance {
                source: source.clone(),
                session: session.clone(),
                sequence: ObservationSequence::new(201),
            },
            car: car_a.clone(),
            position: None,
            motion: None,
            door: Some(DoorEvidence::MovingUnknownDirection),
            mode: None,
            reported_destination: None,
        });
        assert_eq!(host.readiness(), Readiness::NotReady);
        let unresolved_capsule = host.capture_incident(CapsuleTrigger::ObservationUnresolved {
            source: source.as_str().to_owned(),
            session: session.as_str().to_owned(),
            car: car_a.as_str().to_owned(),
        });
        let unresolved_json = unresolved_capsule
            .to_json()
            .expect("post-eviction unresolved capsule encodes");
        let unresolved_report = IncidentCapsule::from_json(&unresolved_json)
            .and_then(|decoded| decoded.replay())
            .expect("post-eviction unresolved capsule replays in a fresh host");
        assert_eq!(unresolved_report.trigger_id, "observation.unresolved");

        // Recover car A, then trigger 2: payload staleness after >200 actions.
        host.ingest_observation(CarObservationUpdate {
            provenance: ObservationProvenance {
                source: source.clone(),
                session: session.clone(),
                sequence: ObservationSequence::new(202),
            },
            car: car_a.clone(),
            position: None,
            motion: None,
            door: Some(DoorEvidence::Known(DoorState::Closed)),
            mode: None,
            reported_destination: None,
        });
        assert_eq!(host.readiness(), Readiness::ReadOnly);
        host.mark_source_payload_stale(source.clone(), session.clone())
            .expect("long-running source becomes stale");
        let stale_capsule =
            host.capture_incident(CapsuleTrigger::source_payload_stale(&source, &session));
        let stale_report = stale_capsule
            .replay()
            .expect("post-eviction stale capsule replays in a fresh host");
        assert_eq!(stale_report.trigger_id, "source.payload-stale");
        let (sanitized, sanitization) = stale_capsule
            .sanitize_candidate()
            .expect("post-eviction stale capsule sanitizes");
        assert!(sanitization.manual_review_required);
        assert_eq!(
            sanitized
                .replay()
                .expect("sanitized post-eviction capsule replays")
                .trigger_id,
            "source.payload-stale"
        );
    }

    #[test]
    fn evicted_invalidation_retains_sequence_watermark_and_max_action_bound() {
        let op_source = ObservationSourceId::new("fixture/op-watermark");
        let op_session = ObservationSessionId::new("op-session-1");
        let aux_source = ObservationSourceId::new("fixture/aux-flood");
        let aux_session = ObservationSessionId::new("aux-session-1");
        let car = CarId::new("A");
        let topology = BuildingTopology::new([Floor::new(9), Floor::new(2)])
            .expect("valid non-contiguous topology");
        let mut host = ReadOnlyHost::new(topology.clone());
        host.incident_recorder = IncidentRecorder::new(256);
        host.bind_observation(
            op_source.clone(),
            car.clone(),
            ObservationBindingRole::Operational,
        )
        .expect("operational binding");
        host.bind_observation(
            aux_source.clone(),
            car.clone(),
            ObservationBindingRole::Auxiliary,
        )
        .expect("auxiliary binding");
        host.begin_source_session(op_source.clone(), op_session.clone())
            .expect("operational session");
        host.begin_source_session(aux_source.clone(), aux_session.clone())
            .expect("auxiliary session");

        let mut op_initial =
            complete_observation(op_source.clone(), op_session.clone(), 50, Floor::new(2));
        op_initial.car = car.clone();
        host.ingest_observation(op_initial.into());
        host.mark_source_payload_stale(op_source.clone(), op_session.clone())
            .expect("operational source invalidated at watermark 50");

        // Flood 270 auxiliary updates so operational session, seq=50, and invalidation are evicted
        // even at the 256-action capsule ceiling.
        for sequence in 1..=270 {
            let mut aux_obs = complete_observation(
                aux_source.clone(),
                aux_session.clone(),
                sequence,
                Floor::new(9),
            );
            aux_obs.car = car.clone();
            host.ingest_observation(aux_obs.into());
        }

        // Delayed duplicate seq=50 on invalidated operational source must still reproduce anomaly.
        let mut duplicate_op =
            complete_observation(op_source.clone(), op_session.clone(), 50, Floor::new(2));
        duplicate_op.car = car.clone();
        host.ingest_observation(duplicate_op.into());
        let anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source: op_source,
            session: op_session,
            previous: ObservationSequence::new(50),
            received: ObservationSequence::new(50),
        };
        let trigger = CapsuleTrigger::observation_sequence_not_increasing(
            &car,
            ObservationBindingRole::Operational,
            &anomaly,
        )
        .expect("sequence anomaly trigger");
        let capsule = host.capture_incident(trigger);
        let report = capsule
            .replay()
            .expect("max-capacity capsule with evicted watermark replays");
        assert!(report.action_count <= 256);
        assert_eq!(report.trigger_id, "observation.sequence-not-increasing");

        // Also verify unsorted JSON topology floors still sanitize monotonically without order flip.
        let unsorted_json = capsule
            .to_json()
            .expect("capsule encodes")
            .replace("[\n      2,\n      9\n    ]", "[9, 2]");
        let unsorted =
            IncidentCapsule::from_json(&unsorted_json).expect("unsorted topology decodes");
        let (candidate, _) = unsorted
            .sanitize_candidate()
            .expect("unsorted topology still sanitizes canonically");
        let candidate_json = candidate.to_json().expect("candidate encodes");
        assert!(candidate_json.contains("[\n      1,\n      2\n    ]"));
        assert_eq!(
            candidate
                .replay()
                .expect("sanitized unsorted-topology capsule replays")
                .trigger_id,
            "observation.sequence-not-increasing"
        );
    }

    #[test]
    fn mixed_role_source_preserves_synchronizing_across_session_reset_and_invalidation_eviction() {
        let source = ObservationSourceId::new("fixture/mixed-role-source");
        let car_op = CarId::new("A");
        let car_aux = CarId::new("B");
        let topology =
            BuildingTopology::new([Floor::new(1), Floor::new(3)]).expect("valid topology");

        // Subcase A: Operational resolved in s1, reset to s2 (Synchronizing), then >128 Auxiliary updates.
        let mut host = ReadOnlyHost::new(topology.clone());
        host.bind_observation(
            source.clone(),
            car_op.clone(),
            ObservationBindingRole::Operational,
        )
        .expect("operational car binding");
        host.bind_observation(
            source.clone(),
            car_aux.clone(),
            ObservationBindingRole::Auxiliary,
        )
        .expect("auxiliary car binding");
        let session_1 = ObservationSessionId::new("mixed-session-1");
        let session_2 = ObservationSessionId::new("mixed-session-2");
        host.begin_source_session(source.clone(), session_1.clone())
            .expect("session 1");
        let mut op_initial = complete_observation(source.clone(), session_1, 1, Floor::new(1));
        op_initial.car = car_op.clone();
        host.ingest_observation(op_initial.into());
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);

        host.begin_source_session(source.clone(), session_2.clone())
            .expect("session 2 invalidates operational car A");
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);

        for sequence in 1..=135 {
            let mut aux_obs =
                complete_observation(source.clone(), session_2.clone(), sequence, Floor::new(3));
            aux_obs.car = car_aux.clone();
            host.ingest_observation(aux_obs.into());
        }
        let mut duplicate_aux =
            complete_observation(source.clone(), session_2.clone(), 135, Floor::new(3));
        duplicate_aux.car = car_aux.clone();
        host.ingest_observation(duplicate_aux.into());
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);

        let anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source: source.clone(),
            session: session_2.clone(),
            previous: ObservationSequence::new(135),
            received: ObservationSequence::new(135),
        };
        let trigger = CapsuleTrigger::observation_sequence_not_increasing(
            &car_aux,
            ObservationBindingRole::Auxiliary,
            &anomaly,
        )
        .expect("auxiliary sequence anomaly trigger");
        let reset_capsule = host.capture_incident(trigger);
        let reset_report = reset_capsule
            .replay()
            .expect("mixed-role post-reset synchronizing capsule replays");
        assert_eq!(
            reset_report.trigger_id,
            "observation.sequence-not-increasing"
        );
        let (sanitized_reset, _) = reset_capsule
            .sanitize_candidate()
            .expect("mixed-role post-reset capsule sanitizes");
        sanitized_reset
            .replay()
            .expect("sanitized mixed-role post-reset capsule replays");

        // Subcase B: Operational invalidated in s1, then >128 Auxiliary updates while Operational stays silent.
        let mut stale_host = ReadOnlyHost::new(topology);
        stale_host
            .bind_observation(
                source.clone(),
                car_op.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("operational car binding");
        stale_host
            .bind_observation(
                source.clone(),
                car_aux.clone(),
                ObservationBindingRole::Auxiliary,
            )
            .expect("auxiliary car binding");
        let stale_session = ObservationSessionId::new("mixed-stale-session");
        stale_host
            .begin_source_session(source.clone(), stale_session.clone())
            .expect("stale session");
        let mut op_obs =
            complete_observation(source.clone(), stale_session.clone(), 1, Floor::new(1));
        op_obs.car = car_op;
        stale_host.ingest_observation(op_obs.into());
        stale_host
            .mark_source_payload_stale(source.clone(), stale_session.clone())
            .expect("operational source marked stale");
        assert_eq!(stale_host.lifecycle(), &HostLifecycle::Synchronizing);

        for sequence in 2..=136 {
            let mut aux_obs = complete_observation(
                source.clone(),
                stale_session.clone(),
                sequence,
                Floor::new(3),
            );
            aux_obs.car = car_aux.clone();
            stale_host.ingest_observation(aux_obs.into());
        }
        let mut dup =
            complete_observation(source.clone(), stale_session.clone(), 136, Floor::new(3));
        dup.car = car_aux.clone();
        stale_host.ingest_observation(dup.into());
        assert_eq!(stale_host.lifecycle(), &HostLifecycle::Synchronizing);

        let stale_anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source,
            session: stale_session,
            previous: ObservationSequence::new(136),
            received: ObservationSequence::new(136),
        };
        let stale_trigger = CapsuleTrigger::observation_sequence_not_increasing(
            &car_aux,
            ObservationBindingRole::Auxiliary,
            &stale_anomaly,
        )
        .expect("auxiliary sequence trigger");
        stale_host
            .capture_incident(stale_trigger)
            .replay()
            .expect("mixed-role post-invalidation synchronizing capsule replays");
    }

    #[test]
    fn evicted_operational_degradation_preserves_exact_session_and_sequence_context() {
        let topology =
            BuildingTopology::new([Floor::new(1), Floor::new(2)]).expect("valid topology");

        // Subcase A: Incomplete operational observation + duplicate seq in s1, followed by s2 reset and >128 aux updates.
        let s1 = ObservationSourceId::new("fixture/op-source");
        let s2 = ObservationSourceId::new("fixture/aux-source");
        let car_a = CarId::new("A");
        let car_b = CarId::new("B");
        let mut host = ReadOnlyHost::new(topology.clone());
        host.bind_observation(
            s1.clone(),
            car_a.clone(),
            ObservationBindingRole::Operational,
        )
        .expect("bind A");
        host.bind_observation(s2.clone(), car_b.clone(), ObservationBindingRole::Auxiliary)
            .expect("bind B");
        let sess_1 = ObservationSessionId::new("op-sess-1");
        let sess_2 = ObservationSessionId::new("op-sess-2");
        let aux_sess = ObservationSessionId::new("aux-sess-1");
        host.begin_source_session(s1.clone(), sess_1.clone())
            .expect("start op-sess-1");
        host.begin_source_session(s2.clone(), aux_sess.clone())
            .expect("start aux-sess-1");

        let partial = CarObservationUpdate {
            provenance: ObservationProvenance {
                source: s1.clone(),
                session: sess_1.clone(),
                sequence: ObservationSequence::new(1),
            },
            car: car_a.clone(),
            position: Some(PositionEvidence::FloorReference(Floor::new(1))),
            motion: None,
            door: None,
            mode: None,
            reported_destination: None,
        };
        host.ingest_observation(partial.clone());
        host.ingest_observation(partial);
        assert!(matches!(host.lifecycle(), HostLifecycle::Degraded(_)));

        // Non-invalidating session reset (no resolved state in sess_1) leaves host Degraded.
        host.begin_source_session(s1.clone(), sess_2)
            .expect("start op-sess-2");
        assert!(matches!(host.lifecycle(), HostLifecycle::Degraded(_)));

        for sequence in 1..=135 {
            let mut aux_obs =
                complete_observation(s2.clone(), aux_sess.clone(), sequence, Floor::new(2));
            aux_obs.car = car_b.clone();
            host.ingest_observation(aux_obs.into());
        }
        let anomaly_a = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source: s1,
            session: sess_1,
            previous: ObservationSequence::new(1),
            received: ObservationSequence::new(1),
        };
        let trigger_a = CapsuleTrigger::observation_sequence_not_increasing(
            &car_a,
            ObservationBindingRole::Operational,
            &anomaly_a,
        )
        .expect("operational sequence trigger");
        let capsule_a = host.capture_incident(trigger_a);
        capsule_a
            .replay()
            .expect("evicted pre-session-reset operational anomaly replays with exact session");
        let (sanitized_a, _) = capsule_a
            .sanitize_candidate()
            .expect("pre-session-reset anomaly capsule sanitizes");
        sanitized_a
            .replay()
            .expect("sanitized pre-session-reset anomaly capsule replays");

        // Subcase B: Same source with Operational car A (anomaly at seq=5) + Auxiliary car B advancing seq=6..=145.
        let shared = ObservationSourceId::new("fixture/shared-source");
        let shared_sess = ObservationSessionId::new("shared-sess-1");
        let mut shared_host = ReadOnlyHost::new(topology);
        shared_host
            .bind_observation(
                shared.clone(),
                car_a.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("bind A operational");
        shared_host
            .bind_observation(
                shared.clone(),
                car_b.clone(),
                ObservationBindingRole::Auxiliary,
            )
            .expect("bind B auxiliary");
        shared_host
            .begin_source_session(shared.clone(), shared_sess.clone())
            .expect("start shared session");

        let mut op_five =
            complete_observation(shared.clone(), shared_sess.clone(), 5, Floor::new(1));
        op_five.car = car_a.clone();
        shared_host.ingest_observation(op_five.clone().into());
        shared_host.ingest_observation(op_five.into());
        assert!(matches!(
            shared_host.lifecycle(),
            HostLifecycle::Degraded(_)
        ));

        for sequence in 6..=145 {
            let mut aux_obs =
                complete_observation(shared.clone(), shared_sess.clone(), sequence, Floor::new(2));
            aux_obs.car = car_b.clone();
            shared_host.ingest_observation(aux_obs.into());
        }
        let anomaly_b = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source: shared,
            session: shared_sess,
            previous: ObservationSequence::new(5),
            received: ObservationSequence::new(5),
        };
        let trigger_b = CapsuleTrigger::observation_sequence_not_increasing(
            &car_a,
            ObservationBindingRole::Operational,
            &anomaly_b,
        )
        .expect("exact watermark=5 operational sequence trigger");
        shared_host
            .capture_incident(trigger_b)
            .replay()
            .expect("evicted anomaly before same-source auxiliary sequence advance replays");
    }

    #[test]
    fn unbound_and_pre_session_rejections_do_not_poison_incident_capsule_capture() {
        let source = ObservationSourceId::new("fixture/clean-recorder");
        let session = ObservationSessionId::new("clean-session-1");
        let car = CarId::new("A");
        let mut host = ReadOnlyHost::new(
            BuildingTopology::new([Floor::new(1), Floor::new(2)]).expect("valid topology"),
        );
        host.bind_observation(
            source.clone(),
            car.clone(),
            ObservationBindingRole::Operational,
        )
        .expect("bind operational car");

        // Pre-session observation before begin_source_session:
        let mut pre_session =
            complete_observation(source.clone(), session.clone(), 1, Floor::new(1));
        pre_session.car = car.clone();
        host.ingest_observation(pre_session.into());

        // Unbound car observation after session starts:
        host.begin_source_session(source.clone(), session.clone())
            .expect("begin session");
        let mut unbound_car =
            complete_observation(source.clone(), session.clone(), 1, Floor::new(1));
        unbound_car.car = CarId::new("UNBOUND");
        host.ingest_observation(unbound_car.into());

        // Unbound source observation:
        let unbound_source = complete_observation(
            ObservationSourceId::new("fixture/unbound-source"),
            ObservationSessionId::new("unbound-session"),
            1,
            Floor::new(1),
        );
        host.ingest_observation(unbound_source.into());

        // Normal valid ingestion followed by stale capture must encode and replay cleanly.
        let mut valid = complete_observation(source.clone(), session.clone(), 1, Floor::new(2));
        valid.car = car;
        host.ingest_observation(valid.into());
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        host.mark_source_payload_stale(source.clone(), session.clone())
            .expect("mark stale");
        let capsule =
            host.capture_incident(CapsuleTrigger::source_payload_stale(&source, &session));
        let json = capsule
            .to_json()
            .expect("unbound/pre-session rejections must not poison capsule validation");
        assert_eq!(
            IncidentCapsule::from_json(&json)
                .and_then(|decoded| decoded.replay())
                .expect("captured capsule replays cleanly")
                .trigger_id,
            "source.payload-stale"
        );
    }

    #[test]
    fn max_binding_compaction_and_multi_source_rollover_stay_within_capsule_bounds() {
        let topology =
            BuildingTopology::new([Floor::new(1), Floor::new(4)]).expect("valid topology");

        // 1 Operational + 255 Auxiliary cars on one source (256 total bindings = MAX_CAPSULE_BINDINGS).
        let source = ObservationSourceId::new("fixture/max-bindings");
        let session = ObservationSessionId::new("max-bindings-session");
        let mut dense_host = ReadOnlyHost::new(topology.clone());
        let op_car = CarId::new("op-000");
        dense_host
            .bind_observation(
                source.clone(),
                op_car.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("bind operational");
        for index in 1..=255 {
            dense_host
                .bind_observation(
                    source.clone(),
                    CarId::new(format!("aux-{index:03}")),
                    ObservationBindingRole::Auxiliary,
                )
                .expect("bind auxiliary");
        }
        dense_host
            .begin_source_session(source.clone(), session.clone())
            .expect("begin dense session");
        let mut op_obs = complete_observation(source.clone(), session.clone(), 1, Floor::new(1));
        op_obs.car = op_car.clone();
        dense_host.ingest_observation(op_obs.into());
        for index in 1..=255_u64 {
            let mut aux_obs =
                complete_observation(source.clone(), session.clone(), index + 1, Floor::new(4));
            aux_obs.car = CarId::new(format!("aux-{index:03}"));
            dense_host.ingest_observation(aux_obs.into());
        }
        dense_host.ingest_observation(CarObservationUpdate {
            provenance: ObservationProvenance {
                source: source.clone(),
                session: session.clone(),
                sequence: ObservationSequence::new(257),
            },
            car: op_car.clone(),
            position: None,
            motion: None,
            door: Some(DoorEvidence::MovingUnknownDirection),
            mode: None,
            reported_destination: None,
        });
        let dense_capsule = dense_host.capture_incident(CapsuleTrigger::ObservationUnresolved {
            source: source.as_str().to_owned(),
            session: session.as_str().to_owned(),
            car: op_car.as_str().to_owned(),
        });
        let dense_report = dense_capsule
            .replay()
            .expect("256-binding host compacts auxiliary preamble within MAX_CAPSULE_ACTIONS");
        assert!(dense_report.action_count <= 256);
        assert_eq!(dense_report.trigger_id, "observation.unresolved");

        // Multi-source (3 sources) interleaved session resets, invalidations, and inactive-session rejection across 256 rollover.
        let s_a = ObservationSourceId::new("fixture/multi-z");
        let s_b = ObservationSourceId::new("fixture/multi-a");
        let s_aux = ObservationSourceId::new("fixture/multi-m-aux");
        let car_1 = CarId::new("car-1");
        let car_2 = CarId::new("car-2");
        let mut multi = ReadOnlyHost::new(topology);
        multi.incident_recorder = IncidentRecorder::new(256);
        multi
            .bind_observation(
                s_a.clone(),
                car_1.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("bind s_a");
        multi
            .bind_observation(
                s_b.clone(),
                car_2.clone(),
                ObservationBindingRole::Operational,
            )
            .expect("bind s_b");
        multi
            .bind_observation(
                s_aux.clone(),
                car_1.clone(),
                ObservationBindingRole::Auxiliary,
            )
            .expect("bind s_aux");

        let sa_1 = ObservationSessionId::new("sa-1");
        let sa_2 = ObservationSessionId::new("sa-2");
        let sb_1 = ObservationSessionId::new("sb-1");
        let sb_2 = ObservationSessionId::new("sb-2");
        let saux_1 = ObservationSessionId::new("saux-1");
        multi
            .begin_source_session(s_a.clone(), sa_1.clone())
            .unwrap();
        multi
            .begin_source_session(s_b.clone(), sb_1.clone())
            .unwrap();
        multi
            .begin_source_session(s_aux.clone(), saux_1.clone())
            .unwrap();

        let mut c1 = complete_observation(s_a.clone(), sa_1.clone(), 1, Floor::new(1));
        c1.car = car_1.clone();
        multi.ingest_observation(c1.into());
        let mut c2 = complete_observation(s_b.clone(), sb_1.clone(), 1, Floor::new(4));
        c2.car = car_2.clone();
        multi.ingest_observation(c2.into());
        assert_eq!(multi.readiness(), Readiness::ReadOnly);

        // Interleave session resets + early inactive-session packet before activation + stale invalidation.
        let mut early_sa2 = complete_observation(s_a.clone(), sa_2.clone(), 1, Floor::new(1));
        early_sa2.car = car_1.clone();
        multi.ingest_observation(early_sa2.into());
        assert!(matches!(multi.lifecycle(), HostLifecycle::Degraded(_)));
        multi
            .begin_source_session(s_a.clone(), sa_2.clone())
            .unwrap();
        multi.mark_source_payload_stale(s_b.clone(), sb_1).unwrap();
        multi
            .begin_source_session(s_b.clone(), sb_2.clone())
            .unwrap();
        let mut c2_recovered = complete_observation(s_b.clone(), sb_2.clone(), 1, Floor::new(4));
        c2_recovered.car = car_2.clone();
        multi.ingest_observation(c2_recovered.into());
        let mut c1_recovered = complete_observation(s_a.clone(), sa_2.clone(), 1, Floor::new(1));
        c1_recovered.car = car_1.clone();
        multi.ingest_observation(c1_recovered.into());
        assert_eq!(multi.readiness(), Readiness::ReadOnly);

        // Delayed retired-session packet on s_a degrades host, then flood 260 auxiliary updates to roll window past 256.
        let mut delayed_sa1 = complete_observation(s_a.clone(), sa_1, 9, Floor::new(1));
        delayed_sa1.car = car_1.clone();
        multi.ingest_observation(delayed_sa1.into());
        assert!(matches!(multi.lifecycle(), HostLifecycle::Degraded(_)));

        for sequence in 1..=260 {
            let mut aux_obs =
                complete_observation(s_aux.clone(), saux_1.clone(), sequence, Floor::new(1));
            aux_obs.car = car_1.clone();
            multi.ingest_observation(aux_obs.into());
        }
        let mut dup_aux = complete_observation(s_aux.clone(), saux_1.clone(), 260, Floor::new(1));
        dup_aux.car = car_1.clone();
        multi.ingest_observation(dup_aux.into());

        let multi_anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source: s_aux,
            session: saux_1,
            previous: ObservationSequence::new(260),
            received: ObservationSequence::new(260),
        };
        let multi_trigger = CapsuleTrigger::observation_sequence_not_increasing(
            &car_1,
            ObservationBindingRole::Auxiliary,
            &multi_anomaly,
        )
        .expect("multi-source trigger");
        let multi_capsule = multi.capture_incident(multi_trigger);
        let multi_report = multi_capsule
            .replay()
            .expect("3-source interleaved session/invalidation/inactive rollover replays");
        assert!(multi_report.action_count <= 256);
        let (multi_sanitized, _) = multi_capsule
            .sanitize_candidate()
            .expect("3-source rollover capsule sanitizes");
        multi_sanitized
            .replay()
            .expect("sanitized 3-source rollover capsule replays");

        // Conflicting operational bindings in JSON must be rejected at decode validation.
        let conflicting_json = multi_capsule
            .to_json()
            .unwrap()
            .replace("\"role\": \"auxiliary\"", "\"role\": \"operational\"");
        assert!(
            IncidentCapsule::from_json(&conflicting_json)
                .expect_err("conflicting operational bindings must fail validation")
                .contains("conflicting operational bindings")
        );
    }

    #[test]
    fn sanitization_is_deterministic_replayable_and_requires_manual_review() {
        let private = capture_reference_incident_capsule()
            .expect("deterministic fixture produces a private capsule");
        let private_before = private.to_json().expect("private capsule encodes");
        let (first, report) = private
            .sanitize_candidate()
            .expect("private capsule produces a candidate");
        let (second, second_report) = private
            .sanitize_candidate()
            .expect("sanitization is repeatable");
        assert_eq!(first, second);
        assert_eq!(report, second_report);
        assert!(report.manual_review_required);
        assert!(report.render().contains("manual_review=REQUIRED"));

        let candidate_json = first.to_json().expect("candidate encodes");
        for private_value in [
            "fixture/capsule-source",
            "capsule-session-A",
            "capsule-session-B",
            "capsule-car",
        ] {
            assert!(!candidate_json.contains(private_value));
        }
        assert!(candidate_json.contains("deterministic-pseudonymization/1"));
        assert!(candidate_json.contains("\"manual_review_required\": true"));
        assert_eq!(
            first
                .replay()
                .expect("candidate reproduces the incident")
                .trigger_id,
            private
                .replay()
                .expect("private capsule reproduces the incident")
                .trigger_id
        );
        assert_eq!(
            private.to_json().expect("private capsule still encodes"),
            private_before
        );
        assert!(
            first
                .sanitize_candidate()
                .expect_err("candidate cannot be silently sanitized twice")
                .contains("already a sanitized candidate")
        );
    }

    #[test]
    fn recorded_bacnet_exercises_protocol_neutral_m5_host_paths() {
        let source = ObservationSourceId::new("fixture/bacnet-platform-check");
        let session_a = ObservationSessionId::new("bacnet-platform-session-A");
        let session_b = ObservationSessionId::new("bacnet-platform-session-B");
        let car = CarId::new("logical-bacnet-car");
        let floors = [(1, 1), (2, 2), (3, 3)]
            .into_iter()
            .map(|(universal, logical)| (universal, Floor::new(logical)))
            .collect::<BTreeMap<_, _>>();
        let properties = RecordedBacnetLiftProperties {
            object_name: "BACnet Lift Object 7".into(),
            car_position: 1,
            car_moving_direction: BacnetLiftCarDirection::Stopped,
            car_door_status: vec![BacnetDoorStatus::Closed],
            car_mode: Some(BacnetLiftCarMode::Normal),
            next_stopping_floor: Some(1),
            out_of_service: false,
        };
        let provenance = |session: &ObservationSessionId, sequence| ObservationProvenance {
            source: source.clone(),
            session: session.clone(),
            sequence: ObservationSequence::new(sequence),
        };
        let mapped = map_bacnet_lift_properties(
            &properties,
            car.clone(),
            &floors,
            provenance(&session_a, 1),
        )
        .expect("recorded BACnet properties map through their existing adapter boundary");
        assert_eq!(mapped.source_context.object_name, "BACnet Lift Object 7");
        assert_ne!(
            mapped.source_context.object_name,
            mapped.observation.car.as_str()
        );

        let mut host = ReadOnlyHost::new(mapped.topology.clone());
        host.bind_observation(
            source.clone(),
            car.clone(),
            ObservationBindingRole::Operational,
        )
        .expect("BACnet source is explicitly bound");
        host.begin_source_session(source.clone(), session_a.clone())
            .expect("BACnet source session starts explicitly");

        let mut unknown_floor = properties.clone();
        unknown_floor.car_position = 99;
        let mapping_error = map_bacnet_lift_properties(
            &unknown_floor,
            car.clone(),
            &floors,
            provenance(&session_a, 1),
        )
        .expect_err("unconfigured BACnet universal floor is rejected");
        assert_eq!(mapping_error, BacnetMappingError::UnknownUniversalFloor(99));
        host.record_mapping_rejection(
            source.clone(),
            session_a.clone(),
            unknown_floor.object_name,
            mapping_error.to_string(),
        );
        assert!(host.diagnostics().entries().any(|entry| {
            entry.layer == HostDiagnosticLayer::Mapping
                && entry.id == HostDiagnosticId::MappingRejected
                && entry.context.source.as_ref() == Some(&source)
                && entry.context.session.as_ref() == Some(&session_a)
                && entry.context.native_entity.as_deref() == Some("BACnet Lift Object 7")
        }));

        host.ingest_observation(mapped.observation.into());
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
        assert_eq!(
            host.source_health(&source)
                .expect("source health is protocol-neutral")
                .payload,
            PayloadActivity::Active
        );

        host.mark_source_payload_stale(source.clone(), session_a.clone())
            .expect("host applies explicit source-specific freshness decision");
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.engine.car_states().is_empty());
        assert_eq!(
            host.source_health(&source)
                .expect("stale BACnet source remains inspectable")
                .payload,
            PayloadActivity::Stale
        );

        let capsule =
            host.capture_incident(CapsuleTrigger::source_payload_stale(&source, &session_a));
        let replay = capsule
            .replay()
            .expect("BACnet-origin canonical capsule replays in a fresh host");
        assert_eq!(replay.trigger_id, "source.payload-stale");
        let (candidate, sanitization) = capsule
            .sanitize_candidate()
            .expect("BACnet-origin capsule uses the protocol-neutral sanitization path");
        assert!(sanitization.manual_review_required);
        assert_eq!(
            candidate
                .replay()
                .expect("sanitized BACnet-origin capsule still replays")
                .trigger_id,
            replay.trigger_id
        );
        let candidate_json = candidate.to_json().expect("candidate encodes");
        for private_value in [
            "fixture/bacnet-platform-check",
            "bacnet-platform-session-A",
            "logical-bacnet-car",
        ] {
            assert!(!candidate_json.contains(private_value));
        }

        let mut recovered_properties = properties.clone();
        recovered_properties.car_position = 2;
        recovered_properties.next_stopping_floor = Some(2);
        let recovered = map_bacnet_lift_properties(
            &recovered_properties,
            car.clone(),
            &floors,
            provenance(&session_a, 2),
        )
        .expect("newer same-session BACnet snapshot maps");
        host.ingest_observation(recovered.observation.into());
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let mut ambiguous_properties = recovered_properties.clone();
        ambiguous_properties.car_door_status =
            vec![BacnetDoorStatus::Opened, BacnetDoorStatus::Closed];
        let ambiguous = map_bacnet_lift_properties(
            &ambiguous_properties,
            car.clone(),
            &floors,
            provenance(&session_a, 3),
        )
        .expect("multi-door disagreement remains valid source evidence");
        assert_eq!(ambiguous.observation.door, DoorEvidence::Unknown);
        assert_eq!(
            ambiguous.source_context.door_statuses,
            [BacnetDoorStatus::Opened, BacnetDoorStatus::Closed]
        );
        host.ingest_observation(ambiguous.observation.into());
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.diagnostics().entries().any(|entry| {
            entry.id == HostDiagnosticId::ObservationUnresolved
                && entry.context.source.as_ref() == Some(&source)
                && entry.context.car.as_ref() == Some(&car)
                && entry.context.role == Some(ObservationBindingRole::Operational)
        }));

        host.begin_source_session(source.clone(), session_b.clone())
            .expect("BACnet source restart is an explicit lifecycle action");
        assert!(host.engine.evidence(&source, &car).is_none());
        let restarted = map_bacnet_lift_properties(
            &properties,
            car.clone(),
            &floors,
            provenance(&session_b, 1),
        )
        .expect("new BACnet source session starts at sequence one");
        host.ingest_observation(restarted.observation.into());
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let delayed = map_bacnet_lift_properties(
            &properties,
            car.clone(),
            &floors,
            provenance(&session_a, 4),
        )
        .expect("delayed old-session snapshot remains syntactically mappable");
        host.ingest_observation(delayed.observation.into());
        assert!(matches!(
            host.lifecycle(),
            HostLifecycle::Degraded(DegradedReason::ObservationRejected(
                ObservationIngestError::InactiveSourceSession { active, received, .. }
            )) if active == &session_b && received == &session_a
        ));
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.diagnostics().entries().any(|entry| {
            entry.id == HostDiagnosticId::ObservationRejected
                && entry.context.source.as_ref() == Some(&source)
                && entry.context.session.as_ref() == Some(&session_a)
                && entry.context.car.as_ref() == Some(&car)
        }));
    }

    #[test]
    fn a_started_process_is_not_ready_before_synchronization() {
        let host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());

        assert_eq!(host.lifecycle(), &HostLifecycle::Starting);
        assert_eq!(host.readiness(), Readiness::NotReady);
    }

    #[test]
    fn recorded_observations_drive_read_only_readiness_and_degradation() {
        let run = execute_proof();

        verify_proof(&run).expect("reference proof should satisfy lifecycle invariants");
        assert!(run.events.iter().any(|event| matches!(
            event,
            HostEvent::ObservationUnresolved { reasons, .. }
                if reasons == &[UnresolvedCarStateReason::DoorDirectionUnknown]
        )));
        assert!(run.events.iter().any(|event| matches!(
            event,
            HostEvent::StateUpdated { state, .. }
                if state.location
                    == CarLocation::Continuous(Position::at_floor(Floor::new(4)))
        )));
    }

    #[test]
    fn readiness_waits_for_every_configured_operational_car() {
        let source = ObservationSourceId::new("fixture/two-car-source");
        let session = ObservationSessionId::new("two-car-session-A");
        let mut host = ReadOnlyHost::new(
            BuildingTopology::new([Floor::new(1), Floor::new(2)]).expect("valid topology"),
        );
        for car in [CarId::new("A"), CarId::new("B")] {
            host.bind_observation(source.clone(), car, ObservationBindingRole::Operational)
                .expect("each logical car has one authority");
        }
        host.begin_source_session(source.clone(), session.clone())
            .expect("source starts explicitly");

        let mut first = complete_observation(source.clone(), session.clone(), 1, Floor::new(1));
        first.car = CarId::new("A");
        host.ingest_observation(first.into());
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);

        let mut second = complete_observation(source, session, 2, Floor::new(2));
        second.car = CarId::new("B");
        host.ingest_observation(second.into());
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
    }

    #[test]
    fn executable_proof_is_deterministic_and_reports_no_write_boundary() {
        let report = run_scenario(LIFECYCLE_SCENARIO).expect("reference scenario should pass");

        assert!(report.contains("determinism: PASS"));
        assert!(report.contains("process alive != ready: PASS"));
        assert!(report.contains("no IntentSink implementation or simulator dependency"));
        assert!(report.contains("duplicate sequence revoked readiness"));
    }

    #[test]
    fn unknown_scenario_is_rejected() {
        assert!(
            run_scenario("unknown")
                .unwrap_err()
                .contains("unknown scenario")
        );
    }

    #[test]
    fn proof_diagnostics_preserve_machine_readable_anomaly() {
        let mut source = reference_source();
        let mut host = reference_host();
        while host.poll(&mut source) {}

        assert!(host.engine.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            DiagnosticEvent::Anomaly(anomaly) if anomaly == &expected_anomaly()
        )));
    }

    #[test]
    fn new_incidents_still_revoke_readiness_after_incident_retention_is_full() {
        let complete = complete_observation(
            reference_source_id(),
            reference_session_id(),
            2,
            Floor::new(4),
        );
        let mut frames = vec![observation(1), complete.clone()];
        frames.extend(std::iter::repeat_n(complete, 10));
        let mut source = RecordedObservationFeed::new(frames);
        let mut host = reference_host();

        while host.poll(&mut source) {}

        assert_eq!(host.engine.diagnostics().incidents().count(), 8);
        assert_eq!(
            host.diagnostics()
                .entries()
                .filter(|entry| matches!(entry.event, HostEvent::ObservationRejected { .. }))
                .count(),
            10
        );
        assert!(matches!(host.lifecycle(), HostLifecycle::Degraded(_)));
        assert_eq!(host.readiness(), Readiness::NotReady);
    }

    #[test]
    fn operational_session_reset_revokes_readiness_until_new_session_state_arrives() {
        let mut host = reference_host();
        let mut initial = RecordedObservationFeed::new([
            observation(1),
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                2,
                Floor::new(4),
            ),
        ]);
        while host.poll(&mut initial) {}
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let next_session = ObservationSessionId::new("headless-fixture-session-B");
        host.begin_source_session(reference_source_id(), next_session.clone())
            .expect("operational source explicitly starts a new session");
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.engine.car_states().is_empty());

        let mut resumed = RecordedObservationFeed::new([complete_observation(
            reference_source_id(),
            next_session,
            1,
            Floor::new(4),
        )]);
        assert!(host.poll(&mut resumed));
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
    }

    #[test]
    fn discovery_without_payload_does_not_establish_readiness() {
        let mut host = reference_host();

        host.note_transport_subscriber_created(&reference_source_id());
        host.note_transport_discovery(&reference_source_id());
        let discovered_revision = host.read_model().revision();
        host.note_transport_subscriber_created(&reference_source_id());

        let health = host
            .source_health(&reference_source_id())
            .expect("bound source has health dimensions");
        assert_eq!(
            health.transport_discovery,
            TransportDiscoveryState::DiscoveryObserved
        );
        assert_eq!(health.payload, PayloadActivity::Awaiting);
        assert_eq!(
            health.session,
            SourceSessionActivity::Active(reference_session_id())
        );
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.engine.car_states().is_empty());
        assert_eq!(host.read_model().revision(), discovered_revision);
    }

    #[test]
    fn operational_payload_staleness_revokes_truth_and_newer_payload_recovers() {
        let mut host = reference_host();
        host.ingest_observation(
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                1,
                Floor::new(4),
            )
            .into(),
        );
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let invalidation = host
            .mark_source_payload_stale(reference_source_id(), reference_session_id())
            .expect("the active source session can be explicitly invalidated");
        assert_eq!(invalidation.invalidated_cars, vec![reference_car_id()]);
        assert_eq!(host.lifecycle(), &HostLifecycle::Synchronizing);
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(host.engine.car_states().is_empty());
        let health = host
            .source_health(&reference_source_id())
            .expect("source health remains inspectable");
        assert_eq!(health.payload, PayloadActivity::Stale);
        assert_eq!(
            health.observations.get(&reference_car_id()),
            Some(&ObservationAvailability::Revoked)
        );
        assert!(host.engine.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            DiagnosticEvent::SourceObservationsInvalidated {
                source,
                session,
                cars,
            } if source == &reference_source_id()
                && session == &reference_session_id()
                && cars == std::slice::from_ref(&reference_car_id())
        )));

        host.ingest_observation(
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                2,
                Floor::new(4),
            )
            .into(),
        );
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
        let recovered = host
            .source_health(&reference_source_id())
            .expect("source health remains inspectable after recovery");
        assert_eq!(recovered.payload, PayloadActivity::Active);
        assert_eq!(
            recovered.observations.get(&reference_car_id()),
            Some(&ObservationAvailability::Resolved)
        );
    }

    #[test]
    fn auxiliary_payload_staleness_is_visible_without_revoking_operational_readiness() {
        let mut host = reference_host();
        host.ingest_observation(
            complete_observation(
                reference_source_id(),
                reference_session_id(),
                1,
                Floor::new(4),
            )
            .into(),
        );
        let operational = host.engine.car_states();
        let auxiliary_source = ObservationSourceId::new("fixture/auxiliary-rmf");
        let auxiliary_session = ObservationSessionId::new("auxiliary-session-A");
        host.bind_observation(
            auxiliary_source.clone(),
            reference_car_id(),
            ObservationBindingRole::Auxiliary,
        )
        .expect("auxiliary binding is explicit");
        host.begin_source_session(auxiliary_source.clone(), auxiliary_session.clone())
            .expect("auxiliary source starts explicitly");
        host.ingest_observation(
            complete_observation(
                auxiliary_source.clone(),
                auxiliary_session.clone(),
                1,
                Floor::new(1),
            )
            .into(),
        );

        host.mark_source_payload_stale(auxiliary_source.clone(), auxiliary_session.clone())
            .expect("auxiliary freshness is supervised independently");

        assert_eq!(host.engine.car_states(), operational);
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
        assert_eq!(
            host.source_health(&auxiliary_source)
                .expect("auxiliary health remains visible")
                .payload,
            PayloadActivity::Stale
        );
        assert!(host.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            HostEvent::SourcePayloadStale { source, session, .. }
                if source == &auxiliary_source && session == &auxiliary_session
        )));
    }

    #[test]
    fn auxiliary_sequence_anomaly_is_visible_without_revoking_operational_readiness() {
        let mut host = reference_host();
        let mut initial = RecordedObservationFeed::new([complete_observation(
            reference_source_id(),
            reference_session_id(),
            1,
            Floor::new(4),
        )]);
        assert!(host.poll(&mut initial));
        let operational = host.engine.car_states();
        assert_eq!(host.readiness(), Readiness::ReadOnly);

        let auxiliary_source = ObservationSourceId::new("fixture/auxiliary-rmf");
        let auxiliary_session = ObservationSessionId::new("auxiliary-session-A");
        host.bind_observation(
            auxiliary_source.clone(),
            reference_car_id(),
            ObservationBindingRole::Auxiliary,
        )
        .expect("auxiliary binding is explicit");
        host.begin_source_session(auxiliary_source.clone(), auxiliary_session.clone())
            .expect("auxiliary source starts explicitly");
        let auxiliary = complete_observation(
            auxiliary_source.clone(),
            auxiliary_session.clone(),
            1,
            Floor::new(1),
        );
        let mut feed = RecordedObservationFeed::new([auxiliary.clone(), auxiliary]);
        assert!(host.poll(&mut feed));
        assert!(host.poll(&mut feed));

        let anomaly = DiagnosticAnomaly::ObservationSequenceNotIncreasing {
            source: auxiliary_source.clone(),
            session: auxiliary_session,
            previous: ObservationSequence::new(1),
            received: ObservationSequence::new(1),
        };
        assert!(host.engine.diagnostics().entries().any(|entry| {
            matches!(&entry.event, DiagnosticEvent::Anomaly(observed) if observed == &anomaly)
        }));
        assert!(host.diagnostics().entries().any(|entry| matches!(
            &entry.event,
            HostEvent::ObservationRejected { anomaly: observed, .. } if observed == &anomaly
        )));
        assert!(host.diagnostics().entries().any(|entry| {
            entry.id == HostDiagnosticId::ObservationRejected
                && entry.context.source.as_ref() == Some(&auxiliary_source)
                && entry.context.car.as_ref() == Some(&reference_car_id())
                && entry.context.role == Some(ObservationBindingRole::Auxiliary)
        }));
        assert_eq!(host.engine.car_states(), operational);
        assert_eq!(host.lifecycle(), &HostLifecycle::ReadyReadOnly);
        assert_eq!(host.readiness(), Readiness::ReadOnly);
    }
}
