#![forbid(unsafe_code)]

mod adapter_validation;
mod app;
mod replay;
mod scenario;
mod ui;
mod virtual_validation;

use std::{env, io, time::Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::{App, FIXED_STEP};

// Construct before setup so partial initialization and ordinary errors restore the terminal.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    }
}

fn main() -> io::Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => run_tui(),
        [flag, name] if flag == "--scenario" => {
            let report = scenario::run(name)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            print!("{report}");
            Ok(())
        }
        [flag] if flag == "--list-scenarios" => {
            for name in scenario::SCENARIOS {
                println!("{name}");
            }
            Ok(())
        }
        [flag] if flag == "--help" || flag == "-h" => {
            println!(
                "LiftAssistant developer simulator\n\n\
                 Usage:\n  liftassistant-tui\n  liftassistant-tui --scenario <name>\n  \
                 liftassistant-tui --list-scenarios"
            );
            Ok(())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unsupported arguments; use --scenario <name> or --help",
        )),
    }
}

fn run_tui() -> io::Result<()> {
    let _guard = TerminalGuard;
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut app = App::new();
    let mut next_step = Instant::now() + FIXED_STEP;

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;
        if event::poll(next_step.saturating_duration_since(Instant::now()))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Release {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q' | 'Q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break;
                        }
                        KeyCode::Up => app.select_floor(1),
                        KeyCode::Down => app.select_floor(-1),
                        KeyCode::Char('u' | 'U') => app.call(liftassistant_model::Direction::Up),
                        KeyCode::Char('d' | 'D') => app.call(liftassistant_model::Direction::Down),
                        KeyCode::Char('a' | 'A') => app.toggle_availability("A"),
                        KeyCode::Char('b' | 'B') => app.toggle_availability("B"),
                        KeyCode::Char(' ') => {
                            app.paused = !app.paused;
                            next_step = Instant::now() + FIXED_STEP;
                        }
                        KeyCode::Char('.') if app.paused => app.step(),
                        KeyCode::Char('p' | 'P') => {
                            app.replay();
                            next_step = Instant::now() + FIXED_STEP;
                        }
                        KeyCode::Char('r' | 'R') => {
                            app.reset();
                            next_step = Instant::now() + FIXED_STEP;
                        }
                        _ => {}
                    }
                }
            }
        }
        if Instant::now() >= next_step {
            app.advance();
            // No elapsed wall time in physics and no burst catch-up steps after a stall.
            next_step = Instant::now() + FIXED_STEP;
        }
    }
    Ok(())
}
