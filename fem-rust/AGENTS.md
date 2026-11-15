# FEM scenario contributor boundary

Scenario additions and scenario-specific solver work belong inside this
`fem-rust/` directory. Keep each new scenario's contract in `src/scenarios/<module>.json`
and its solver implementation in `src/scenarios/<module>.rs`. Register
the package in `src/scenarios/registry.json` using the ID without hyphens as the filename stem, and set
`schemaDocument.x-fem-scenario.solverModule` to the Rust module name.

The build script discovers registered modules. Do not add a hand-maintained scenario registry module, a binary wrapper or Cargo target, or a
profile-ID branch to shared Rust code for an ordinary scenario addition.
`src/scenario_runtime.rs`, `build.rs`, and the code under `src/` outside
`scenarios/` are shared infrastructure; change them only when improving
a cross-scenario contract or behavior. Never put scenario-specific data in
`fem.schema.json` or change application files outside this directory for a
scenario addition.

Scenario contracts declare supported Study types and Physics boundary/feature
flags. Stationary orchestration is a shared Study capability; a scenario opts
in through its JSON contract and supplies only its own numerical method.
