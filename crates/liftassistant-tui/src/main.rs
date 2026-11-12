#![forbid(unsafe_code)]

use std::{collections::BTreeMap, time::Duration};
use liftassistant_core::dispatch;
use liftassistant_model::{Capabilities, CarId, Direction, Floor, HallCall};
use liftassistant_runtime::{ElevatorRuntime, RuntimeEvent};
use liftassistant_sim::SimRuntime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runtime = SimRuntime::reference_two_car();
    let call = HallCall { floor: Floor::new(8), direction: Direction::Up };

    let states = runtime.car_states();
    let capabilities: BTreeMap<CarId, Capabilities> = states.iter().filter_map(|car| {
        runtime.capabilities(&car.id).map(|caps| (car.id.clone(), caps))
    }).collect();

    let Some(intent) = dispatch(&call, &states, &capabilities) else {
        eprintln!("No eligible car for {call:?}");
        return Ok(());
    };

    println!("LiftAssistant bootstrap demo");
    println!("runtime: {}", runtime.runtime_name());
    println!("hall call: {call:?}");
    println!("decision: {intent:?}");
    runtime.submit_intent(intent)?;

    for tick in 0..40 {
        let events = runtime.tick(Duration::from_millis(500));
        println!();
        println!("tick {tick:02}");
        for car in runtime.car_states() {
            println!(
                "  car {}: {:>4.2}F  {:?}  door={:?} target={:?}",
                car.id, car.position.floor_position, car.motion, car.door, car.target
            );
        }
        for event in &events { println!("    event: {event:?}"); }
        if events.iter().any(|event| matches!(
            event,
            RuntimeEvent::Arrived { floor, .. } if *floor == Floor::new(8)
        )) { break; }
    }
    Ok(())
}
