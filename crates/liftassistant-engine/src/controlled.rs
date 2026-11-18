use std::collections::{BTreeMap, VecDeque};

use liftassistant_model::{
    CallId, CallState, CarId, Intent, IntentId, MotionState, OperatingMode, ReportedDestination,
};

use crate::Engine;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrincipalId(String);

impl PrincipalId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Identifies the configured lower controller, independent of the requester and any
/// session or holder token maintained by that controller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControllerId(String);

impl ControllerId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthorityGrantId(u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityGrant {
    pub id: AuthorityGrantId,
    pub principal: PrincipalId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityError {
    ControllerUnbound,
    ControllerAlreadyBound,
    ScopeOccupied,
    AlreadyGranted,
    Fenced,
    IdentityExhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentRequest {
    pub id: IntentId,
    pub intent: Intent,
    pub call: Option<CallId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntentRecord {
    pub request: IntentRequest,
    pub controller: ControllerId,
    pub authority: AuthorityGrant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteBlocker {
    AuthorityFenced,
    ControllerUnbound,
    WrongController,
    ControllerUnavailable,
    ObservationUnavailable,
    Mode(OperatingMode),
    Busy,
    CapabilitiesUnavailable,
    OperationUnsupported,
    UnsupportedFloor,
    InvalidCall,
    ActiveIntent(IntentId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Admission {
    Admitted,
    Retry,
    IdentityConflict,
    RetiredIdentity,
    Blocked(WriteBlocker),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ControlledIntents {
    controllers: BTreeMap<CarId, ControllerId>,
    grants: BTreeMap<CarId, AuthorityGrant>,
    last_grant: u64,
    active: BTreeMap<CarId, IntentRecord>,
    recent: VecDeque<IntentRecord>,
    last_admitted: Option<IntentId>,
}

impl ControlledIntents {
    fn record(&self, id: IntentId) -> Option<&IntentRecord> {
        self.active
            .values()
            .chain(self.recent.iter())
            .find(|record| record.request.id == id)
    }

    pub(crate) fn has_controller(&self, car: &CarId) -> bool {
        self.controllers.contains_key(car)
    }

    pub(crate) fn is_configured(&self) -> bool {
        !self.controllers.is_empty()
    }

    fn check_grant(&self, car: &CarId, grant: &AuthorityGrant) -> Result<(), WriteBlocker> {
        if self.grants.get(car) != Some(grant) {
            return Err(WriteBlocker::AuthorityFenced);
        }
        Ok(())
    }
}

impl Engine {
    pub fn bind_controlled_car(
        &mut self,
        car: CarId,
        controller: ControllerId,
    ) -> Result<(), AuthorityError> {
        if !self.is_controlled() && !self.calls.occupied_cars().is_empty() {
            return Err(AuthorityError::ScopeOccupied);
        }
        if self.controlled.has_controller(&car) {
            return Err(AuthorityError::ControllerAlreadyBound);
        }
        self.controlled.controllers.insert(car, controller);
        Ok(())
    }

    pub(crate) fn is_controlled(&self) -> bool {
        self.controlled.is_configured()
    }

    pub fn grant_authority(
        &mut self,
        car: &CarId,
        principal: PrincipalId,
    ) -> Result<AuthorityGrant, AuthorityError> {
        if !self.controlled.has_controller(car) {
            return Err(AuthorityError::ControllerUnbound);
        }
        if self.controlled.grants.contains_key(car) {
            return Err(AuthorityError::AlreadyGranted);
        }
        let id = self
            .controlled
            .last_grant
            .checked_add(1)
            .ok_or(AuthorityError::IdentityExhausted)?;
        self.controlled.last_grant = id;
        let grant = AuthorityGrant {
            id: AuthorityGrantId(id),
            principal,
        };
        self.controlled.grants.insert(car.clone(), grant.clone());
        Ok(grant)
    }

    pub fn revoke_authority(
        &mut self,
        car: &CarId,
        grant: &AuthorityGrant,
    ) -> Result<(), AuthorityError> {
        self.controlled
            .check_grant(car, grant)
            .map_err(|_| AuthorityError::Fenced)?;
        self.controlled.grants.remove(car);
        Ok(())
    }

    pub fn intent_record(&self, id: IntentId) -> Option<&IntentRecord> {
        self.controlled.record(id)
    }

    /// Per-request write readiness. The adapter supplies its current availability.
    pub fn write_readiness(
        &self,
        request: &IntentRequest,
        grant: &AuthorityGrant,
        controller: &ControllerId,
        controller_available: bool,
    ) -> Result<(), WriteBlocker> {
        let car = request.intent.car();
        let expected = self
            .controlled
            .controllers
            .get(car)
            .ok_or(WriteBlocker::ControllerUnbound)?;
        self.controlled.check_grant(car, grant)?;
        if expected != controller {
            return Err(WriteBlocker::WrongController);
        }
        if !controller_available {
            return Err(WriteBlocker::ControllerUnavailable);
        }
        let state = self
            .operational_car_state(car)
            .ok_or(WriteBlocker::ObservationUnavailable)?;
        if state.mode != OperatingMode::Normal {
            return Err(WriteBlocker::Mode(state.mode));
        }
        let caps = self
            .execution_capabilities(car)
            .ok_or(WriteBlocker::CapabilitiesUnavailable)?;
        let supported = match &request.intent {
            Intent::ServeFloor { .. } => caps.serve_floor,
            Intent::ParkAt { .. } => caps.park,
        };
        if !supported {
            return Err(WriteBlocker::OperationUnsupported);
        }
        let floor = request.intent.floor();
        if !self.topology.contains(floor) || !caps.supports_floor(floor) {
            return Err(WriteBlocker::UnsupportedFloor);
        }
        if let Some(active) = self.controlled.active.get(car) {
            return Err(WriteBlocker::ActiveIntent(active.request.id));
        }
        if let Some(id) = request.call {
            let valid = matches!(&request.intent, Intent::ServeFloor { .. })
                && self.calls.get(id).is_some_and(|call| {
                    call.state == CallState::Pending && call.call.floor == floor
                })
                && !self
                    .controlled
                    .active
                    .values()
                    .any(|record| record.request.call == Some(id));
            if !valid {
                return Err(WriteBlocker::InvalidCall);
            }
        }
        if self.calls.occupied_cars().contains(car)
            || state.motion != MotionState::Stopped
            || match state.reported_destination {
                ReportedDestination::None => false,
                ReportedDestination::Unknown => true,
                ReportedDestination::Known(target) => target != state.location.floor_reference(),
            }
        {
            return Err(WriteBlocker::Busy);
        }
        Ok(())
    }

    /// Register a hall call for the controlled composition. Dispatch is driven by an
    /// explicitly authorized request, with the CallId carried separately.
    pub fn register_controlled_call(
        &mut self,
        call: liftassistant_model::HallCall,
    ) -> Result<liftassistant_core::CallRegistration, liftassistant_core::HallCallRejection> {
        liftassistant_core::validate_hall_call(&call, &self.topology)?;
        Ok(self.calls.register(call))
    }

    /// Virtual intent admission reserves a car before lower-controller submission.
    pub fn admit_virtual_intent(
        &mut self,
        request: IntentRequest,
        grant: &AuthorityGrant,
        controller: &ControllerId,
        controller_available: bool,
    ) -> Admission {
        if let Err(reason) = self.controlled.check_grant(request.intent.car(), grant) {
            return Admission::Blocked(reason);
        }
        if let Some(record) = self.controlled.record(request.id) {
            return if record.request == request {
                Admission::Retry
            } else {
                Admission::IdentityConflict
            };
        }
        // M6A uses one deterministically allocated, monotonically issued ID domain.
        // After bounded outcome retention, the watermark still prevents re-execution.
        if self
            .controlled
            .last_admitted
            .is_some_and(|last| request.id <= last)
        {
            return Admission::RetiredIdentity;
        }
        if let Err(reason) = self.write_readiness(&request, grant, controller, controller_available)
        {
            return Admission::Blocked(reason);
        }
        self.controlled.last_admitted = Some(request.id);
        self.controlled.active.insert(
            request.intent.car().clone(),
            IntentRecord {
                request,
                controller: controller.clone(),
                authority: grant.clone(),
            },
        );
        Admission::Admitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{BuildingTopology, CarState, ExecutionCapabilities, Floor};

    #[test]
    fn admission_deduplicates_before_reservation_and_fences_revoked_grants() {
        let topology = BuildingTopology::new((1..=8).map(Floor::new)).unwrap();
        let mut engine = Engine::new(topology);
        let car = CarId::new("A");
        let controller = ControllerId::new("virtual");
        let principal = PrincipalId::new("writer");
        engine
            .bind_controlled_car(car.clone(), controller.clone())
            .unwrap();
        let grant = engine.grant_authority(&car, principal.clone()).unwrap();
        assert_eq!(
            engine.grant_authority(&car, PrincipalId::new("other")),
            Err(AuthorityError::AlreadyGranted)
        );
        let mut request = IntentRequest {
            id: IntentId::new(1),
            intent: Intent::ServeFloor {
                car: car.clone(),
                floor: Floor::new(5),
            },
            call: None,
        };
        assert_eq!(
            engine.admit_virtual_intent(request.clone(), &grant, &controller, true),
            Admission::Blocked(WriteBlocker::ObservationUnavailable)
        );
        engine
            .operational_states
            .insert(car.clone(), CarState::new("A", Floor::new(1)));
        engine.set_execution_capabilities(car.clone(), ExecutionCapabilities::reference_defaults());
        assert_eq!(
            engine.admit_virtual_intent(request.clone(), &grant, &controller, true),
            Admission::Admitted
        );
        for _ in 0..10 {
            assert_eq!(
                engine.admit_virtual_intent(request.clone(), &grant, &controller, true),
                Admission::Retry
            );
        }
        let before = engine.intent_record(request.id).cloned();
        request.intent = Intent::ParkAt {
            car: car.clone(),
            floor: Floor::new(2),
        };
        assert_eq!(
            engine.admit_virtual_intent(request.clone(), &grant, &controller, true),
            Admission::IdentityConflict
        );
        assert_eq!(engine.intent_record(request.id), before.as_ref());
        engine.revoke_authority(&car, &grant).unwrap();
        assert_ne!(
            engine.grant_authority(&car, principal).unwrap().id,
            grant.id
        );
        assert_eq!(
            engine.admit_virtual_intent(request, &grant, &controller, true),
            Admission::Blocked(WriteBlocker::AuthorityFenced)
        );
    }
}
