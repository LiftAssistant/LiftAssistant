#![forbid(unsafe_code)]

use std::{collections::BTreeMap, error::Error, fmt, time::Duration};

use liftassistant_model::{
    CarId, CarLocation, CarObservation, CarObservationUpdate, CarState, DoorEvidence, DoorState,
    Floor, Intent, IntentId, MotionEvidence, ObservationProvenance, ObservationSequence,
    ObservationSessionId, ObservationSourceId, OperatingModeEvidence, PositionEvidence,
    ReportedDestination,
};

#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeEvent {
    IntentAccepted(Intent),
    Observation(CarObservationUpdate),
    Arrived { car: CarId, floor: Floor },
    DoorStateChanged { car: CarId, state: DoorState },
}

/// Configured lower-controller identity. Vendor holder/session IDs stay inside its adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControllerId(String);

impl ControllerId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Submission {
    NotSubmitted,
    Submitted,
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReceiptStatus {
    Accepted,
    Rejected,
    Indeterminate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentReceipt {
    pub controller: ControllerId,
    pub intent: IntentId,
    pub status: ReceiptStatus,
}

/// A receipt and canonical observations arrive separately from submission.
pub trait ControlledIntentSink {
    fn controller_id(&self) -> &ControllerId;
    fn available(&self, car: &CarId) -> bool;
    fn submit(&mut self, id: IntentId, intent: &Intent) -> Submission;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeError {
    message: String,
}

impl RuntimeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for RuntimeError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnresolvedCarStateReason {
    PositionUnknown,
    MotionUnknown,
    DoorUnknown,
    DoorDirectionUnknown,
    OperatingModeUnknown,
    OperatingModeSourceSpecific,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CarStateResolution {
    Resolved(CarState),
    Unresolved {
        car: CarId,
        reasons: Vec<UnresolvedCarStateReason>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvidenceSample<T> {
    pub value: T,
    pub provenance: ObservationProvenance,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssembledCarObservation {
    pub latest_provenance: ObservationProvenance,
    pub car: CarId,
    pub position: Option<EvidenceSample<PositionEvidence>>,
    pub motion: Option<EvidenceSample<MotionEvidence>>,
    pub door: Option<EvidenceSample<DoorEvidence>>,
    pub mode: Option<EvidenceSample<OperatingModeEvidence>>,
    pub reported_destination: Option<EvidenceSample<ReportedDestination>>,
}

impl AssembledCarObservation {
    /// Returns a whole-car snapshot only after every canonical field has evidence.
    pub fn snapshot(&self) -> Option<CarObservation> {
        Some(CarObservation {
            provenance: self.latest_provenance.clone(),
            car: self.car.clone(),
            position: self.position.as_ref()?.value,
            motion: self.motion.as_ref()?.value,
            door: self.door.as_ref()?.value,
            mode: self.mode.as_ref()?.value,
            reported_destination: self.reported_destination.as_ref()?.value,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationAssemblyError {
    NonIncreasingSequence {
        source: ObservationSourceId,
        session: ObservationSessionId,
        previous: ObservationSequence,
        received: ObservationSequence,
    },
}

/// Deterministic in-memory assembly of incremental canonical evidence.
///
/// Evidence is never merged across configured sources. Sequence establishes ordering only; it
/// carries no wall-clock freshness meaning.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObservationAssembler {
    observations:
        BTreeMap<(ObservationSourceId, ObservationSessionId, CarId), AssembledCarObservation>,
    last_sequence: BTreeMap<(ObservationSourceId, ObservationSessionId), ObservationSequence>,
}

impl ObservationAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(
        &mut self,
        update: CarObservationUpdate,
    ) -> Result<AssembledCarObservation, ObservationAssemblyError> {
        let CarObservationUpdate {
            provenance,
            car,
            position,
            motion,
            door,
            mode,
            reported_destination,
        } = update;
        let stream = (provenance.source.clone(), provenance.session.clone());
        if let Some(previous) = self
            .last_sequence
            .get(&stream)
            .copied()
            .filter(|previous| provenance.sequence <= *previous)
        {
            return Err(ObservationAssemblyError::NonIncreasingSequence {
                source: provenance.source,
                session: provenance.session,
                previous,
                received: provenance.sequence,
            });
        }

        let key = (
            provenance.source.clone(),
            provenance.session.clone(),
            car.clone(),
        );
        let current = self
            .observations
            .entry(key)
            .or_insert_with(|| AssembledCarObservation {
                latest_provenance: provenance.clone(),
                car,
                position: None,
                motion: None,
                door: None,
                mode: None,
                reported_destination: None,
            });
        current.latest_provenance = provenance.clone();
        merge_sample(&mut current.position, position, &provenance);
        merge_sample(&mut current.motion, motion, &provenance);
        merge_sample(&mut current.door, door, &provenance);
        merge_sample(&mut current.mode, mode, &provenance);
        merge_sample(
            &mut current.reported_destination,
            reported_destination,
            &provenance,
        );
        self.last_sequence.insert(stream, provenance.sequence);
        Ok(current.clone())
    }

    pub fn current(
        &self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
        car: &CarId,
    ) -> Option<&AssembledCarObservation> {
        self.observations
            .get(&(source.clone(), session.clone(), car.clone()))
    }

    pub fn remove_source(&mut self, source: &ObservationSourceId) {
        self.observations
            .retain(|(candidate, _, _), _| candidate != source);
        self.last_sequence
            .retain(|(candidate, _), _| candidate != source);
    }

    /// Removes current evidence while preserving the active stream's ordering watermark.
    ///
    /// A host uses this after an explicit lifecycle/freshness decision. Keeping the last
    /// sequence prevents an old or duplicate sample in the same source session from restoring
    /// invalidated evidence. Starting a genuinely new source session still uses `remove_source`
    /// to reset both evidence and ordering.
    pub fn invalidate_source_evidence(&mut self, source: &ObservationSourceId) {
        self.observations
            .retain(|(candidate, _, _), _| candidate != source);
    }

    /// Removes one car's evidence without changing the source/session watermark.
    pub fn invalidate_car_evidence(
        &mut self,
        source: &ObservationSourceId,
        session: &ObservationSessionId,
        car: &CarId,
    ) {
        self.observations
            .remove(&(source.clone(), session.clone(), car.clone()));
    }
}

fn merge_sample<T>(
    current: &mut Option<EvidenceSample<T>>,
    update: Option<T>,
    provenance: &ObservationProvenance,
) {
    if let Some(value) = update {
        *current = Some(EvidenceSample {
            value,
            provenance: provenance.clone(),
        });
    }
}

/// Resolves one canonical observation without history, probability, or source-specific inference.
pub fn resolve_car_observation(observation: &CarObservation) -> CarStateResolution {
    let mut reasons = Vec::new();

    let location = match observation.position {
        PositionEvidence::Exact(position) => Some(CarLocation::Continuous(position)),
        PositionEvidence::FloorReference(floor) => Some(CarLocation::Floor(floor)),
        PositionEvidence::Unknown => {
            reasons.push(UnresolvedCarStateReason::PositionUnknown);
            None
        }
    };

    let motion = match observation.motion {
        MotionEvidence::Known(motion) => Some(motion),
        MotionEvidence::Unknown => {
            reasons.push(UnresolvedCarStateReason::MotionUnknown);
            None
        }
    };

    let door = match observation.door {
        DoorEvidence::Known(door) => Some(door),
        DoorEvidence::MovingUnknownDirection => {
            reasons.push(UnresolvedCarStateReason::DoorDirectionUnknown);
            None
        }
        DoorEvidence::Unknown => {
            reasons.push(UnresolvedCarStateReason::DoorUnknown);
            None
        }
    };

    let mode = match observation.mode {
        OperatingModeEvidence::Known(mode) => Some(mode),
        OperatingModeEvidence::Unknown => {
            reasons.push(UnresolvedCarStateReason::OperatingModeUnknown);
            None
        }
        OperatingModeEvidence::SourceSpecific => {
            reasons.push(UnresolvedCarStateReason::OperatingModeSourceSpecific);
            None
        }
    };

    match (location, motion, door, mode) {
        (Some(location), Some(motion), Some(door), Some(mode)) => {
            CarStateResolution::Resolved(CarState {
                id: observation.car.clone(),
                location,
                motion,
                door,
                mode,
                reported_destination: observation.reported_destination,
            })
        }
        _ => CarStateResolution::Unresolved {
            car: observation.car.clone(),
            reasons,
        },
    }
}

/// Submission boundary for LiftAssistant high-level intents.
pub trait IntentSink {
    fn submit_intent(&mut self, intent: Intent) -> Result<(), RuntimeError>;
}

/// Explicit-time driver used only by deterministic simulators and recorded replay.
pub trait SimulationDriver {
    fn advance(&mut self, elapsed: Duration) -> Vec<RuntimeEvent>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{
        Direction, MotionState, ObservationProvenance, ObservationSequence, ObservationSessionId,
        ObservationSourceId, OperatingMode, Position, ReportedDestination,
    };

    fn session() -> ObservationSessionId {
        ObservationSessionId::new("fixture-session-A")
    }

    fn observation() -> CarObservation {
        CarObservation {
            provenance: ObservationProvenance {
                source: ObservationSourceId::new("fixture/open-rmf"),
                session: session(),
                sequence: ObservationSequence::new(1),
            },
            car: CarId::new("lift-A"),
            position: PositionEvidence::FloorReference(Floor::new(3)),
            motion: MotionEvidence::Known(MotionState::Moving(Direction::Up)),
            door: DoorEvidence::MovingUnknownDirection,
            mode: OperatingModeEvidence::Known(OperatingMode::FireService),
            reported_destination: ReportedDestination::Known(Floor::new(4)),
        }
    }

    #[test]
    fn incomplete_evidence_is_not_promoted_to_operational_state() {
        assert_eq!(
            resolve_car_observation(&observation()),
            CarStateResolution::Unresolved {
                car: CarId::new("lift-A"),
                reasons: vec![UnresolvedCarStateReason::DoorDirectionUnknown],
            }
        );
    }

    #[test]
    fn stopped_open_floor_reference_resolves_at_floor() {
        let mut observation = observation();
        observation.provenance.sequence = ObservationSequence::new(2);
        observation.position = PositionEvidence::FloorReference(Floor::new(4));
        observation.motion = MotionEvidence::Known(MotionState::Stopped);
        observation.door = DoorEvidence::Known(DoorState::Open);
        observation.reported_destination = ReportedDestination::None;

        assert_eq!(
            resolve_car_observation(&observation),
            CarStateResolution::Resolved(CarState {
                id: CarId::new("lift-A"),
                location: CarLocation::Floor(Floor::new(4)),
                motion: MotionState::Stopped,
                door: DoorState::Open,
                mode: liftassistant_model::OperatingMode::FireService,
                reported_destination: ReportedDestination::None,
            })
        );
    }

    #[test]
    fn source_specific_mode_stays_unresolved() {
        let mut observation = observation();
        observation.position = PositionEvidence::Exact(Position::at_floor(Floor::new(3)));
        observation.motion = MotionEvidence::Known(MotionState::Stopped);
        observation.door = DoorEvidence::Known(DoorState::Open);
        observation.mode = OperatingModeEvidence::SourceSpecific;

        assert_eq!(
            resolve_car_observation(&observation),
            CarStateResolution::Unresolved {
                car: CarId::new("lift-A"),
                reasons: vec![UnresolvedCarStateReason::OperatingModeSourceSpecific],
            }
        );
    }

    #[test]
    fn reported_destination_variants_remain_distinct_after_resolution() {
        let mut observation = observation();
        observation.position = PositionEvidence::Exact(Position::at_floor(Floor::new(3)));
        observation.motion = MotionEvidence::Known(MotionState::Stopped);
        observation.door = DoorEvidence::Known(DoorState::Closed);

        for reported_destination in [
            ReportedDestination::Known(Floor::new(4)),
            ReportedDestination::None,
            ReportedDestination::Unknown,
        ] {
            observation.reported_destination = reported_destination;
            let CarStateResolution::Resolved(state) = resolve_car_observation(&observation) else {
                panic!("complete exact evidence should resolve")
            };
            assert_eq!(
                state.location,
                CarLocation::Continuous(Position::at_floor(Floor::new(3)))
            );
            assert_eq!(state.reported_destination, reported_destination);
        }
    }

    fn update(sequence: u64) -> CarObservationUpdate {
        CarObservationUpdate {
            provenance: ObservationProvenance {
                source: ObservationSourceId::new("fixture/incremental"),
                session: session(),
                sequence: ObservationSequence::new(sequence),
            },
            car: CarId::new("lift-A"),
            position: None,
            motion: None,
            door: None,
            mode: None,
            reported_destination: None,
        }
    }

    #[test]
    fn omitted_fields_preserve_evidence_but_explicit_unknown_replaces_it() {
        let mut assembler = ObservationAssembler::new();
        let mut first = update(1);
        first.position = Some(PositionEvidence::FloorReference(Floor::new(3)));
        first.motion = Some(MotionEvidence::Known(MotionState::Moving(Direction::Up)));
        let first = assembler.apply(first).expect("first update is ordered");
        assert!(first.snapshot().is_none());

        let mut second = update(2);
        second.door = Some(DoorEvidence::MovingUnknownDirection);
        let second = assembler.apply(second).expect("second update is ordered");
        assert_eq!(
            second.position,
            Some(EvidenceSample {
                value: PositionEvidence::FloorReference(Floor::new(3)),
                provenance: ObservationProvenance {
                    source: ObservationSourceId::new("fixture/incremental"),
                    session: session(),
                    sequence: ObservationSequence::new(1),
                },
            })
        );
        assert!(second.snapshot().is_none());

        let mut third = update(3);
        third.position = Some(PositionEvidence::Unknown);
        let third = assembler.apply(third).expect("third update is ordered");
        assert_eq!(
            third.position,
            Some(EvidenceSample {
                value: PositionEvidence::Unknown,
                provenance: ObservationProvenance {
                    source: ObservationSourceId::new("fixture/incremental"),
                    session: session(),
                    sequence: ObservationSequence::new(3),
                },
            })
        );
        assert_eq!(
            third
                .motion
                .as_ref()
                .map(|sample| sample.provenance.sequence),
            Some(ObservationSequence::new(1))
        );

        let mut fourth = update(4);
        fourth.door = Some(DoorEvidence::Known(DoorState::Closed));
        fourth.mode = Some(OperatingModeEvidence::Known(OperatingMode::Normal));
        fourth.reported_destination = Some(ReportedDestination::Unknown);
        let snapshot = assembler
            .apply(fourth)
            .expect("fourth update is ordered")
            .snapshot()
            .expect("every field now has evidence, including explicit unknown");
        assert_eq!(snapshot.position, PositionEvidence::Unknown);
        assert_eq!(
            resolve_car_observation(&snapshot),
            CarStateResolution::Unresolved {
                car: CarId::new("lift-A"),
                reasons: vec![UnresolvedCarStateReason::PositionUnknown],
            }
        );
    }

    #[test]
    fn non_increasing_sequence_is_rejected_without_mutating_evidence() {
        let mut assembler = ObservationAssembler::new();
        let mut first = update(2);
        first.position = Some(PositionEvidence::FloorReference(Floor::new(3)));
        assembler.apply(first).expect("first update is ordered");

        let mut duplicate = update(2);
        duplicate.position = Some(PositionEvidence::Unknown);
        assert_eq!(
            assembler.apply(duplicate),
            Err(ObservationAssemblyError::NonIncreasingSequence {
                source: ObservationSourceId::new("fixture/incremental"),
                session: session(),
                previous: ObservationSequence::new(2),
                received: ObservationSequence::new(2),
            })
        );
        assert_eq!(
            assembler
                .current(
                    &ObservationSourceId::new("fixture/incremental"),
                    &session(),
                    &CarId::new("lift-A"),
                )
                .and_then(|current| current.position.as_ref())
                .map(|sample| sample.value),
            Some(PositionEvidence::FloorReference(Floor::new(3)))
        );
    }

    #[test]
    fn explicit_invalidation_drops_evidence_but_preserves_session_ordering() {
        let mut assembler = ObservationAssembler::new();
        let source = ObservationSourceId::new("fixture/incremental");
        let car = CarId::new("lift-A");
        let mut first = update(100);
        first.position = Some(PositionEvidence::FloorReference(Floor::new(3)));
        assembler.apply(first).expect("first update is ordered");

        assembler.invalidate_source_evidence(&source);
        assert!(assembler.current(&source, &session(), &car).is_none());

        assert_eq!(
            assembler.apply(update(100)),
            Err(ObservationAssemblyError::NonIncreasingSequence {
                source: source.clone(),
                session: session(),
                previous: ObservationSequence::new(100),
                received: ObservationSequence::new(100),
            })
        );
        let resumed = assembler
            .apply(update(101))
            .expect("a newer sample in the active session may rebuild evidence");
        assert_eq!(
            resumed.latest_provenance.sequence,
            ObservationSequence::new(101)
        );
        assert!(resumed.position.is_none());
    }

    #[test]
    fn evidence_is_not_merged_across_sources() {
        let mut assembler = ObservationAssembler::new();
        let mut first = update(1);
        first.position = Some(PositionEvidence::FloorReference(Floor::new(3)));
        assembler.apply(first).expect("first source is ordered");

        let mut second = update(1);
        second.provenance.source = ObservationSourceId::new("fixture/other");
        second.motion = Some(MotionEvidence::Known(MotionState::Stopped));
        assembler
            .apply(second)
            .expect("second source has independent ordering");

        let first = assembler
            .current(
                &ObservationSourceId::new("fixture/incremental"),
                &session(),
                &CarId::new("lift-A"),
            )
            .expect("first source remains available");
        let second = assembler
            .current(
                &ObservationSourceId::new("fixture/other"),
                &session(),
                &CarId::new("lift-A"),
            )
            .expect("second source remains available");
        assert!(first.motion.is_none());
        assert!(second.position.is_none());
        assert!(first.snapshot().is_none() && second.snapshot().is_none());
    }
}
