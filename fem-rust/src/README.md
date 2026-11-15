# FEM Rust source map

`dispatcher.rs` is the single FEM command entry point. It performs common
preflight, discovers the selected scenario package, and delegates numerical
work through a generated in-process module registry. `build.rs` generates that
registry from `src/scenarios/registry.json` and each package's
`solverModule` declaration.

All scenario-specific numerical kernels and module entry points live under
`scenarios/`. There are no scenario-specific Cargo binary targets or
wrapper files. A runner exports the common `run(arguments)` interface and is
invoked by `fullmeta-fem` in a child mode, preserving the existing operation,
event, and progress lifecycle.

Shared implementation stays at the source root. `coordinator.rs` owns common
project orchestration, including stationary-study preparation and execution;
`study_prepare.rs` builds schema-declared Study plans; `model_validation.rs`
checks model declarations; `scenario_runtime.rs` loads versioned scenario
packages; and the remaining modules implement common expressions, material
handling, solver requests, results, operation lifecycle, and schema access.
Scenario runners own physics algorithms and checks on numerical inputs and
generated mesh data.

The authored `fem.schema.json` is scenario-neutral. The editor and native
consumers load `src/scenarios/registry.json` and its versioned packages.
The editor composes them in `schema-documents.js`; the FEM dispatcher and
mesher use the shared loader in `scenario_runtime.rs`. These declarations
supply profile, material, Physics, field, capability, validation, UI, and
runtime data without profile-ID branches in shared FEM source.

To add a scenario, add its package and registry entry under `src/scenarios/`, then
add its numerical runner under `src/scenarios/`. Declare its `solverModule`
and supported Study types in the JSON package. The generated registry compiles
it into `fullmeta-fem`; adding an ordinary scenario does not require a new
Cargo target, shared dispatcher branch, or edit outside `fem-rust/`.

Reusable supplied-schema validation, schema bundling, tree navigation, and ID
checks live in the application-neutral `platform/model-rust/`. The shared
quantity core uses five SI dimension exponents `[m, kg, s, A, K]` and accepts
legacy four-exponent JSON vectors. See [the Rust application guide](../README.md)
for commands and scenario limits.
