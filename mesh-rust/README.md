# FullMeta mesh-rust

`mesh-rust` is the standalone geometry-to-mesh command-line tool. It accepts FullMeta model JSON, compiles supported geometry to Gmsh `.geo`, runs Gmsh, and exports a versioned mesh handoff. The FEM schema registers the app-local Gmsh executable at `x-fullmeta-runtime.tools.gmsh`; the Rust runtime resolves that path relative to the schema file.

The mesher reads capabilities and reserved field/material symbols directly from the installed schema files through `src/schema_metadata.rs`. It loads the selected model schema and its recursively referenced documents, selects scenario declarations by `model.scenario.profileId`, and reserves symbols across all of that scenario's formulations. Shared Study type declarations and execution rules belong to the root model schema. The schemas remain the only authored source of those declarations. The mesher shares the generic `fullmeta-model` document loader and contains no fixed scenario or field/material symbol list.

## Commands

```text
fullmeta-mesh check model.json [--component componentId]
fullmeta-mesh compile model.json [--component componentId] [--output-dir folder] [--overwrite]
fullmeta-mesh preview - --component componentId --runs-directory fem_runs
fullmeta-mesh mesh path/to/model.geo path/to/gmsh.exe
fullmeta-mesh build model.json [--component componentId] [--output-dir folder] [--overwrite]
```

The browser editor invokes `check` or `compile` to validate and emit the current component geometry. Its **Build mesh** action then invokes `mesh` on that exact generated geometry. `build` combines those operations for command-line use. When opening a model, the browser can invoke `preview` through the Rust Kernel command runner; it computes the current geometry hash and returns the newest matching mesh-only or completed run after verifying the run record, mesh report, and cached artifact hashes. It does not run Gmsh or create a new mesh.

Model geometry contains source primitives and selections; it does not contain a user-authored BooleanFragments item. A Union is a reusable domain group: its child primitives are combined with Gmsh BooleanUnion, then the Union's ID represents the combined domain in selection addresses. Separate top-level domains are fragmented together so their shared material interfaces remain conformal. The ID-keyed selection memberships classify the resulting cells, faces, and edges for materials, Physics, mesh sizing, and integration.

The geometry compiler writes a hash-named `.geo` file and its matching `-model-map.json`. New 2D and 3D builds use `fullmeta-mesh-map/6`; the map records its dimension and provides one shared set of top-level fields for selections, physical groups, geometry features, construction features, and primitives. Its `constructionFeatures` entries preserve standalone lower-dimensional feature IDs, types, and Gmsh curve expressions without adding them to `domainFeatureIds`. Readers continue to accept version 5 maps from existing runs. Geometry feature map entries identify their Gmsh entity expressions, which can be scalar tags or arrays returned by Union. The 2D EM and acoustic profiles run Gmsh in triangle mode and write `FULLMETA_MESH/2` with V/E/J/Q records, plus C records for tagged interior curves used by clipped selections. The 2D exporter derives exterior and material-interface edges from triangle adjacency, then applies exterior boundary tags and preserves tagged interior curves separately for integration. Three-dimensional profiles export tetrahedra with their boundary and material-interface records. The Gmsh `.msh` is retained for inspection and interoperability. The Gmsh log and mesh report are written beside these artifacts. Existing hash-named geometry artifacts are reused when their contents match; changed geometry receives a new hash name.

For a nonlinear coordinate filter, the Windows mesher queries the bundled Gmsh SDK for a point on each address-selected entity and applies the expression before generating the mesh. This assumes the expression describes a contour that matches the whole intended entity. The geometry and its map must stay together: the map carries the filter expression and resolved parameters. See the [Gmsh dependency notes](vendor/README.md) for the library location and override.

## Build

Build the registered Windows release executable from the FullMeta root:

```text
node scripts/build-runtime.mjs
```

The canonical optimized executable is `applications/fem/.runtime/x86_64-pc-windows-gnu/release/fullmeta-mesh.exe`. The build plan uses this same Cargo target directory for the mesher, dispatcher, and solver executables. Package staging copies only the required executables into the install payload.

The JavaScript browser integration lives in `../js/mesh-rust/`. This root directory owns the Rust CLI and bundled Gmsh; the JavaScript folder owns only the browser/server request and artifact-handling helpers.
