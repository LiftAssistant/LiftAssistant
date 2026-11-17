//! Bounded executable checks for the two observation families already evidenced by the project.
//!
//! This developer-only module is not an adapter SDK, compatibility promise, or certification
//! surface. It deliberately keeps the live-and-recorded RMF evidence separate from the
//! recorded-only BACnet evidence.

use std::collections::BTreeMap;

use liftassistant_engine::{
    Engine, ObservationBindingRole, ObservationIngestError, ObservationIngestResult,
};
use liftassistant_model::{
    CarId, DoorEvidence, Floor, MotionEvidence, ObservationProvenance, ObservationSequence,
    ObservationSessionId, ObservationSourceId, OperatingModeEvidence, PositionEvidence,
    ReportedDestination,
};
use liftassistant_rmf::{
    RmfAdapterError, RmfInteractionModeProjection, RmfLiftBinding, RmfLiftState,
    RmfObservationAdapter,
};
use liftassistant_runtime::CarStateResolution;
use liftassistant_sim::{
    BacnetDoorStatus, BacnetLiftCarDirection, BacnetLiftCarMode, BacnetMappingError,
    RecordedBacnetLiftProperties, map_bacnet_lift_properties,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ValidationCheck {
    ExplicitNativeToLogicalIdentity,
    EngineBindingRequired,
    ExplicitSourceSessionLifecycle,
    SourceGlobalSessionLocalSequence,
    AmbiguityPreservedAsEvidence,
    RestartInvalidatesOldEvidence,
    CanonicalEnginePathOwnsState,
    ObservationOnlyNoWritePath,
}

const REQUIRED_CHECKS: [ValidationCheck; 8] = [
    ValidationCheck::ExplicitNativeToLogicalIdentity,
    ValidationCheck::EngineBindingRequired,
    ValidationCheck::ExplicitSourceSessionLifecycle,
    ValidationCheck::SourceGlobalSessionLocalSequence,
    ValidationCheck::AmbiguityPreservedAsEvidence,
    ValidationCheck::RestartInvalidatesOldEvidence,
    ValidationCheck::CanonicalEnginePathOwnsState,
    ValidationCheck::ObservationOnlyNoWritePath,
];

impl ValidationCheck {
    const fn label(self) -> &'static str {
        match self {
            Self::ExplicitNativeToLogicalIdentity => "explicit native-to-logical identity",
            Self::EngineBindingRequired => "Engine binding required",
            Self::ExplicitSourceSessionLifecycle => "explicit source-session lifecycle",
            Self::SourceGlobalSessionLocalSequence => "source-global/session-local sequence",
            Self::AmbiguityPreservedAsEvidence => "omission/ambiguity preserved as evidence",
            Self::RestartInvalidatesOldEvidence => "restart invalidates old evidence",
            Self::CanonicalEnginePathOwnsState => "canonical Engine path owns operational state",
            Self::ObservationOnlyNoWritePath => "observation-only path has no write action",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProfileReport {
    family: &'static str,
    evidence_scope: &'static str,
    checks: Vec<ValidationCheck>,
    source_specific_evidence: Vec<&'static str>,
}

pub fn run() -> Result<String, String> {
    let first = validate_profiles()?;
    let second = validate_profiles()?;
    if first != second {
        return Err("adapter validation profiles produced non-deterministic results".into());
    }

    let mut report = String::from(
        "scenario: adapter-validation-profiles\nstatus: PASS (internal engineering checks; not certification)\n",
    );
    for profile in first {
        report.push_str(&format!(
            "\nprofile: {}\nevidence scope: {}\n",
            profile.family, profile.evidence_scope
        ));
        for check in profile.checks {
            report.push_str(&format!("  PASS {}\n", check.label()));
        }
        for evidence in profile.source_specific_evidence {
            report.push_str(&format!("  evidence: {evidence}\n"));
        }
    }
    Ok(report)
}

fn validate_profiles() -> Result<Vec<ProfileReport>, String> {
    Ok(vec![validate_rmf_profile()?, validate_bacnet_profile()?])
}

fn validate_rmf_profile() -> Result<ProfileReport, String> {
    let source = ObservationSourceId::new("validation/open-rmf");
    let session_a = ObservationSessionId::new("rmf-ingestion-A");
    let session_b = ObservationSessionId::new("rmf-ingestion-B");
    let mut adapter_a = rmf_adapter(source.clone(), session_a.clone())?;

    let identity_error = adapter_a
        .map(&rmf_state("logical-A"))
        .expect_err("logical CarId text must not create an RMF native-name binding");
    if identity_error
        != (RmfAdapterError::UnconfiguredLift {
            lift_name: "logical-A".into(),
            sequence: ObservationSequence::new(1),
        })
    {
        return Err(format!(
            "RMF profile did not reject implicit name identity: {identity_error:?}"
        ));
    }

    let mapped_a = adapter_a
        .map(&rmf_state("Lift1"))
        .map_err(|error| format!("RMF Lift1 mapping failed: {error}"))?;
    let mapped_b = adapter_a
        .map(&rmf_state("Lift2"))
        .map_err(|error| format!("RMF Lift2 mapping failed: {error}"))?;
    if mapped_a.update.car != CarId::new("logical-A")
        || mapped_b.update.car != CarId::new("logical-B")
        || mapped_a.source_context.lift_name != "Lift1"
        || mapped_b.source_context.lift_name != "Lift2"
        || mapped_a.source_context.source_session_id != "rmf-control-session-not-source-generation"
        || mapped_a.update.provenance.session != session_a
    {
        return Err("RMF explicit native-to-logical bindings were not preserved".into());
    }
    if [
        mapped_a.update.provenance.sequence.value(),
        mapped_b.update.provenance.sequence.value(),
    ] != [2, 3]
    {
        return Err("RMF adapter sequence was not one source-global stream".into());
    }

    let mut unbound_engine = Engine::new(mapped_a.topology.clone());
    if unbound_engine.ingest_observation_update(mapped_a.update.clone())
        != ObservationIngestResult::Rejected(ObservationIngestError::UnboundObservation {
            source: source.clone(),
            car: CarId::new("logical-A"),
        })
    {
        return Err("RMF canonical update bypassed the Engine binding gate".into());
    }

    let mut engine = Engine::new(mapped_a.topology.clone());
    for car in [CarId::new("logical-A"), CarId::new("logical-B")] {
        engine
            .bind_observation(source.clone(), car, ObservationBindingRole::Operational)
            .map_err(|error| format!("RMF Engine binding failed: {error:?}"))?;
    }
    engine
        .begin_source_session(source.clone(), session_a.clone())
        .map_err(|error| format!("RMF source session A failed: {error:?}"))?;
    expect_resolved(
        engine.ingest_observation_update(mapped_a.update.clone()),
        "RMF logical-A",
    )?;
    expect_resolved(
        engine.ingest_observation_update(mapped_b.update),
        "RMF logical-B",
    )?;
    if engine.car_states().len() != 2
        || engine
            .operational_car_state(&CarId::new("logical-A"))
            .is_none()
        || engine
            .operational_car_state(&CarId::new("logical-B"))
            .is_none()
    {
        return Err("RMF observations did not become unique Engine-owned car states".into());
    }

    let mut ambiguous = rmf_state("Lift1");
    ambiguous.destination_floor.clear();
    ambiguous.door_state = RmfLiftState::DOOR_MOVING;
    ambiguous.current_mode = RmfLiftState::MODE_EMERGENCY;
    let ambiguous = adapter_a
        .map(&ambiguous)
        .map_err(|error| format!("RMF ambiguous evidence mapping failed: {error}"))?;
    if ambiguous.update.reported_destination != Some(ReportedDestination::Unknown)
        || ambiguous.update.door != Some(DoorEvidence::MovingUnknownDirection)
        || ambiguous.update.mode != Some(OperatingModeEvidence::SourceSpecific)
    {
        return Err("RMF ambiguity was collapsed into omitted or guessed evidence".into());
    }

    let mut adapter_b = rmf_adapter(source.clone(), session_b.clone())?;
    let transition = engine
        .begin_source_session(source.clone(), session_b.clone())
        .map_err(|error| format!("RMF source session B failed: {error:?}"))?;
    if transition.invalidated_cars != [CarId::new("logical-A"), CarId::new("logical-B")]
        || !engine.car_states().is_empty()
        || engine.evidence(&source, &CarId::new("logical-A")).is_some()
    {
        return Err("RMF explicit restart retained old operational evidence".into());
    }
    let delayed = adapter_a
        .map(&rmf_state("Lift1"))
        .map_err(|error| format!("RMF delayed old-session mapping failed: {error}"))?;
    if engine.ingest_observation_update(delayed.update)
        != ObservationIngestResult::Rejected(ObservationIngestError::InactiveSourceSession {
            source: source.clone(),
            active: session_b.clone(),
            received: session_a,
        })
    {
        return Err("RMF delayed old-session evidence was not rejected".into());
    }
    let recovered = adapter_b
        .map(&rmf_state("Lift1"))
        .map_err(|error| format!("RMF session-B mapping failed: {error}"))?;
    if recovered.update.provenance.sequence != ObservationSequence::new(1) {
        return Err("RMF new source session did not reset its local sequence".into());
    }
    expect_resolved(
        engine.ingest_observation_update(recovered.update),
        "RMF recovery",
    )?;

    Ok(ProfileReport {
        family: "Open-RMF observation adapter",
        evidence_scope: "official live Hotel proof plus recorded regressions",
        checks: REQUIRED_CHECKS.to_vec(),
        source_specific_evidence: vec![
            "lift_name remains source context and never becomes CarId",
            "RMF session_id remains source context and never becomes SourceSession",
            "door-moving, empty destination, and source-specific mode remain ambiguous evidence",
        ],
    })
}

fn validate_bacnet_profile() -> Result<ProfileReport, String> {
    let source = ObservationSourceId::new("validation/bacnet-recorded");
    let session_a = ObservationSessionId::new("bacnet-ingestion-A");
    let session_b = ObservationSessionId::new("bacnet-ingestion-B");
    let floors = bacnet_floors();
    let native_a = bacnet_properties("BACnet Lift Object logical-B", 1);
    let native_b = bacnet_properties("BACnet Lift Object logical-A", 2);
    let mapped_a = map_bacnet_lift_properties(
        &native_a,
        CarId::new("logical-A"),
        &floors,
        provenance(&source, &session_a, 1),
    )
    .map_err(|error| format!("BACnet logical-A mapping failed: {error}"))?;
    let mapped_b = map_bacnet_lift_properties(
        &native_b,
        CarId::new("logical-B"),
        &floors,
        provenance(&source, &session_a, 2),
    )
    .map_err(|error| format!("BACnet logical-B mapping failed: {error}"))?;
    if mapped_a.observation.car != CarId::new("logical-A")
        || mapped_b.observation.car != CarId::new("logical-B")
        || mapped_a.source_context.object_name == mapped_a.observation.car.as_str()
        || mapped_b.source_context.object_name == mapped_b.observation.car.as_str()
    {
        return Err("BACnet explicit logical CarId input was replaced by native identity".into());
    }
    if [
        mapped_a.observation.provenance.sequence.value(),
        mapped_b.observation.provenance.sequence.value(),
    ] != [1, 2]
    {
        return Err("BACnet recorded boundary did not preserve source-global order".into());
    }
    let mut unmapped_floor = native_a.clone();
    unmapped_floor.car_position = 99;
    if map_bacnet_lift_properties(
        &unmapped_floor,
        CarId::new("logical-A"),
        &floors,
        provenance(&source, &session_a, 3),
    ) != Err(BacnetMappingError::UnknownUniversalFloor(99))
    {
        return Err("BACnet unconfigured universal floor was not rejected".into());
    }

    let mut unbound_engine = Engine::new(mapped_a.topology.clone());
    if unbound_engine.ingest_observation_update((&mapped_a.observation).into())
        != ObservationIngestResult::Rejected(ObservationIngestError::UnboundObservation {
            source: source.clone(),
            car: CarId::new("logical-A"),
        })
    {
        return Err("BACnet canonical observation bypassed the Engine binding gate".into());
    }

    let mut engine = Engine::new(mapped_a.topology.clone());
    for car in [CarId::new("logical-A"), CarId::new("logical-B")] {
        engine
            .bind_observation(source.clone(), car, ObservationBindingRole::Operational)
            .map_err(|error| format!("BACnet Engine binding failed: {error:?}"))?;
    }
    engine
        .begin_source_session(source.clone(), session_a.clone())
        .map_err(|error| format!("BACnet source session A failed: {error:?}"))?;
    expect_resolved(
        engine.ingest_observation_update((&mapped_a.observation).into()),
        "BACnet logical-A",
    )?;
    expect_resolved(
        engine.ingest_observation_update((&mapped_b.observation).into()),
        "BACnet logical-B",
    )?;
    if engine.car_states().len() != 2 {
        return Err("BACnet observations did not become unique Engine-owned car states".into());
    }

    let mut ambiguous_native = native_a.clone();
    ambiguous_native.car_door_status = vec![BacnetDoorStatus::Opened, BacnetDoorStatus::Closed];
    ambiguous_native.next_stopping_floor = None;
    let ambiguous = map_bacnet_lift_properties(
        &ambiguous_native,
        CarId::new("logical-A"),
        &floors,
        provenance(&source, &session_a, 3),
    )
    .map_err(|error| format!("BACnet ambiguous mapping failed: {error}"))?;
    let ambiguous_update: liftassistant_model::CarObservationUpdate =
        (&ambiguous.observation).into();
    if ambiguous_update.door != Some(DoorEvidence::Unknown)
        || ambiguous_update.reported_destination != Some(ReportedDestination::Unknown)
        || ambiguous.source_context.door_statuses
            != [BacnetDoorStatus::Opened, BacnetDoorStatus::Closed]
    {
        return Err("BACnet multi-door/destination ambiguity was omitted or guessed".into());
    }
    match engine.ingest_observation_update(ambiguous_update) {
        ObservationIngestResult::Resolution(CarStateResolution::Unresolved { .. }) => {}
        other => {
            return Err(format!(
                "BACnet ambiguous evidence did not revoke resolution: {other:?}"
            ));
        }
    }

    let mut decoupled_native = native_a.clone();
    decoupled_native.out_of_service = true;
    decoupled_native.car_mode = Some(BacnetLiftCarMode::OutOfService);
    let decoupled = map_bacnet_lift_properties(
        &decoupled_native,
        CarId::new("logical-A"),
        &floors,
        provenance(&source, &session_a, 4),
    )
    .map_err(|error| format!("BACnet decoupled-object mapping failed: {error}"))?;
    if decoupled.observation.position != PositionEvidence::Unknown
        || decoupled.observation.motion != MotionEvidence::Unknown
        || decoupled.observation.door != DoorEvidence::Unknown
        || decoupled.observation.mode != OperatingModeEvidence::Unknown
        || !decoupled.source_context.object_out_of_service
    {
        return Err("BACnet object decoupling was misreported as lift-car state".into());
    }
    match engine.ingest_observation_update((&decoupled.observation).into()) {
        ObservationIngestResult::Resolution(CarStateResolution::Unresolved { .. }) => {}
        other => {
            return Err(format!(
                "BACnet decoupled evidence did not stay unresolved: {other:?}"
            ));
        }
    }

    let transition = engine
        .begin_source_session(source.clone(), session_b.clone())
        .map_err(|error| format!("BACnet source session B failed: {error:?}"))?;
    if transition.invalidated_cars != [CarId::new("logical-B")]
        || !engine.car_states().is_empty()
        || engine.evidence(&source, &CarId::new("logical-A")).is_some()
    {
        return Err("BACnet explicit restart retained old evidence/state".into());
    }
    let delayed = map_bacnet_lift_properties(
        &native_a,
        CarId::new("logical-A"),
        &floors,
        provenance(&source, &session_a, 5),
    )
    .map_err(|error| format!("BACnet delayed mapping failed: {error}"))?;
    if engine.ingest_observation_update((&delayed.observation).into())
        != ObservationIngestResult::Rejected(ObservationIngestError::InactiveSourceSession {
            source: source.clone(),
            active: session_b.clone(),
            received: session_a,
        })
    {
        return Err("BACnet delayed old-session evidence was not rejected".into());
    }
    let recovered = map_bacnet_lift_properties(
        &native_a,
        CarId::new("logical-A"),
        &floors,
        provenance(&source, &session_b, 1),
    )
    .map_err(|error| format!("BACnet session-B mapping failed: {error}"))?;
    expect_resolved(
        engine.ingest_observation_update((&recovered.observation).into()),
        "BACnet recovery",
    )?;

    Ok(ProfileReport {
        family: "BACnet Lift recorded mapper",
        evidence_scope: "recorded mapping only; no live production endpoint supplied",
        checks: REQUIRED_CHECKS.to_vec(),
        source_specific_evidence: vec![
            "Object_Name remains source context and explicit CarId remains logical identity",
            "universal floor numbers require configured mapping",
            "multi-door disagreement and decoupled-object values remain unresolved evidence",
        ],
    })
}

fn expect_resolved(result: ObservationIngestResult, context: &str) -> Result<(), String> {
    match result {
        ObservationIngestResult::Resolution(CarStateResolution::Resolved(_)) => Ok(()),
        other => Err(format!(
            "{context} did not resolve through Engine: {other:?}"
        )),
    }
}

fn rmf_adapter(
    source: ObservationSourceId,
    session: ObservationSessionId,
) -> Result<RmfObservationAdapter, String> {
    RmfObservationAdapter::new(
        source,
        session,
        RmfInteractionModeProjection::conservative().with_agv_as_normal(),
        [
            (
                "Lift1".into(),
                RmfLiftBinding::new(CarId::new("logical-A"), rmf_floors()),
            ),
            (
                "Lift2".into(),
                RmfLiftBinding::new(CarId::new("logical-B"), rmf_floors()),
            ),
        ],
    )
    .map_err(|error| format!("RMF validation adapter configuration failed: {error}"))
}

fn rmf_floors() -> BTreeMap<String, Floor> {
    [("L1", 1), ("L2", 2), ("L3", 3)]
        .into_iter()
        .map(|(label, floor)| (label.to_owned(), Floor::new(floor)))
        .collect()
}

fn rmf_state(lift_name: &str) -> RmfLiftState {
    RmfLiftState {
        lift_name: lift_name.into(),
        available_floors: vec!["L1".into(), "L2".into(), "L3".into()],
        current_floor: "L1".into(),
        destination_floor: "L1".into(),
        door_state: RmfLiftState::DOOR_CLOSED,
        motion_state: RmfLiftState::MOTION_STOPPED,
        available_modes: vec![RmfLiftState::MODE_AGV],
        current_mode: RmfLiftState::MODE_AGV,
        session_id: "rmf-control-session-not-source-generation".into(),
    }
}

fn bacnet_floors() -> BTreeMap<u8, Floor> {
    [(1, 1), (2, 2), (3, 3)]
        .into_iter()
        .map(|(universal, floor)| (universal, Floor::new(floor)))
        .collect()
}

fn bacnet_properties(object_name: &str, floor: u8) -> RecordedBacnetLiftProperties {
    RecordedBacnetLiftProperties {
        object_name: object_name.into(),
        car_position: floor,
        car_moving_direction: BacnetLiftCarDirection::Stopped,
        car_door_status: vec![BacnetDoorStatus::Closed],
        car_mode: Some(BacnetLiftCarMode::Normal),
        next_stopping_floor: Some(floor),
        out_of_service: false,
    }
}

fn provenance(
    source: &ObservationSourceId,
    session: &ObservationSessionId,
    sequence: u64,
) -> ObservationProvenance {
    ObservationProvenance {
        source: source.clone(),
        session: session.clone(),
        sequence: ObservationSequence::new(sequence),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_evidenced_families_pass_the_same_bounded_checks() {
        let reports = validate_profiles().expect("validation profiles pass");

        assert_eq!(reports.len(), 2);
        assert!(
            reports
                .iter()
                .all(|report| report.checks == REQUIRED_CHECKS)
        );
        assert_eq!(
            reports[1].evidence_scope,
            "recorded mapping only; no live production endpoint supplied"
        );
    }

    #[test]
    fn executable_report_disclaims_certification_and_live_bacnet() {
        let report = run().expect("validation report renders");

        assert!(report.contains("not certification"));
        assert!(report.contains("official live Hotel proof plus recorded regressions"));
        assert!(report.contains("no live production endpoint supplied"));
    }
}
