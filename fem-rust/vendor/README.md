# FEM solver vendored dependencies

`rivrs-sparse/` is the crates.io source for `rivrs-sparse` 0.1.1, licensed under
Apache-2.0. Upstream: https://github.com/jcallaham/rivrs-linalg

The local manifest disables Faer's default features and enables only `std` and
`sparse-linalg`. This prevents Faer's optional parallel runtime from pulling in
`atomic-wait`, which does not support the project's `wasm32-wasip1` target. No
RIVRS solver or factorization source has been changed.

The `wasm32-wasip1` wrapper uses RIVRS `Par::Seq` because this build is not
configured with worker threads. Native builds continue to use the configured
parallelism. Solver backend and matrix ordering remain those requested by the
model.
