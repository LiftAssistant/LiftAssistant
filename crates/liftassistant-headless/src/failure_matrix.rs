use super::*;

use serde_json::Value;

pub(super) fn source(index: usize) -> ObservationSourceId {
    ObservationSourceId::new(format!("matrix-source-{index}"))
}

fn session(generation: u64) -> ObservationSessionId {
    ObservationSessionId::new(format!("matrix-session-{generation}"))
}

fn car(index: usize) -> CarId {
    CarId::new(format!("car-{index:03}"))
}

pub(super) fn profile(cars: usize, sources: usize) -> ReadOnlyHost {
    let mut host = ReadOnlyHost::new(BuildingTopology::reference_eight_floor());
    let operational_sources = sources / 2;
    for index in 0..cars {
        let group = index % operational_sources;
        host.bind_observation(
            source(group),
            car(index),
            ObservationBindingRole::Operational,
        )
        .unwrap();
        host.bind_observation(
            source(group + operational_sources),
            car(index),
            ObservationBindingRole::Auxiliary,
        )
        .unwrap();
    }
    for index in 0..sources {
        host.begin_source_session(source(index), session(0))
            .unwrap();
        host.note_transport_subscriber_created(&source(index));
        host.note_transport_discovery(&source(index));
    }
    for index in 0..cars {
        let group = index % operational_sources;
        let sequence = (index / operational_sources + 1) as u64;
        for (source_index, floor) in [(group, 1), (group + operational_sources, 8)] {
            host.ingest_observation(update(source_index, 0, sequence, index, floor));
        }
    }
    assert_eq!(host.readiness(), Readiness::ReadOnly);
    host
}

pub(super) fn update(
    source_index: usize,
    generation: u64,
    sequence: u64,
    car_index: usize,
    floor: i16,
) -> CarObservationUpdate {
    CarObservationUpdate::from_state(
        ObservationProvenance {
            source: source(source_index),
            session: session(generation),
            sequence: ObservationSequence::new(sequence),
        },
        &liftassistant_model::CarState::new(car(car_index).as_str(), Floor::new(floor)),
    )
}

fn refill(host: &mut ReadOnlyHost, cars: usize, stride: usize, generation: u64, offset: u64) {
    for (ordinal, index) in (0..cars).step_by(stride).enumerate() {
        host.ingest_observation(update(0, generation, offset + ordinal as u64 + 1, index, 1));
    }
}

fn assert_ready(host: &ReadOnlyHost, cars: usize) {
    assert_eq!(host.readiness(), Readiness::ReadOnly);
    let states = host.engine.car_states();
    assert_eq!(states.len(), cars);
    assert!(
        states
            .iter()
            .all(|state| state.location == CarLocation::Floor(Floor::new(1))),
        "auxiliary floor 8 must never become operational truth"
    );
}

#[test]
fn repeated_source_and_data_plane_churn_at_three_scales() {
    for (cars, sources) in [(4, 2), (32, 8), (128, 16)] {
        let mut host = profile(cars, sources);
        let stride = sources / 2;
        let stream_len = (cars / stride) as u64;
        for generation in 0..32 {
            assert_ready(&host, cars);
            let auxiliary = source(stride);
            host.mark_source_payload_stale(auxiliary.clone(), session(generation))
                .unwrap();
            host.record_transport_failure(
                auxiliary.clone(),
                session(generation),
                "fixture transport failure".into(),
            );
            host.begin_source_session(auxiliary, session(generation + 1))
                .unwrap();
            host.ingest_observation(update(stride, generation + 1, 1, 0, 8));
            assert_ready(&host, cars);
            assert_eq!(
                host.engine
                    .source_state(&source(stride), &car(0))
                    .unwrap()
                    .location,
                CarLocation::Floor(Floor::new(8))
            );

            host.mark_source_payload_stale(source(0), session(generation))
                .unwrap();
            assert_eq!(host.readiness(), Readiness::NotReady);
            let health = host.source_health(&source(0)).unwrap();
            assert_eq!(
                health.transport_discovery,
                TransportDiscoveryState::DiscoveryObserved
            );
            assert_eq!(
                health.session,
                SourceSessionActivity::Active(session(generation))
            );
            assert_eq!(health.payload, PayloadActivity::Stale);
            assert!(
                health
                    .observations
                    .values()
                    .all(|state| *state == ObservationAvailability::Revoked)
            );
            for index in (0..cars).step_by(stride) {
                assert!(host.engine.operational_car_state(&car(index)).is_none());
                assert!(host.engine.evidence(&source(0), &car(index)).is_none());
            }

            // A sequence higher than car 0's last accepted value still falls below the source watermark.
            host.ingest_observation(update(0, generation, stream_len - 1, 0, 6));
            assert_eq!(host.readiness(), Readiness::NotReady);
            assert!(host.engine.evidence(&source(0), &car(0)).is_none());
            let next_sequence = stream_len + 1 + u64::from(generation != 0);
            let mut partial = update(0, generation, next_sequence, 0, 1);
            partial.door = None;
            host.ingest_observation(partial);
            assert!(host.engine.operational_car_state(&car(0)).is_none());
            assert!(
                host.engine
                    .evidence(&source(0), &car(0))
                    .unwrap()
                    .door
                    .is_none()
            );
            refill(&mut host, cars, stride, generation, next_sequence);
            assert_ready(&host, cars);
            assert_eq!(
                host.source_health(&source(0)).unwrap().payload,
                PayloadActivity::Active
            );

            host.begin_source_session(source(0), session(generation + 1))
                .unwrap();
            assert_eq!(host.readiness(), Readiness::NotReady);
            assert!(host.engine.evidence(&source(0), &car(0)).is_none());
            host.ingest_observation(update(0, generation, 999, 0, 6));
            assert!(host.engine.evidence(&source(0), &car(0)).is_none());
            refill(&mut host, cars, stride, generation + 1, 0);
            assert_ready(&host, cars);
            host.ingest_observation(update(0, generation, 1000, 0, 6));
            assert!(matches!(
                host.lifecycle(),
                HostLifecycle::Degraded(DegradedReason::ObservationRejected(
                    ObservationIngestError::InactiveSourceSession { .. }
                ))
            ));
            assert_eq!(
                host.engine.operational_car_state(&car(0)).unwrap().location,
                CarLocation::Floor(Floor::new(1))
            );
            host.ingest_observation(update(0, generation + 1, stream_len + 1, 0, 1));
            assert_ready(&host, cars);
        }
        host.record_transport_failure(source(0), session(32), "fixture transport failure".into());
        assert!(host.diagnostics().entries().any(|entry| {
            entry.id == HostDiagnosticId::TransportFailed
                && entry.layer == HostDiagnosticLayer::Transport
        }));
        host.mark_source_payload_stale(source(0), session(32))
            .unwrap();
        assert_eq!(host.readiness(), Readiness::NotReady);
        assert!(
            host.engine
                .car_states()
                .iter()
                .all(|state| state.door != DoorState::Fault)
        );
        assert_eq!(
            host.diagnostics().entries().count(),
            HOST_DIAGNOSTIC_CAPACITY
        );
        assert_eq!(host.engine.diagnostics().entries().count(), 128);
        assert_eq!(host.engine.diagnostics().incidents().count(), 8);
        let capsule = host.capture_incident(CapsuleTrigger::source_payload_stale(
            &source(0),
            &session(32),
        ));
        capsule.replay().expect("compacted scale capsule replays");
        let model = serde_json::to_value(host.read_model()).unwrap();
        assert_eq!(model["sources"].as_array().unwrap().len(), sources);
        assert_eq!(
            model["operational_cars"].as_array().unwrap().len(),
            cars - stream_len as usize
        );
        assert_eq!(
            model["recent_diagnostics"].as_array().unwrap().len(),
            HOST_DIAGNOSTIC_CAPACITY
        );
        assert_eq!(model["readiness"], Value::from("not_ready"));
        println!(
            "matrix cars={cars} sources={sources} rounds=32 capsule_bytes={}",
            capsule.to_json().unwrap().len()
        );
    }
}
