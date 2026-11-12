# Scenarios

Scenarios are deterministic simulator inputs, not a second implementation of LiftAssistant semantics.

During M1, scenario support should grow out of the interactive simulator rather than being designed in advance.

## First replay target

~~~text
initial state:
  car A at 3F
  car B at 1F

input:
  hall call 8F UP

expected semantic flow:
  dispatch chooses an eligible car
  -> ServeFloor intent
  -> fixed-step simulated movement
  -> arrival at 8F
  -> door open
~~~

The second target marks the normally preferred car out of service and verifies that another eligible car is selected.

## Format policy

Do not freeze a general scenario schema before M1.1 works.

For M1.2:
1. define the minimal command representation needed by the running simulator;
2. replay it in memory;
3. only then add a small human-readable file representation if useful;
4. keep simulator-only developer controls out of the future public elevator specification.
