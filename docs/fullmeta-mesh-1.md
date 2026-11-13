# FullMeta mesh formats 1 and 2

Updated: 2026-10-25. These are mesh-format version numbers. Both formats are in current use.

The `.mesh` file is FullMeta's downstream mesh handoff. `mesh-rust` creates format 1 for tetrahedra and format 2 for triangles after resolving material tags against the model map. Native solvers and the Graphics preview read the appropriate format. Gmsh `.msh` remains available beside it for inspection and interoperability.

The first line is `FULLMETA_MESH/1`. Remaining records are comma-separated ASCII rows:

| Record | Fields after the record label | Meaning |
| --- | --- | --- |
| `V` | `nodeId,x,y,z` | Node coordinates in meters. Node IDs are compact, zero-based indices. |
| `F` | `faceId,node0,node1,node2,physicalTag` | Tagged boundary triangle. |
| `L` | `edgeId,node0,node1,physicalTag` | Tagged boundary edge segment, such as a clipped edge selection. |
| `I` | `faceId,node0,node1,node2,materialTag0,materialTag1` | Triangle between tetrahedra with different resolved material tags. |
| `T` | `tetId,node0,node1,node2,node3,materialTag` | Linear tetrahedron and its resolved material tag. |
| `P` | `pointId,nodeId,physicalTag` | Tagged point-selection node. |

The file ends with `FULLMETA_MESH_DATA_END`. IDs in face and tetrahedron records refer to `V` node IDs. Material and physical tags are interpreted with the matching `-model-map.json`. `I` records are for interface display; the FEM solver reads `V`, `F`, and `T` and constructs its element adjacency from tetrahedra.

`mesh-rust` also emits `FULLMETA_MESH/2` when Gmsh produces a 2D triangular mesh. Its records are:

| Record | Fields after the record label | Meaning |
| --- | --- | --- |
| `V` | `nodeId,x,y,z` | Node coordinates in meters; 2D nodes lie at `z=0`. |
| `E` | `edgeId,node0,node1,physicalTag` | Boundary line segment. `physicalTag` is zero when no boundary selection matches that edge. |
| `C` | `edgeId,node0,node1,physicalTag` | Tagged internal curve segment, such as a clipped edge selection within a 2D domain. |
| `J` | `edgeId,node0,node1,materialTag0,materialTag1` | Edge between triangular cells with different material tags. |
| `Q` | `triangleId,node0,node1,node2,materialTag` | Linear triangular cell and its resolved material tag. |
| `P` | `pointId,nodeId,physicalTag` | Tagged point-selection node. |

The format 2 file ends with `FULLMETA_MESH_DATA_END`. Boundary edges are derived from triangle adjacency, then tagged to match the model map's boundary selections. Tagged Gmsh curves inside the domain are emitted separately as `C` records. Edges without a matching selection keep tag zero. The browser preview parser reads the tagged segments so it can highlight clipped selections.

The 2D acoustic, electromagnetic and thermal solvers consume triangular meshes. The 3D acoustic and electromagnetic solvers consume tetrahedral meshes. New geometry compilation writes a dimensioned `fullmeta-mesh-map/6` sidecar; readers also retain support for existing version 5 maps. The mesh and its matching map must describe the same generated geometry and selections. See the [mesh tool guide](../mesh-rust/README.md) for compilation and export commands.
