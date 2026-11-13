# FullMeta FEM Application

Finite-element simulation workspace for FullMeta. The application schema defines the editable FEM model and its interface; the package also includes examples, documentation, Rust tools, and the Windows runtime executables.

## Package layout

- `catalog.json` identifies this application and declares package version `0.1.0`.
- `fem.schema.json`, `schema-documents.js`, `package.js`, `js/`, and `ui/` define the application package and browser integration.
- `examples/` contains sample FEM projects.
- `docs/` and `specification/` contain user and developer documentation.
- `fem-rust/`, `mesh-rust/`, and `.runtime/` contain the Rust tools, bundled dependencies, and runtime executables.

The mesher uses both parts of Gmsh 4.15.2: `mesh-rust/vendor/gmsh-4.15.2/` provides the `gmsh.exe` executable, while `mesh-rust/vendor/gmsh-4.15.2-sdk/` provides the matching C API library. See `mesh-rust/vendor/README.md` for details.

## License

The FullMeta FEM application files are licensed under the MIT License; see `LICENSE`. Third-party software keeps its own license. In particular, the bundled Gmsh executable and SDK are distributed under the GNU GPL version 2 or later with the upstream exception; see `mesh-rust/vendor/gmsh-4.15.2/gmsh-4.15.2-Windows64/LICENSE.txt` and the accompanying Gmsh `README.txt`.
