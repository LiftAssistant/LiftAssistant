# LiftAssistant Copilot repository instructions

The authoritative project agent rules are in `/AGENTS.md`.

Before substantial work, use the repository context in this order:

1. `README.md`
2. `docs/PROJECT_MEMORY.md`
3. `docs/STATUS.md`
4. `docs/ROADMAP.md`
5. `docs/ARCHITECTURE.md`
6. `AGENTS.md`
7. relevant code/tests

Do not create a conflicting Copilot-only architecture or workflow.

Key rules:
- default to SELF-ITERATION;
- HUMAN_PENDING blocks only dependent work;
- obey HUMAN-REVIEW boundaries in `AGENTS.md`;
- keep Core deterministic and vendor-neutral;
- use high-level intents, never low-level safety/actuation primitives;
- prefer working vertical slices over speculative infrastructure;
- current milestone is M2 Semantic Core;
- current next direction is persistent call identity/lifecycle and multiple pending calls;
- current repository workflow is direct `main` development unless the user says otherwise;
- before committing, verify author/committer identity; intended GitHub account is `Axiaobo7788`, not alternate account `xiaobo-kipfel`.

If any instruction here appears to conflict with `AGENTS.md`, follow `AGENTS.md` and reconcile this file.
