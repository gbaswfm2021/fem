# Mesh tool implementation

The Rust sources here implement the standalone `fullmeta-mesh` pipeline:

- `compiler.rs`, `emit.rs`, and `expression.rs` validate model geometry and emit Gmsh `.geo` plus its model map.
- `mesher.rs` invokes Gmsh and converts the generated `.msh` into the versioned FullMeta `.mesh` format.
- `geometry_cli.rs` and `main.rs` provide the command-line operations; `sha256.rs` supports deterministic artifact naming.

The root `mesh-rust/README.md` documents commands and output files. `vendor/` contains the bundled Gmsh distribution.

The Model 2 geometry compiler currently handles 2D rectangles and circles. The mesher selects Gmsh's 2D generation mode when the `.geo` file declares `Mesh 2;`. It exports linear triangles, adjacency-derived boundary edges, and material-interface edges as `FULLMETA_MESH/2`. The established 3D `.geo` files continue to produce `FULLMETA_MESH/1` tetrahedral output.
