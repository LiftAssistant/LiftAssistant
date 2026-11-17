use std::{collections::BTreeMap, error::Error, fmt};

use liftassistant_model::{
    BuildingTopology, CarId, CarObservationUpdate, Floor, ObservationProvenance,
    ObservationSequence, ObservationSessionId, ObservationSourceId,
};

use crate::{
    RmfInteractionModeProjection, RmfLiftState, RmfMappingError, RmfSourceContext,
    map_rmf_lift_state_with_projection,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RmfLiftBinding {
    logical_car: CarId,
    floor_labels: BTreeMap<String, Floor>,
}

impl RmfLiftBinding {
    pub fn new(logical_car: CarId, floor_labels: BTreeMap<String, Floor>) -> Self {
        Self {
            logical_car,
            floor_labels,
        }
    }

    pub fn logical_car(&self) -> &CarId {
        &self.logical_car
    }

    pub fn floor_labels(&self) -> &BTreeMap<String, Floor> {
        &self.floor_labels
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappedRmfUpdate {
    pub update: CarObservationUpdate,
    pub topology: BuildingTopology,
    pub source_context: RmfSourceContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RmfAdapterConfigError {
    NoBindings,
    EmptyNativeLiftName,
    DuplicateNativeLiftName(String),
}

impl fmt::Display for RmfAdapterConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoBindings => f.write_str("Open-RMF adapter requires at least one lift binding"),
            Self::EmptyNativeLiftName => {
                f.write_str("Open-RMF adapter native lift_name cannot be empty")
            }
            Self::DuplicateNativeLiftName(name) => {
                write!(f, "duplicate Open-RMF lift binding for {name:?}")
            }
        }
    }
}

impl Error for RmfAdapterConfigError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RmfAdapterError {
    UnconfiguredLift {
        lift_name: String,
        sequence: ObservationSequence,
    },
    Mapping {
        lift_name: String,
        sequence: ObservationSequence,
        error: RmfMappingError,
    },
    SequenceExhausted,
}

impl fmt::Display for RmfAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnconfiguredLift {
                lift_name,
                sequence,
            } => write!(
                f,
                "Open-RMF lift {lift_name:?} has no explicit logical-car binding at source sequence {}",
                sequence.value()
            ),
            Self::Mapping {
                lift_name,
                sequence,
                error,
            } => write!(
                f,
                "Open-RMF lift {lift_name:?} failed mapping at source sequence {}: {error}",
                sequence.value()
            ),
            Self::SequenceExhausted => {
                f.write_str("Open-RMF source observation sequence exhausted")
            }
        }
    }
}

impl Error for RmfAdapterError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Mapping { error, .. } => Some(error),
            Self::UnconfiguredLift { .. } | Self::SequenceExhausted => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RmfObservationAdapter {
    source: ObservationSourceId,
    session: ObservationSessionId,
    interaction_mode_projection: RmfInteractionModeProjection,
    bindings: BTreeMap<String, RmfLiftBinding>,
    next_sequence: u64,
}

impl RmfObservationAdapter {
    pub fn new(
        source: ObservationSourceId,
        session: ObservationSessionId,
        interaction_mode_projection: RmfInteractionModeProjection,
        bindings: impl IntoIterator<Item = (String, RmfLiftBinding)>,
    ) -> Result<Self, RmfAdapterConfigError> {
        let mut configured = BTreeMap::new();
        for (lift_name, binding) in bindings {
            if lift_name.is_empty() {
                return Err(RmfAdapterConfigError::EmptyNativeLiftName);
            }
            if configured.insert(lift_name.clone(), binding).is_some() {
                return Err(RmfAdapterConfigError::DuplicateNativeLiftName(lift_name));
            }
        }
        if configured.is_empty() {
            return Err(RmfAdapterConfigError::NoBindings);
        }

        Ok(Self {
            source,
            session,
            interaction_mode_projection,
            bindings: configured,
            next_sequence: 1,
        })
    }

    pub fn source(&self) -> &ObservationSourceId {
        &self.source
    }

    pub fn session(&self) -> &ObservationSessionId {
        &self.session
    }

    pub fn bindings(&self) -> &BTreeMap<String, RmfLiftBinding> {
        &self.bindings
    }

    pub fn map(&mut self, state: &RmfLiftState) -> Result<MappedRmfUpdate, RmfAdapterError> {
        let sequence = ObservationSequence::new(self.next_sequence);
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(RmfAdapterError::SequenceExhausted)?;

        let Some(binding) = self.bindings.get(&state.lift_name) else {
            return Err(RmfAdapterError::UnconfiguredLift {
                lift_name: state.lift_name.clone(),
                sequence,
            });
        };
        let provenance = ObservationProvenance {
            source: self.source.clone(),
            session: self.session.clone(),
            sequence,
        };
        let mapped = map_rmf_lift_state_with_projection(
            state,
            binding.logical_car.clone(),
            &binding.floor_labels,
            self.interaction_mode_projection,
            provenance,
        )
        .map_err(|error| RmfAdapterError::Mapping {
            lift_name: state.lift_name.clone(),
            sequence,
            error,
        })?;

        Ok(MappedRmfUpdate {
            update: mapped.observation.into(),
            topology: mapped.topology,
            source_context: mapped.source_context,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_engine::{
        Engine, ObservationBindingRole, ObservationIngestError, ObservationIngestResult,
    };
    use liftassistant_runtime::CarStateResolution;

    fn floors() -> BTreeMap<String, Floor> {
        [("L1", 1), ("L2", 2), ("L3", 3)]
            .into_iter()
            .map(|(label, floor)| (label.to_owned(), Floor::new(floor)))
            .collect()
    }

    fn state(lift_name: &str) -> RmfLiftState {
        RmfLiftState {
            lift_name: lift_name.into(),
            available_floors: vec!["L1".into(), "L2".into(), "L3".into()],
            current_floor: "L1".into(),
            destination_floor: "L1".into(),
            door_state: RmfLiftState::DOOR_CLOSED,
            motion_state: RmfLiftState::MOTION_STOPPED,
            available_modes: Vec::new(),
            current_mode: RmfLiftState::MODE_AGV,
            session_id: String::new(),
        }
    }

    fn adapter(session: &str) -> RmfObservationAdapter {
        RmfObservationAdapter::new(
            ObservationSourceId::new("rmf/hotel"),
            ObservationSessionId::new(session),
            RmfInteractionModeProjection::conservative().with_agv_as_normal(),
            [
                (
                    "Lift1".into(),
                    RmfLiftBinding::new(CarId::new("A"), floors()),
                ),
                (
                    "Lift2".into(),
                    RmfLiftBinding::new(CarId::new("B"), floors()),
                ),
            ],
        )
        .expect("valid explicit bindings")
    }

    #[test]
    fn explicit_native_names_map_to_distinct_logical_cars_with_one_source_sequence() {
        let mut adapter = adapter("adapter-start-A");

        let first = adapter.map(&state("Lift1")).expect("Lift1 is bound");
        let second = adapter.map(&state("Lift2")).expect("Lift2 is bound");
        let third = adapter.map(&state("Lift1")).expect("Lift1 remains bound");

        assert_eq!(first.update.car, CarId::new("A"));
        assert_eq!(second.update.car, CarId::new("B"));
        assert_eq!(third.update.car, CarId::new("A"));
        assert_eq!(first.update.provenance.sequence.value(), 1);
        assert_eq!(second.update.provenance.sequence.value(), 2);
        assert_eq!(third.update.provenance.sequence.value(), 3);
        assert_eq!(first.source_context.lift_name, "Lift1");
        assert_eq!(second.source_context.lift_name, "Lift2");
    }

    #[test]
    fn unconfigured_native_name_is_rejected_without_name_based_identity_inference() {
        let mut adapter = adapter("adapter-start-A");
        let error = adapter
            .map(&state("A"))
            .expect_err("matching logical-car text is not an RMF binding");

        assert_eq!(
            error,
            RmfAdapterError::UnconfiguredLift {
                lift_name: "A".into(),
                sequence: ObservationSequence::new(1),
            }
        );
        assert_eq!(
            adapter
                .map(&state("Lift1"))
                .expect("configured lift still maps")
                .update
                .provenance
                .sequence
                .value(),
            2
        );
    }

    #[test]
    fn explicit_adapter_restart_invalidates_engine_state_and_rejects_old_session_data() {
        let source = ObservationSourceId::new("rmf/hotel");
        let mut engine = Engine::new(
            BuildingTopology::new([Floor::new(1), Floor::new(2), Floor::new(3)])
                .expect("valid topology"),
        );
        engine
            .bind_observation(
                source.clone(),
                CarId::new("A"),
                ObservationBindingRole::Operational,
            )
            .expect("bind A");
        engine
            .bind_observation(
                source.clone(),
                CarId::new("B"),
                ObservationBindingRole::Operational,
            )
            .expect("bind B");

        let mut first_adapter = adapter("adapter-start-A");
        engine
            .begin_source_session(source.clone(), first_adapter.session().clone())
            .expect("begin first session");
        let delayed = first_adapter
            .map(&state("Lift1"))
            .expect("map first session")
            .update;
        assert!(matches!(
            engine.ingest_observation_update(delayed.clone()),
            ObservationIngestResult::Resolution(CarStateResolution::Resolved(_))
        ));
        assert!(engine.operational_car_state(&CarId::new("A")).is_some());

        let second_adapter = adapter("adapter-start-B");
        let transition = engine
            .begin_source_session(source, second_adapter.session().clone())
            .expect("begin second session explicitly");
        assert_eq!(transition.invalidated_cars, vec![CarId::new("A")]);
        assert!(engine.car_states().is_empty());
        assert_eq!(
            engine.ingest_observation_update(delayed),
            ObservationIngestResult::Rejected(ObservationIngestError::InactiveSourceSession {
                source: ObservationSourceId::new("rmf/hotel"),
                active: ObservationSessionId::new("adapter-start-B"),
                received: ObservationSessionId::new("adapter-start-A"),
            })
        );
    }

    #[test]
    fn rmf_control_session_never_becomes_the_adapter_source_session() {
        let mut source_state = state("Lift1");
        source_state.session_id = "rmf-control-session".into();
        let mut adapter = adapter("adapter-lifetime");

        let mapped = adapter.map(&source_state).expect("bound state maps");

        assert_eq!(
            mapped.update.provenance.session,
            ObservationSessionId::new("adapter-lifetime")
        );
        assert_eq!(
            mapped.source_context.source_session_id,
            "rmf-control-session"
        );
    }
}
