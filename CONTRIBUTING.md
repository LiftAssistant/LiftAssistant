# Contributing

LiftAssistant is in an early architecture phase.

Before substantial changes:
- read AGENTS.md and docs/ARCHITECTURE.md;
- check docs/STATUS.md;
- keep core independent from vendor protocols and UI;
- add tests for deterministic behavior;
- avoid dependencies that do not materially reduce complexity.

## Safety

Do not connect experimental LiftAssistant code to a passenger elevator or other safety-critical installation. Early hardware work belongs in simulation, virtual buses, HIL, benches, or controlled non-passenger environments.

## License status

No open-source license has been selected yet.
