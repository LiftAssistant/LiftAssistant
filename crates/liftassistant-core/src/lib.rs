#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use liftassistant_model::{Capabilities, CarId, CarState, HallCall, Intent, OperatingMode};

pub fn dispatch(
    call: &HallCall,
    cars: &[CarState],
    capabilities: &BTreeMap<CarId, Capabilities>,
) -> Option<Intent> {
    let car = cars
        .iter()
        .filter(|car| car.mode == OperatingMode::Normal)
        .filter(|car| capabilities.get(&car.id).is_some_and(|caps| caps.serve_floor))
        .min_by(|left, right| {
            let ld = (left.position.floor_position - f64::from(call.floor.index())).abs();
            let rd = (right.position.floor_position - f64::from(call.floor.index())).abs();
            ld.total_cmp(&rd).then_with(|| left.id.cmp(&right.id))
        })?;

    Some(Intent::ServeFloor { car: car.id.clone(), floor: call.floor })
}

#[cfg(test)]
mod tests {
    use super::*;
    use liftassistant_model::{Direction, Floor};

    fn caps(cars: &[CarState]) -> BTreeMap<CarId, Capabilities> {
        cars.iter().map(|c| (c.id.clone(), Capabilities::reference_defaults())).collect()
    }

    #[test]
    fn nearest_eligible_car_is_selected() {
        let cars = vec![CarState::new("A", Floor::new(3)), CarState::new("B", Floor::new(1))];
        let call = HallCall { floor: Floor::new(8), direction: Direction::Up };
        assert_eq!(
            dispatch(&call, &cars, &caps(&cars)),
            Some(Intent::ServeFloor { car: CarId::new("A"), floor: Floor::new(8) })
        );
    }

    #[test]
    fn out_of_service_car_is_not_selected() {
        let mut a = CarState::new("A", Floor::new(7));
        a.mode = OperatingMode::OutOfService;
        let cars = vec![a, CarState::new("B", Floor::new(1))];
        let call = HallCall { floor: Floor::new(8), direction: Direction::Up };
        assert_eq!(
            dispatch(&call, &cars, &caps(&cars)),
            Some(Intent::ServeFloor { car: CarId::new("B"), floor: Floor::new(8) })
        );
    }
}
