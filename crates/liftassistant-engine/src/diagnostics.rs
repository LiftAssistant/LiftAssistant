use std::collections::{BTreeMap, VecDeque};

use liftassistant_model::{
    CarId, CarObservationUpdate, Intent, ObservationSequence, ObservationSessionId,
    ObservationSourceId,
};
use liftassistant_runtime::RuntimeEvent;

use crate::{EngineEvent, ObservationBindingRole};

const INCIDENT_BEFORE_ENTRIES: usize = 8;
const INCIDENT_AFTER_ENTRIES: usize = 2;
const INCIDENT_LIMIT: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticSequence(u64);

impl DiagnosticSequence {
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// A detected sequence invariant violation, not a root cause or equipment fault.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticAnomaly {
    ObservationSequenceNotIncreasing {
        source: ObservationSourceId,
        session: ObservationSessionId,
        previous: ObservationSequence,
        received: ObservationSequence,
    },
    IntentAcceptedWithoutGeneration {
        intent: Intent,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum DiagnosticEvent {
    Observation {
        update: CarObservationUpdate,
        role: ObservationBindingRole,
    },
    Engine(EngineEvent),
    SourceObservationsInvalidated {
        source: ObservationSourceId,
        session: ObservationSessionId,
        cars: Vec<CarId>,
    },
    Anomaly(DiagnosticAnomaly),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiagnosticEntry {
    pub sequence: DiagnosticSequence,
    pub event: DiagnosticEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IncidentSnapshot {
    pub trigger: DiagnosticAnomaly,
    pub evidence: Vec<DiagnosticEntry>,
    pub after_window_complete: bool,
}

impl IncidentSnapshot {
    pub fn replay_anomalies(&self) -> Vec<DiagnosticAnomaly> {
        let mut detector = SequenceDetector::default();
        let mut anomalies = Vec::new();
        for entry in &self.evidence {
            if !matches!(entry.event, DiagnosticEvent::Anomaly(_)) {
                anomalies.extend(detector.observe(&entry.event));
            }
        }
        anomalies
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FlightRecorder {
    capacity: usize,
    next_sequence: u64,
    entries: VecDeque<DiagnosticEntry>,
    incidents: VecDeque<IncidentSnapshot>,
    active_after_remaining: Option<usize>,
    detector: SequenceDetector,
}

impl FlightRecorder {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            next_sequence: 1,
            entries: VecDeque::new(),
            incidents: VecDeque::new(),
            active_after_remaining: None,
            detector: SequenceDetector::default(),
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = &DiagnosticEntry> {
        self.entries.iter()
    }

    pub fn incidents(&self) -> impl Iterator<Item = &IncidentSnapshot> {
        self.incidents.iter()
    }

    pub(crate) fn forget_observation_stream(
        &mut self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
    ) {
        // Engine rejects retired-session observations before they reach this detector.
        self.detector
            .last_observation_sequence
            .remove(&(source.clone(), session.clone()));
    }

    pub(crate) fn record_observation(
        &mut self,
        update: CarObservationUpdate,
        role: ObservationBindingRole,
    ) {
        self.record(DiagnosticEvent::Observation { update, role });
    }

    pub(crate) fn record_engine_events(&mut self, events: &[EngineEvent]) {
        for event in events {
            self.record(DiagnosticEvent::Engine(event.clone()));
        }
    }

    pub(crate) fn record_source_invalidation(
        &mut self,
        source: ObservationSourceId,
        session: ObservationSessionId,
        cars: Vec<CarId>,
    ) {
        self.record(DiagnosticEvent::SourceObservationsInvalidated {
            source,
            session,
            cars,
        });
    }

    fn record(&mut self, event: DiagnosticEvent) {
        let anomalies = self.detector.observe(&event);
        self.push(event, true);
        for anomaly in anomalies {
            self.trigger(anomaly);
        }
    }

    fn trigger(&mut self, anomaly: DiagnosticAnomaly) {
        self.active_after_remaining.take();

        let mut evidence = self
            .entries
            .iter()
            .rev()
            .take(INCIDENT_BEFORE_ENTRIES)
            .cloned()
            .collect::<Vec<_>>();
        evidence.reverse();
        self.push(DiagnosticEvent::Anomaly(anomaly.clone()), false);
        evidence.push(
            self.entries
                .back()
                .cloned()
                .expect("anomaly was added to the flight recorder"),
        );
        if self.incidents.len() == INCIDENT_LIMIT {
            self.incidents.pop_front();
        }
        self.incidents.push_back(IncidentSnapshot {
            trigger: anomaly,
            evidence,
            after_window_complete: false,
        });
        self.active_after_remaining = Some(INCIDENT_AFTER_ENTRIES);
    }

    fn push(&mut self, event: DiagnosticEvent, append_to_active: bool) {
        let entry = DiagnosticEntry {
            sequence: DiagnosticSequence(self.next_sequence),
            event,
        };
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .expect("diagnostic sequence space exhausted");

        if self.entries.len() == self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry.clone());

        if append_to_active {
            if let Some(remaining) = self.active_after_remaining.as_mut() {
                let incident = self
                    .incidents
                    .back_mut()
                    .expect("active incident has a snapshot");
                incident.evidence.push(entry);
                *remaining -= 1;
                if *remaining == 0 {
                    incident.after_window_complete = true;
                    self.active_after_remaining = None;
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SequenceDetector {
    last_observation_sequence:
        BTreeMap<(ObservationSourceId, ObservationSessionId), ObservationSequence>,
    generated_intents: Vec<Intent>,
}

impl SequenceDetector {
    fn observe(&mut self, event: &DiagnosticEvent) -> Vec<DiagnosticAnomaly> {
        match event {
            DiagnosticEvent::Observation { update, .. } => {
                let source = &update.provenance.source;
                let session = &update.provenance.session;
                let received = update.provenance.sequence;
                let key = (source.clone(), session.clone());
                match self.last_observation_sequence.get(&key).copied() {
                    Some(previous) if received <= previous => {
                        vec![DiagnosticAnomaly::ObservationSequenceNotIncreasing {
                            source: source.clone(),
                            session: session.clone(),
                            previous,
                            received,
                        }]
                    }
                    _ => {
                        self.last_observation_sequence.insert(key, received);
                        Vec::new()
                    }
                }
            }
            DiagnosticEvent::Engine(EngineEvent::IntentGenerated { intent, .. }) => {
                self.generated_intents.push(intent.clone());
                Vec::new()
            }
            DiagnosticEvent::Engine(EngineEvent::IntentRejected { intent, .. }) => {
                if let Some(index) = self
                    .generated_intents
                    .iter()
                    .position(|generated| generated == intent)
                {
                    self.generated_intents.remove(index);
                }
                Vec::new()
            }
            DiagnosticEvent::Engine(EngineEvent::Runtime(RuntimeEvent::IntentAccepted(intent))) => {
                if let Some(index) = self
                    .generated_intents
                    .iter()
                    .position(|generated| generated == intent)
                {
                    self.generated_intents.remove(index);
                    Vec::new()
                } else {
                    vec![DiagnosticAnomaly::IntentAcceptedWithoutGeneration {
                        intent: intent.clone(),
                    }]
                }
            }
            DiagnosticEvent::Engine(_)
            | DiagnosticEvent::SourceObservationsInvalidated { .. }
            | DiagnosticEvent::Anomaly(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use liftassistant_model::{BuildingTopology, CarState, Floor, ObservationProvenance};

    use super::*;
    use crate::{Engine, SourceSessionError};

    #[test]
    fn sequence_detector_retains_only_the_current_stream_after_session_churn() {
        let mut engine = Engine::new(BuildingTopology::reference_eight_floor());
        let source = ObservationSourceId::new("churn");
        let first_session = ObservationSessionId::new("session-0");
        let car = CarState::new("A", Floor::new(1));
        engine
            .bind_observation(
                source.clone(),
                car.id.clone(),
                ObservationBindingRole::Operational,
            )
            .unwrap();
        for generation in 0..1024 {
            let session = ObservationSessionId::new(format!("session-{generation}"));
            engine
                .begin_source_session(source.clone(), session.clone())
                .unwrap();
            let update = CarObservationUpdate::from_state(
                ObservationProvenance {
                    source: source.clone(),
                    session,
                    sequence: ObservationSequence::new(1),
                },
                &car,
            );
            engine.ingest_observation_update(update.clone());
            engine.ingest_observation_update(update);
            assert_eq!(
                engine.diagnostics.detector.last_observation_sequence.len(),
                1
            );
        }
        assert_eq!(engine.diagnostics.incidents().count(), INCIDENT_LIMIT);
        assert_eq!(
            engine.diagnostics.entries().count(),
            crate::DEFAULT_DIAGNOSTIC_CAPACITY
        );
        assert!(matches!(
            engine.begin_source_session(source, first_session),
            Err(SourceSessionError::RetiredSession { .. })
        ));
        println!(
            "retired identity entries={} after 1024 activations",
            engine.retired_source_sessions.len()
        );
    }
}
