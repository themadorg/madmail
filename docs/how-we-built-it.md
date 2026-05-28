# How Madmail v2 Was Built

This document describes the development process used to create Madmail v2: a general three-stage method for planning and building complex software with heavy AI assistance, followed by how that method was applied in practice.

## The General Method

The process has three distinct stages. The goal is to do the hard thinking **up front** so that later implementation is fast, consistent, and low-friction.

### Stage 0: Choose the Language and Foundation

Pick the language and core tooling deliberately. For Madmail v2 the choice was **Rust** for these reasons:

- It is compiled — you get concrete build artifacts and clear feedback.
- Warnings and errors act as a tight, continuous feedback loop.
- The ecosystem has a huge number of high-quality crates that solve common problems.
- The language design actively prevents many classes of mistakes, so the final result tends to be more reliable.

### Stage 1: Design the Complete Project Structure First

This is the most important and most difficult stage.

Before writing significant code, think through the **entire project** and define exactly what belongs where. The structure must be crystal clear — similar to how Laravel or Django projects feel when you open them.

- Talk with a strong large-context model (Google AI Studio / Gemini class) to work through the full design.
- The output of this stage is usually **one very large, precise definition file** that describes the project, its components, responsibilities, data flows, and folder/crate layout in detail. This file later becomes (or feeds directly into) the TDD.
- From that single file you can derive the exact directory structure, crate boundaries, module organization, and even the first set of implementation tickets.

Doing this work properly at the beginning saves enormous amounts of time and rework later. The AI (and human) always know exactly where any new piece of code should live.

### Stage 2: Execute with Fast, Cheap Agentic Tools

Once the structure and direction are solid, switch to fast and inexpensive agentic coding tools (Cursor in Auto mode, similar cheap/fast agents, etc.).

- Seed the agent with the big definition (often by placing the key content into `README.md` or a planning document).
- Bring reference implementations and prior versions into the repository using **git submodules** under a `context/` directory. This gives the AI enormous amounts of real code to study without reinventing behavior.
- Create a `docs/` folder for project documentation and a `plans/` (or `plan/`) folder for step-by-step implementation tickets.
- Work in very small, well-scoped steps with human review gates after each meaningful piece.
- **Test aggressively.** Write as many tests as possible — unit tests, functionality tests, and smoke tests. Test fast and test stage-by-stage during development rather than waiting until the end.
- **Keep the TDD alive.** Update the Technical Design Documents regularly throughout implementation whenever new understanding or constraints appear.
- For the very first plans and critical early phases, use a strong thinking model (high-quality reasoning + large context) to set the correct direction before handing detailed work to faster/cheaper agents.
- Maintain a script that can rebuild a fresh `context.txt` bundle on demand, so the planner always works with the most relevant and up-to-date reference material.

The combination of a rock-solid upfront structure + cheap fast agents + rich context references + continuous testing and design updates makes progress extremely rapid while keeping quality high.

## How This Was Applied to Madmail v2


| Stage | What Happened |
| ----- | ------------- |
| **0** | Rust was chosen as the implementation language for the reasons listed above. |
| **1** | Extensive planning sessions were run in Google AI Studio. The result was a detailed Technical Design Document (`docs/TDD/`) plus the initial breakdown of the entire project into phases. This produced the complete high-level architecture and the precise crate and module layout used today. |
| **2** | Day-to-day implementation was done in Cursor (and similar fast agents). All reference projects (Madmail v1, Stalwart, Delta Chat core, Iroh, WebRTC, cmdeploy, etc.) were brought in as git submodules under `context/`. A massive set of tiny, single-purpose tickets was created under `docs/plans/` (b1–b9 + p1). A context-bundling script (`scripts/build-planning-context.sh`) was written and used constantly so the planner could be fed a fresh snapshot of the codebase at any time. Every ticket included writing tests (unit + smoke + functionality) and was reviewed by a human before moving forward. The TDD was actively updated during execution. For the earliest and most important phases, strong thinking models were used to guide the overall direction correctly. |

The result is the structure you see today:

- Extremely granular, reviewable implementation steps in `docs/plans/`
- Authoritative design in `docs/TDD/`
- Human-friendly explanations in `docs/project/`
- Rich reference material in `context/`
- A living `docs/` tree that explains both the product and how it was built

See the companion document [AI-assisted development](ai-assisted-development.md) for the exact tools and division of labor between human and AI.

## Key Artifacts


| Topic                                 | Location                                                                     |
| ------------------------------------- | ---------------------------------------------------------------------------- |
| Phase-by-phase implementation tickets | `[docs/plans/](plans/)` (b1–b9, p1, and others)                              |
| Authoritative technical design        | `[docs/TDD/README.md](TDD/README.md)`                                        |
| Project architecture tour             | `[docs/project/README.md](project/README.md)`                                |
| Build, test, and deployment           | [13 — Build, test, and deploy](project/13-build-test-deploy.md)              |
| Context bundle generator              | `[scripts/build-planning-context.sh](../scripts/build-planning-context.sh)`  |
| Planning prompts                      | `[docs/prompts/](prompts/)`                                                  |
| Reference projects & submodules       | `[context/](context-references.md)` and `[external/](context-references.md)` |


Contributions, corrections, and additional narrative are very welcome via [GitHub Discussions](https://github.com/themadorg/madmailv2/discussions).