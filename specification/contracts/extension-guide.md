# Extend the FEM application

This guide describes the current FEM application layout and the division between schema metadata, JavaScript integration, shared libraries, and numerical executables. It uses the current names: `fem.schema.json`, `x-fullmeta-rules`, `fem_model.json`, `fem_runs/`, and `fem_results/`.

## Where the FEM contract lives

In the source tree, the main application schema is `applications/fem/fem.schema.json`. It owns the shared FEM model, built-in profile schemas under `$defs.femProfiles`, UI declarations, runtime storage, executable registrations, result-adapter mappings, and validation rules. The application package is in `applications/fem/package.js`; schema document discovery is in `applications/fem/schema-documents.js`.

The project format declares `application: "fem"`, its model format, and a relative `x-fullmeta-schema` path. The current storage names are declared by the FEM runtime contract:

| Data | Current name |
| --- | --- |
| Project model | `fem_model.json` |
| Generated run folders | `fem_runs/` |
| Saved result folders | `fem_results/` |

Validation programs are stored under the schema root key `x-fullmeta-rules`. Add or change schema-owned validation there; do not introduce a second validation registry.

## Extend a profile using schema metadata

If a change can be expressed as a profile constraint, field/material declaration, default, unit, editor option, or supported study type, keep it in the profile schema and its metadata. The built-in profiles are under `$defs.femProfiles` in `fem.schema.json`; schema-document code exposes them to editor and validation APIs. The application package derives scenario metadata, launcher choices, starter values and editor options from the loaded scenario declarations. Tree visibility conditions consume those active-profile editor options, so profile-specific sections do not need a parallel hard-coded visibility table. Optional external profile schemas are loaded through the schema's `$ref` graph.

An external profile should declare its `x-fem-scenario` metadata and the profile-specific physics and validation metadata required by the FEM adapters. Add its editor profile settings and result-adapter mapping where needed. Keep the common project shape and generic conditions in the shared schema definitions; a profile should specialize them instead of copying common definitions.

Schema-only changes do not need a new Rust executable if an existing solver already supports the requested model and Study. A schema declaration alone does not implement new numerical behavior: unsupported equations, discretizations, boundary conditions, result fields, or Study types need solver and adapter code.

## Add numerical behavior

FEM Rust sources are under `applications/fem/fem-rust/src/`. The dispatcher validates and routes project requests; scenario executables own numerical work. Keep shared model interpretation in the existing schema/context APIs and put new numerical implementation in a solver module or a new executable as appropriate.

Register executable paths and allowed entry points in the FEM schema's `x-fullmeta-runtime.externalPrograms`. If the solver writes browser-readable results, add or update its result adapter in the runtime `resources.resultAdapters` mapping and package the adapter JavaScript module.

The application runtime build plan is `applications/fem/runtime-build.json`. It lists crates and build targets consumed by `scripts/build-runtime.mjs`. Register new native targets in this plan and add the corresponding schema routing and package payload. The Windows FEM package builder is `scripts/Build-Windows-FEM-Package.ps1`; its payload must contain every executable, Gmsh and resource file needed at runtime. The [public FEM application guide](../../docs/README.md) describes the current schema/build status, and the [native executable reference](../../fem-rust/README.md) gives the scenario command interface and extension steps.

The distributed FEM package may include `fem-rust` source for inspection and extension while omitting the stable shared Rust sources in `platform/model-rust` and the FEM `mesh-rust` crate. Those shared crates are path dependencies of `fem-rust`; compiling modified FEM Rust code therefore requires a complete compatible FullMeta source checkout with the shared crates available. The bundled compiled programs remain usable without that source checkout.

## Build and review

From the FullMeta source root, use the repository's registered build plan:

```powershell
node scripts/build-runtime.mjs
```

The build requires the Rust toolchain and target configured by the project. To package already-built Windows outputs, use:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/Build-Windows-FEM-Package.ps1 -SkipRuntimeBuild
```

Review the staged package contents when changing a solver or profile: the schema and extension files, JavaScript adapters, native executables, Gmsh, and any resources referenced by the runtime must travel together. The package should not include the shared `model-rust` or Rust `mesh-rust` source trees.

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
