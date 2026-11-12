# LiftAssistant

**LiftAssistant is an open, vendor-neutral software platform for elevators.**

The long-term goal is to make elevator controllers, protocols, capabilities, policies, diagnostics, simulation, and building integrations available through one common operational model — without replacing standards that already solve part of the problem.

LiftAssistant is currently an early architecture and simulation project. **It is not approved for controlling passenger elevators or any safety-critical equipment.**

## Why

Elevator systems are becoming more software-defined, but the ecosystem remains fragmented across controller vendors, field buses, building protocols, group-control systems, maintenance platforms, and proprietary tooling.

LiftAssistant aims to provide a neutral layer above those systems:

```text
Applications / Home Assistant / BMS / research tools
                         |
                 LiftAssistant Core
      model / capability / intent / policy / diagnostics
                         |
                  Runtime boundary
            +------------+------------+
            |            |            |
         Simulator     CiA 417      Vendor adapters
                                      |
                               Existing controllers
```

The project should integrate existing standards instead of inventing replacements for them.

## v0.1 direction

The first vertical slice is intentionally small:

- Rust workspace
- canonical elevator domain model
- deterministic core logic
- runtime abstraction
- deterministic simulator
- developer TUI
- two cars / eight floors
- hall-call -> dispatch -> intent -> simulated movement -> observed state

The TUI is the first observable client, not the architecture.

## Architecture rules

1. **Stable semantics, replaceable implementations.** The specification must remain language-independent even though the reference implementation is Rust.
2. **Intent, not actuator control.** The core may request `ServeFloor(8)`; it must not expose brake, drive-enable, or motor-torque primitives.
3. **The core is vendor-neutral.** Vendor-specific parameters and protocol details belong in runtime adapters.
4. **Simulation first.** New semantics should be testable against a deterministic simulator before real hardware.
5. **Integrate existing standards.** CiA 417, BACnet elevator objects, ETSI SAREF4LIFT, GSET cross-brand dispatch, Open-RMF/RFA, and other prior art should be mapped rather than ignored.
6. **Safety authority stays below LiftAssistant.** Real motion and safety functions remain the responsibility of validated/certified execution systems.
7. **No real passenger-elevator control in the early milestones.**

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/PROJECT_MEMORY.md](docs/PROJECT_MEMORY.md), and [AGENTS.md](AGENTS.md).

## Repository status

Current milestone: **M0 — repository bootstrap**.

Next milestone: **M1 — Core + deterministic simulator + developer TUI**.

See [docs/STATUS.md](docs/STATUS.md) for the live project state.

## License

A license has **not** been selected yet. This is an explicit human-review decision before public releases or external contributions are accepted.
