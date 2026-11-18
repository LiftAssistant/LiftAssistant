//! Production RMF receive supervision at the Host boundary.

use liftassistant_rmf::RmfAdapterError;

use super::{
    BTreeMap, BuildingTopology, CarId, Instant, PayloadActivity, ReadOnlyHost, RmfFreshnessPolicy,
    RmfLiftState, RmfObservationAdapter,
};

pub(super) struct RmfCarFreshness {
    policy: RmfFreshnessPolicy,
    last_payloads: BTreeMap<CarId, Instant>,
}

impl RmfCarFreshness {
    pub(super) fn new(policy: RmfFreshnessPolicy) -> Self {
        Self {
            policy,
            last_payloads: BTreeMap::new(),
        }
    }

    /// Runs on every receive and on idle ticks. Only bound cars which have produced
    /// payloads have a timer. Each expired entry is removed until that car sends again.
    pub(super) fn supervise(
        &mut self,
        host: &mut ReadOnlyHost,
        adapter: &mut RmfObservationAdapter,
        now: Instant,
        state: Option<&RmfLiftState>,
    ) -> Result<(), String> {
        let mut revocation_error = None;
        self.last_payloads.retain(|car, last| {
            if now.duration_since(*last) < self.policy.max_payload_silence() {
                return true;
            }
            if let Err(error) =
                host.revoke_car_observations(adapter.source(), adapter.session(), car)
            {
                revocation_error = Some(error);
            }
            false
        });
        if let Some(error) = revocation_error {
            return Err(format!("failed to revoke stale RMF car: {error:?}"));
        }
        let Some(state) = state else {
            return Ok(());
        };

        host.note_transport_discovery(adapter.source());
        let health = host
            .source_health
            .get_mut(adapter.source())
            .ok_or("RMF source lacks bound health state")?;
        if health.payload != PayloadActivity::Active {
            health.payload = PayloadActivity::Active;
            host.bump_read_model_revision();
        }
        let car = adapter
            .bindings()
            .get(&state.lift_name)
            .map(|binding| binding.logical_car().clone());
        if let Some(car) = &car {
            self.last_payloads.insert(car.clone(), now);
        }

        let mapping_error = match adapter.map(state) {
            Ok(mapped) => match validate_available_floors(host.engine.topology(), &mapped.topology)
            {
                Ok(()) => {
                    host.ingest_observation(mapped.update);
                    return Ok(());
                }
                Err(error) => error,
            },
            Err(RmfAdapterError::SequenceExhausted) => {
                return Err(RmfAdapterError::SequenceExhausted.to_string());
            }
            Err(error) => error.to_string(),
        };
        host.record_mapping_rejection(
            adapter.source().clone(),
            adapter.session().clone(),
            state.lift_name.clone(),
            mapping_error,
        );
        if let Some(car) = car {
            host.revoke_car_observations(adapter.source(), adapter.session(), &car)
                .map_err(|error| format!("failed to revoke rejected RMF car: {error:?}"))?;
        }
        Ok(())
    }
}

pub(super) fn validate_available_floors(
    building: &BuildingTopology,
    advertised: &BuildingTopology,
) -> Result<(), String> {
    if let Some(floor) = advertised
        .served_floors()
        .iter()
        .find(|floor| !building.contains(**floor))
    {
        return Err(format!(
            "RMF lift advertises floor {floor:?} outside the configured building topology"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        Floor, HostDiagnosticId, ObservationAvailability, ObservationBindingRole,
        ObservationSessionId, ObservationSourceId, Readiness, RmfInteractionModeProjection,
        RmfLiftBinding,
    };
    use std::time::Duration;

    use super::*;

    fn fixture(
        role_a: ObservationBindingRole,
    ) -> (ReadOnlyHost, RmfObservationAdapter, RmfCarFreshness) {
        let mut host = ReadOnlyHost::new(BuildingTopology::new([1, 2, 3].map(Floor::new)).unwrap());
        let source = ObservationSourceId::new("test/multiplexed");
        let session = ObservationSessionId::new("fixture-incarnation");
        // L4 is recognized by the mapper but deliberately outside this Host's universe.
        let labels: BTreeMap<_, _> = (1..=4)
            .map(|floor| (format!("L{floor}"), Floor::new(floor)))
            .collect();
        let mut bindings = Vec::new();
        for (native, car, role) in [
            ("Lift1", "A", role_a),
            ("Lift2", "B", ObservationBindingRole::Operational),
        ] {
            let car = CarId::new(car);
            host.bind_observation(source.clone(), car.clone(), role)
                .unwrap();
            bindings.push((native.to_string(), RmfLiftBinding::new(car, labels.clone())));
        }
        host.begin_source_session(source.clone(), session.clone())
            .unwrap();
        host.note_transport_subscriber_created(&source);
        let adapter = RmfObservationAdapter::new(
            source,
            session,
            RmfInteractionModeProjection::conservative().with_agv_as_normal(),
            bindings,
        )
        .unwrap();
        let monitor =
            RmfCarFreshness::new(RmfFreshnessPolicy::new(Duration::from_secs(1)).unwrap());
        (host, adapter, monitor)
    }

    fn lift(name: &str, floors: &[&str]) -> RmfLiftState {
        RmfLiftState {
            lift_name: name.into(),
            available_floors: floors.iter().map(|label| (*label).to_string()).collect(),
            current_floor: "L1".into(),
            destination_floor: "L1".into(),
            motion_state: RmfLiftState::MOTION_STOPPED,
            door_state: RmfLiftState::DOOR_CLOSED,
            available_modes: Vec::new(),
            current_mode: RmfLiftState::MODE_AGV,
            session_id: "changeable-rmf-control-session".into(),
        }
    }

    #[test]
    fn rmf_per_car_expiry_and_recovery_with_an_active_peer() {
        for role in [
            ObservationBindingRole::Operational,
            ObservationBindingRole::Auxiliary,
        ] {
            let (mut host, mut adapter, mut monitor) = fixture(role);
            let source = adapter.source().clone();
            let session = adapter.session().clone();
            let (a, b) = (CarId::new("A"), CarId::new("B"));
            let (lift_a, lift_b) = (lift("Lift1", &["L1", "L2"]), lift("Lift2", &["L1", "L3"]));
            let base = Instant::now();
            let mut step = |host: &mut ReadOnlyHost, ms, payload| {
                monitor
                    .supervise(
                        host,
                        &mut adapter,
                        base + Duration::from_millis(ms),
                        payload,
                    )
                    .unwrap();
            };
            step(&mut host, 0, Some(&lift_b));
            step(&mut host, 600, Some(&lift_b));
            step(&mut host, 1100, Some(&lift_b));
            assert_eq!(
                host.source_health(&source).unwrap().observations[&a],
                ObservationAvailability::Awaiting
            );
            step(&mut host, 1100, Some(&lift_a));
            assert_eq!(host.readiness(), Readiness::ReadOnly);
            step(&mut host, 1700, Some(&lift_b));
            step(&mut host, 2100, None);
            assert_eq!(
                host.source_health(&source).unwrap().observations[&a],
                ObservationAvailability::Revoked
            );
            assert_eq!(
                host.source_health(&source).unwrap().payload,
                PayloadActivity::Active
            );
            assert_eq!(host.engine.active_source_session(&source), Some(&session));
            assert!(host.engine.evidence(&source, &a).is_none());
            assert!(host.engine.source_state(&source, &a).is_none());
            assert!(host.engine.operational_car_state(&b).is_some());
            assert_eq!(
                host.readiness(),
                if role == ObservationBindingRole::Operational {
                    Readiness::NotReady
                } else {
                    Readiness::ReadOnly
                }
            );
            step(&mut host, 2150, Some(&lift_b));
            step(&mut host, 2200, Some(&lift_a));
            assert_eq!(host.readiness(), Readiness::ReadOnly);
            assert_eq!(
                host.engine
                    .evidence(&source, &a)
                    .unwrap()
                    .latest_provenance
                    .sequence
                    .value(),
                7
            );
        }
    }

    #[test]
    fn rmf_mapping_failures_and_floor_subsets_are_scoped_to_the_entity() {
        for role in [
            ObservationBindingRole::Operational,
            ObservationBindingRole::Auxiliary,
        ] {
            let (mut host, mut adapter, mut monitor) = fixture(role);
            let now = Instant::now();
            let (a, b) = (CarId::new("A"), CarId::new("B"));
            let source = adapter.source().clone();
            let good_a = lift("Lift1", &["L1", "L2", "L3"]);
            let good_b = lift("Lift2", &["L1", "L2"]);
            for state in [&good_a, &good_b, &lift("Lift3", &["L1"])] {
                monitor
                    .supervise(&mut host, &mut adapter, now, Some(state))
                    .unwrap();
            }
            assert_eq!(host.readiness(), Readiness::ReadOnly);
            assert!(host.engine.source_state(&source, &a).is_some());
            assert_eq!(monitor.last_payloads.len(), 2);
            for floor in ["L4", "UNMAPPED"] {
                monitor
                    .supervise(
                        &mut host,
                        &mut adapter,
                        now,
                        Some(&lift("Lift1", &["L1", floor])),
                    )
                    .unwrap();
                assert!(host.engine.evidence(&source, &a).is_none());
                assert!(host.engine.operational_car_state(&b).is_some());
                assert_eq!(
                    host.readiness(),
                    if role == ObservationBindingRole::Operational {
                        Readiness::NotReady
                    } else {
                        Readiness::ReadOnly
                    }
                );
                monitor
                    .supervise(&mut host, &mut adapter, now, Some(&good_b))
                    .unwrap();
                monitor
                    .supervise(&mut host, &mut adapter, now, Some(&good_a))
                    .unwrap();
                assert_eq!(host.readiness(), Readiness::ReadOnly);
            }
            let rejected: Vec<_> = host
                .diagnostics()
                .entries()
                .filter(|entry| entry.id == HostDiagnosticId::MappingRejected)
                .collect();
            assert_eq!(rejected.len(), 3);
            assert_eq!(rejected[0].context.native_entity.as_deref(), Some("Lift3"));
            assert_eq!(
                host.engine
                    .evidence(&source, &a)
                    .unwrap()
                    .latest_provenance
                    .sequence
                    .value(),
                9
            );
        }
    }
}
