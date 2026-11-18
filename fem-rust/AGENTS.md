# FEM scenario contributor boundary

Keep each scenario's declarations in `src/scenarios/<module>.json` and
register it in `src/scenarios/registry.json` using the ID without hyphens as
the filename stem. Use a declared generic execution capability for supported
mathematics; do not add scenario-specific Rust solver files.

The build script validates registered packages. Do not add a hand-maintained
scenario registry module, a binary wrapper or Cargo target, or a profile-ID
branch to shared Rust code for an ordinary scenario addition.
`src/scenario_runtime.rs`, `build.rs`, and the code under `src/` outside
`scenarios/` are shared infrastructure; change them only when improving
a cross-scenario contract or behavior. Never put scenario-specific data in
`fem.schema.json` or change application files outside this directory for a
scenario addition.

Scenario contracts declare supported Study types and Physics boundary/feature
flags. Stationary orchestration is a shared Study capability; a scenario opts
in through its JSON contract and supplies only its own numerical method.
