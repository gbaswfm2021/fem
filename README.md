# FullMeta FEM Application

Finite-element simulation workspace for FullMeta. The application schema defines the editable FEM model and its interface; the package also includes examples, documentation, Rust tools, and the Windows runtime executables.

## Package layout

- `catalog.json` identifies this application and declares package version `2.0.0`.
- `fem.schema.json`, `schema-documents.js`, `package.js`, `js/`, and `ui/` define the application package and browser integration.
- `native-operations.json` defines FEM's native operation contracts for the Kernel and FEM executable.
- `examples/` contains sample FEM projects.
- `docs/` and `specification/` contain user and developer documentation.
- `fem-rust/src/scenarios/` contains the scenario registry, JSON contracts and Rust runners. Contract and runner filenames share a stem, such as `thermal2d.json` and `thermal2d.rs`; the model profile ID remains `thermal-2d`.
- `fem-rust/src/mesh/` contains geometry and meshing code; `fem-rust/vendor/` contains Gmsh, its SDK and the vendored solver dependency. FEM builds against the public platform SDKs and three DLLs without the platform implementation sources.
- `.build/` is a disposable Cargo cache; `.runtime/` contains the staged `fullmeta-fem` executable and its three platform DLLs.

The mesher uses both parts of Gmsh 4.15.2: `fem-rust/vendor/gmsh-4.15.2/` provides the `gmsh.exe` executable, while `fem-rust/vendor/gmsh-4.15.2-sdk/` provides the matching C API library. See [the Gmsh notes](fem-rust/vendor/GMSH.md) for details.

## Guides

- [FEM project guide](docs/README.md): profiles, model editing and current development status.
- [FEM Rust and scenario interfaces](docs/fem-rust-and-scenario-interfaces.md): native operations, scenario contracts and runner boundaries.
- [FEM Rust guide](fem-rust/README.md): native commands and a FEM-only build.
- [Scenario contributor guide](fem-rust/src/scenarios/README.md): adding or changing a scenario.
- [Mesher guide](fem-rust/src/mesh/GUIDE.md): geometry, Gmsh and mesh outputs.
- [Application specification](specification/README.md): target behavior and acceptance requirements.

Local `implementation/` and `tests/` directories contain development evidence
and checks; the FEM `.gitignore` excludes them from new Git additions.

## License

The FullMeta FEM application files are licensed under the MIT License; see `LICENSE`. Third-party software keeps its own license. In particular, the bundled Gmsh executable and SDK are distributed under the GNU GPL version 2 or later with the upstream exception; see `fem-rust/vendor/gmsh-4.15.2/gmsh-4.15.2-Windows64/LICENSE.txt` and the accompanying Gmsh `README.txt`.
