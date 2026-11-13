# Bundled Gmsh dependency

`gmsh-4.15.2/` contains the bundled Windows 64-bit Gmsh distribution used by the Rust mesh tool. The tool invokes its `gmsh.exe` to mesh generated `.geo` files.

The distribution is third-party software; consult its included `README.txt` and `LICENSE.txt` for upstream details and terms. FullMeta integration and geometry/mesh conversion logic live in `mesh-rust/src/`.

`gmsh-4.15.2-sdk/` contains the matching upstream Windows Gmsh C API library and header. The mesher uses the library to obtain points on address-selected CAD entities for nonlinear coordinate filters before invoking the regular Gmsh executable. The library is normally found beside the bundled executable distribution; `FULLMETA_GMSH_API_PATH` can name an alternative compatible library. The SDK archive is the official Gmsh 4.15.2 Windows 64-bit SDK (SHA-256 `7513cc7096e0492af88eab8c217f313ee4fb6155a3155723042e2547f55845e8`); the distribution's license also applies to the SDK.
