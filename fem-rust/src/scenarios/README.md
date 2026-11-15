# Scenario runner extension interface

For the editor-to-native and dispatcher-to-runner boundaries, see the
[FEM Rust and scenario interface guide](../../../docs/fem-rust-and-scenario-interfaces.md).

This directory contains the scenario-specific numerical implementations. A
scenario opts in by declaring `schemaDocument.x-fem-scenario.solverModule` in
its registered package under `fem-rust/src/scenarios/`. Use a lowercase Rust module
name matching both the JSON and source filename stems, such as `thermal2d.json` and `thermal2d.rs` for the existing profile ID `thermal-2d`. Remove hyphens from the ID to form the filenames.

Each module exports the same crate-level entry point:

```rust
pub(crate) fn run(arguments: &[String]) -> Result<i32, String>
```

The build script reads only registered packages, checks module names and source
files, and generates the in-process module registry. The common dispatcher
selects a module from the package declaration, then starts the same
`fullmeta-fem` executable in its private `--scenario-module` mode. The runner
receives the solver arguments and inherits the operation's environment and
standard streams. It handles numerical requests and owns checks on its
physics-specific coefficients, constraints, mesh data, and numerical solve.

The editor-facing `--server` operation, common preflight, mesh preparation,
run records, progress lifecycle, and stationary Study orchestration stay in
shared FEM code. A scenario declares `studyType`, its preparation kind,
supported solver backends, and results in JSON. The shared `stationaryPlan`
preparation route supplies stationary solvers with a prepared request; the
scenario module does not need a separate project adapter. Frequency-domain
preparation is also shared where its declared request kind is supported.

Scenario result and Study metadata belong in the package contract. Declare
solver-consumed Study fields/defaults through
`x-fem-scenario.studyFingerprint`; use `x-fem-scenario.resultDefaults` for
result filenames, field schemas, labels, and nonstandard frequency columns.
This keeps generic editor, preparation, and result code independent of
scenario IDs.

## Adding a scenario

1. Add `src/scenarios/<module>.json` and register the package ID and filename in
   `src/scenarios/registry.json`.
2. Declare the profile's dimension, formulation and field catalogs, materials,
   Physics support flags, study support, solver runtime, solver module name,
   and result capabilities in the JSON package.
3. Add `src/scenarios/<module>.rs` and export the common `run` entry
   point. The module receives prepared solver requests, not project-level
   lifecycle work.
4. Point the scenario's distinct solver program ID to
   `.runtime/x86_64-pc-windows-gnu/release/fullmeta-fem.exe` and declare its
   supported entries. The application runtime already builds this one solver
   executable; no new Cargo target, binary wrapper, feature, or shared source
   branch is needed.
5. Rebuild and verify contract loading, mesher handoff, scenario solve, and
   result mapping.

The generated registry makes module declarations build-checked and dispatch
selection data-driven. Adding an ordinary scenario requires changes only
inside `fem-rust/`: its scenario JSON, registry entry, and solver module under
`src/`.
`fem.schema.json` and shared Rust code stay unchanged. A genuinely new shared
model semantic or output format would require a separate cross-scenario
capability extension.
