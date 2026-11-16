use liftassistant_model::MotionState;
use liftassistant_runtime::ObservationSource;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, call_state_label, direction_name, intent_label};

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 70 || area.height < 24 {
        frame.render_widget(
            Paragraph::new("Resize terminal to at least 70 x 24. Q / Esc quit."),
            area,
        );
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(11),
        Constraint::Min(7),
        Constraint::Length(4),
    ])
    .split(area);
    let mode = if app.paused { "PAUSED" } else { "RUNNING" };
    frame.render_widget(
        Paragraph::new(format!(
            "LiftAssistant | Interactive Simulator | {mode} | step={} (500 ms)",
            app.steps
        ))
        .style(Style::default().fg(Color::Cyan)),
        rows[0],
    );

    let columns = Layout::horizontal([Constraint::Length(25), Constraint::Min(40)]).split(rows[1]);
    let states = app.runtime.car_states();
    let building: Vec<Line> = app
        .engine
        .topology()
        .served_floors()
        .iter()
        .rev()
        .map(|floor| {
            let a = if states[0].position.nearest_floor == *floor {
                "A"
            } else {
                " "
            };
            let b = if states[1].position.nearest_floor == *floor {
                "B"
            } else {
                " "
            };
            let selected = app.selected_floor == *floor;
            Line::styled(
                format!(
                    "{} {floor} |  {a}  |  {b}  |",
                    if selected { ">" } else { " " }
                ),
                if selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            )
        })
        .collect();
    frame.render_widget(
        Paragraph::new(building).block(panel("Building / selected >")),
        columns[0],
    );

    let mut details = Vec::new();
    for car in &states {
        let motion = match car.motion {
            MotionState::Stopped => "stopped",
            MotionState::Moving(direction) => direction_name(direction),
        };
        let reported_destination = car.reported_destination.to_string();
        let serve = app
            .runtime
            .capabilities(&car.id)
            .is_some_and(|caps| caps.serve_floor);
        details.push(Line::from(format!(
            "{}  {:.2}F (near {}) {motion} reported -> {reported_destination}",
            car.id, car.position.floor_position, car.position.nearest_floor
        )));
        details.push(Line::from(format!(
            "   door={:?} mode={:?}",
            car.door, car.mode
        )));
        details.push(Line::from(format!("   serve_floor={serve}")));
        details.push(Line::from(""));
    }
    frame.render_widget(Paragraph::new(details).block(panel("Cars")), columns[1]);

    let last = app.engine.last_call().map_or_else(
        || "none".into(),
        |call| format!("{} {}", call.floor, direction_name(call.direction)),
    );
    let decision = app
        .engine
        .decision()
        .map_or_else(|| "-".into(), intent_label);
    let lower_columns =
        Layout::horizontal([Constraint::Length(32), Constraint::Min(32)]).split(rows[2]);
    let calls: Vec<Line> = app
        .engine
        .calls()
        .calls()
        .map(|record| {
            Line::from(format!(
                "{} {} {}  {}",
                record.id,
                record.call.floor,
                direction_name(record.call.direction),
                call_state_label(&record.state)
            ))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(calls).block(panel("Calls")),
        lower_columns[0],
    );

    let visible = lower_columns[1].height.saturating_sub(2) as usize;
    let trace: Vec<Line> = app
        .trace
        .iter()
        .skip(app.trace.len().saturating_sub(visible))
        .map(|line| Line::from(line.as_str()))
        .collect();
    frame.render_widget(
        Paragraph::new(trace).block(panel(&format!("Events | {last} -> {decision}"))),
        lower_columns[1],
    );
    let lowest = app.engine.topology().lowest_floor();
    let highest = app.engine.topology().highest_floor();
    let terminals = if lowest == highest {
        format!("{lowest}: no hall direction")
    } else {
        format!("{lowest}: UP only; {highest}: DOWN only")
    };
    frame.render_widget(Paragraph::new(format!(
        "Selected: {} | {terminals} | rec={}\nUp/Down select | U/D call | Space pause/resume | . step (paused)\nA/B availability | P replay | R reset | Q/Esc quit", app.selected_floor, if app.recording_full { "FULL".into() } else { app.recording.len().to_string() }
    )).style(Style::default().fg(Color::Gray)), rows[3]);
}

fn panel(title: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(title)
}
