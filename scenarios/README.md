# Scenarios

Scenarios are deterministic simulator inputs, not a second implementation of LiftAssistant semantics.

The interactive simulator records developer commands in memory and can replay them with P.

## First replay target

~~~text
initial state:
  car A at 3F
  car B at 1F

input:
  hall call 8F DOWN

expected semantic flow:
  dispatch chooses an eligible car
  -> ServeFloor intent
  -> fixed-step simulated movement
  -> arrival at 8F
  -> door open
~~~

The second target marks the normally preferred car out of service and verifies that another eligible car is selected.

## In-memory replay

The TUI crate owns a minimal developer-only command representation: a valid or invalid
hall-call attempt, or one fixed 500 ms simulation step. Both live input and the replay
runner use the same path. P runs the recording twice without sleeping, compares the
complete semantic event sequences and final states, verifies the live state, and leaves
the replayed session paused. Continue stepping/calling afterward, or R to reset.

Recording stops at 2,048 commands. The simulator continues, but P refuses incomplete
recordings; reset to begin recording again. The visible trace separately retains 100 entries.

No file format or normative public schema is defined. Developer UI scheduling and
floor selection are not simulation inputs; calls include their explicit floor/direction.
