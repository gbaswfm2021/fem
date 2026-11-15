# Extend the FEM application

This guide describes the current FEM application layout and the division between schema metadata, JavaScript integration, shared libraries, and numerical executables. It uses the current names: `fem.schema.json`, `x-fullmeta-rules`, `fem_model.json`, `fem_runs/`, and `fem_results/`.

## Where the FEM contract lives

`applications/fem/fem.schema.json` is the scenario-neutral base schema. It
owns the fixed project structure, generic Physics item variants, shared UI and
validation declarations, runtime storage, and the common capability contract.
Do not add scenario materials, field definitions, profile switches, or
scenario-only boundary flags to this file. Scenario packages are registered
under `fem-rust/src/scenarios/registry.json`; their JSON contracts supply
profile declarations and are composed with the base schema in memory. The
application package is in `applications/fem/package.js`, and browser schema
discovery is in `applications/fem/schema-documents.js` and
`applications/fem/js/scenario-discovery.js`.

The project format declares `application: "fem"`, its model format, and a relative `x-fullmeta-schema` path. The current storage names are declared by the FEM runtime contract:

| Data | Current name |
| --- | --- |
| Project model | `fem_model.json` |
| Generated run folders | `fem_runs/` |
| Saved result folders | `fem_results/` |

Validation programs are stored under the schema root key `x-fullmeta-rules`. Add or change schema-owned validation there; do not introduce a second validation registry.

## Add a scenario contract

Put scenario-specific information in a new package under `fem-rust/src/scenarios/`
and add its safe sibling filename and ID to `registry.json`. The package's
`schemaDocument` carries `x-fem-scenario` declarations for profile identity,
dimension, physics, formulations, primary and derived fields, materials, and
result quantities. Its capability declarations cover mesh requirements,
supported boundary and feature variants, studies, solver settings, and other
scenario behavior already supported by the generic runtime. Keep the common
project structure and fixed Physics item variants in `fem.schema.json`; adding
a scenario must not require editing that base schema.

The browser loader and native FEM consumers discover the same registered
scenario packages at runtime. They compose the package's scenario declarations
with the generic base schema. A scenario package therefore does not need to be
added to the root schema's `$ref` list or copied into `$defs.femProfiles`.
The JSON contract controls scenario-dependent UI availability and validation
flags; it does not implement a new numerical algorithm.

## Add numerical behavior

For a new solver, add its Rust source at
`applications/fem/fem-rust/src/scenarios/<module>.rs` and declare that
module in `schemaDocument.x-fem-scenario.solverModule`. It implements the
scenario runner API documented in
[`fem-rust/src/scenarios/README.md`](../../fem-rust/src/scenarios/README.md).
The generated registry in `fem-rust/build.rs` compiles registered modules and
the shared dispatcher routes to the selected module using JSON metadata. The
runner owns scenario-specific project execution, numerical work, and checks
that need generated mesh or computed values.

The scenario package still registers its program and allowed entries under
its `schemaContributions.x-fullmeta-runtime.externalPrograms` declaration and
selects that program in `solverRuntimesByProfile`. An in-process module points
to the built `fullmeta-fem` executable; the runtime build plan already builds
that dispatcher. Adding a scenario does not add a target to
`applications/fem/runtime-build.json`, Cargo target/features, or the Windows
package binary list. Existing built-in scenario executables retain their
current routes.

If a solver writes result artifacts or quantities that use a new common
representation, add the required capability in the scenario contract where
the current result adapter supports it. A new common result format requires a
deliberate shared runtime extension. A schema declaration alone does not
implement equations, discretizations, boundary behavior, or numerical checks.
The [public FEM application guide](../../docs/README.md) describes the current
application status, and the [native executable reference](../../fem-rust/README.md)
gives the scenario command interface and extension steps.

The distributed FEM package may include `fem-rust` source for inspection and extension while omitting the stable shared Rust sources in `platform/model-rust`; the mesher is included under `fem-rust/src/mesh/`. The shared crates are path dependencies of `fem-rust`; compiling modified FEM Rust code therefore requires a complete compatible FullMeta source checkout with the shared crates available. The bundled compiled programs remain usable without that source checkout.

## Build and review

For a FEM-only Windows build and staging, follow the [FEM Rust build guide](../../fem-rust/README.md#build-fem-on-windows). To rebuild the registered workspace runtime, including shared and other application targets, use the repository's full build plan from the FullMeta source root:

```powershell
node scripts/build-runtime.mjs
```

The build requires the Rust toolchain and target configured by the project. To package already-built Windows outputs, use:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/Build-Windows-FEM-Package.ps1 -SkipRuntimeBuild
```

Review the staged package contents when changing a solver or profile: the schema and extension files, JavaScript adapters, native executables, Gmsh, and any resources referenced by the runtime must travel together. The package should not include the shared `model-rust` source tree.

## Useful source map

| Concern | Source |
| --- | --- |
| Model schema, profiles, runtime declarations | `applications/fem/fem.schema.json` |
| Application package and editor integration | `applications/fem/package.js` and `applications/fem/js/` |
| Profile schema loading | `applications/fem/schema-documents.js` |
| FEM native code | `applications/fem/fem-rust/src/` |
| Runtime targets | `applications/fem/runtime-build.json` |
| Shared runtime build planner | `scripts/runtime-build-state.mjs` and `scripts/build-runtime.mjs` |
| Windows FEM package | `scripts/Build-Windows-FEM-Package.ps1` |
