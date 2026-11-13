# Mesh appearance and scene rendering

The Graphics Console presents CAD, mesh, field, and selection as separate layers. Use the shared Scene View for the canvas, camera controls, and repaint lifecycle. Use checked preview or result data for mesh layers and declared model geometry for CAD. Store all stroke widths, lengths, and radii below in CSS pixels and scale the backing canvas for the display density. [Graphics and results schema acceptance](graphics-and-results-schema-acceptance.md)

## Layers and controls

| Layer | Data | Visible drawing |
| --- | --- | --- |
| Field | Checked result for the selected recipe, run, and case | Scalar colors over the applicable surface or slice at opacity `0.88` |
| Mesh | Checked preview or result mesh for the current scene | Translucent triangle fills, with each triangle's optional wire stroke |
| Geometry | Declared CAD for the model and current geometry focus | Exterior contours and boundary ridges |
| Selection | Current tree item's geometry or resolved named addresses | Accented geometry, faces, segments, or points |
| Annotations | Current scene and camera | Field color scale when applicable and orientation axes |

Paint over `#0c141e` in the table's order, from field through annotations. Paint mesh triangles back to front by mean projected vertex depth. Keep each triangle's fill and stroke together and break depth ties in sampled source order. This lets foreground fills soften wire strokes behind them while leaving CAD and selection readable. [Field surfaces and color scale](../RUNTIME.md#field-surfaces-and-color-scale)

In the graphics declaration, identify the Geometry, Mesh, and Edges roles for structure and field scenes. Geometry and Mesh control their own layers. Store the Edges preference per open model document and retain it when changing scenes or turning Mesh off and on. When checked wire data exists, Edges can be set while Mesh is off. Draw strokes only when Mesh is visible, Edges is on, and checked wire data is available. Declare the visibility dependency on the `mesh` role through `requiresVisible` for the edge layer. When an applicable layer has no checked data, disable its control and give the reason. Retain the saved preference so the control can resume when checked data becomes available. Layer changes preserve the selected row, Settings, and camera.

## Mesh topology and preview limits

Resolve node IDs to finite coordinates from the `V` records. Face and cell node IDs must resolve to distinct nodes within that record. Carry the physical tag as an integer for ordinary faces. Use these surface primitives in source order before camera depth sorting.

| Mesh format | Renderable triangles |
| --- | --- |
| `FULLMETA_MESH/2` | Each `Q` triangle in encounter order, carrying its own physical tag |
| `FULLMETA_MESH/1` | Each explicit `F` boundary triangle and `I` interface triangle in their combined encounter order, followed by derived exterior faces from the `T` tetrahedra |

For `T(a,b,c,d)`, visit `[a,b,c]`, `[a,b,d]`, `[a,c,d]`, `[b,c,d]`. Use a sorted node-ID triple as the face key. Count occurrences across all tetrahedra before selecting exterior faces. Append only keys that occur once and have no explicit `F` face, in first encounter order. A derived face retains its parent tetrahedron's physical tag. An `I` triangle carries interface identity and receives interface styling regardless of adjacent material tags. Structural `E`, `C`, `L`, and `P` records can support entity resolution where declared. Surface fills come from the triangle sources in the table.

Use at most `18000` triangles in the mesh paint preview. For `N` normalized triangles, take source indices `0, stride, 2 × stride, …` below `N`, with `stride = max(1, ceil(N / 18000))`. Use the same subset for fills and strokes and keep it stable across camera movements and display toggles. When sampling, report `preview draws <drawn> of <total> mesh faces`. Mesh counts, entity resolution, field statistics, and solver inputs use the full checked data.

## Mesh palette

| Triangle kind | Fill | Edges stroke |
| --- | --- | --- |
| Ordinary mesh triangle | `hsla(h,70%,55%,0.10)` | `hsla(h,75%,65%,0.55)` |
| Material interface | `rgba(72,215,178,0.12)` | `rgba(72,215,178,0.72)` |
| Mesh over a drawn field | `rgba(129,153,175,0.12)` | `rgba(163,185,204,0.36)` |

For an ordinary triangle, `h = (abs(physicalTag) × 47) mod 360`. Stroke width is `0.7`. Apply the field row to every checked mesh triangle while a field is drawn, including interface triangles, so material colors do not compete with scalar colors. Use the checked native mesh matching the displayed run and case where available. When a field provides its own checked surface topology as a fallback, that surface may be filled with `rgba(129,153,175,0.13)`. Wire strokes require checked wire data for the displayed result. The mesh styles apply equally to a prepared mesh in a structure scene and a checked result mesh, with the field row taking effect when field colors are present.

## CAD and selection

- Use `#8ea4b7` at opacity `0.72` and width `1.35` for CAD outlines. Draw 2D exterior contours and 3D boundary ridges, leaving CAD surface areas clear. A composed object shows its exterior silhouette and ridges. Its tessellation diagonals and consumed operands do not contribute ordinary CAD outlines. Selecting an operand may expose that operand through the selection layer.
- Use the shared `--accent` token for selection, with `#48d7b2` as the fallback. Selected geometry uses an opaque `2.8` outline. Selected 2D domains use fill at opacity `0.24`. Selected boundaries and edges use `4.5` rounded strokes. Selected 3D domains and boundary faces use fill at opacity `0.30` and outline at opacity `0.82` with width `0.8`. Selected points use filled radius `5` markers.
- Resolve selection from the chosen tree item and its declared addresses and scope. Show a resolvable highlight in the selection layer, including before meshing when the address resolves against CAD. Keep it independent of the CAD and mesh checkboxes and keep Settings bound to the same item.

### CAD face highlight pattern

For a 3D boundary or domain selection that resolves against CAD, derive the highlight patches in the originating surface's coordinates. For a box face, divide its two tangential extents into eight equal intervals each. Paint the selected portion of that face with the specified translucent accent fill and the regular grid of patch boundaries. A fully selected box face has 64 quadrilateral patches. For a cylinder wall, divide the circumference into 48 equal angular intervals and the length into eight equal axial intervals. For a cylinder cap, use the same angular intervals and four equal radial intervals from the center to the rim. Clip patches to the selected face region and preserve real selection boundaries and holes. Tessellating a patch for fill is allowed. Keep the fill triangles' internal diagonals and seams from the scene partition out of the stroke output.

A periodic pair or a condition that references it resolves the two boundary selections independently and paints both with the same treatment. An unrelated geometry edit that leaves both selected face regions unchanged leaves the highlight patch positions and line pattern unchanged. Keep the CAD-resolvable highlight stable when toggling Mesh or Edges and when a compatible mesh is loaded. A checked mesh may supply selection support where CAD cannot resolve it. In that fallback, stroke the selection region's boundary and real ridges, leaving internal element edges to the mesh wire layer. The graphics summary distinguishes a resolved highlight from a no-match state.

For a deterministic CAD test, create a `4 × 2 × 3 m` box at the origin with boundary address rows restricted by `y` and `y - 2`. Link the two selections in a pair and select a condition that uses it. Verify both `12 m²` faces, 64 patches per face, and nine equally spaced lines along each tangent including the two perimeter lines. Other faces remain unhighlighted. Repeat with a box at `(1, 0.5, 0.75) m` of size `(2, 1, 1.5) m` entirely inside the first box. Use the same camera and viewport to compare the selection strokes and coverage. The additional box may add CAD contours. The two highlighted face grids stay the same. Repeat with Mesh off and checked mesh data both present and absent. Inspect normalized paint output and the live canvas. Use a view in which both selected faces have sampleable interiors and check for unintended diagonal or bunched strokes.

## Camera and orientation axes

Use XY projection with screen Y inverted for 2D and orthographic projection for 3D. Center on the midpoint of the eligible scene bounds. At fit, use `0.72 × min(width, height) / max(span, 1e-12)`, where `span` is the largest model-space extent. Calculate the bounds independently of layer visibility so toggles leave the framing stable. The 3D default yaw is `0.68` radians and pitch is `-0.42` radians. Reset view restores fit, the default orientation, zoom `1`, and zero pan. It preserves the selection, Settings, and layer preferences.

Draw the axes inside a radius `31` circle with `rgba(12,20,30,0.82)` backing. Place its center `34` from the right and `30` from the bottom of the canvas. Project unit axes through the scene orientation and normalize each nonzero projected direction to length `25`. Draw rounded strokes of width `1.8` and arrowheads of length `4.5` at `±0.55` radians. Use `#ff657a` for x, `#60dca6` for y, and `#75aeff` for z. Show x and y in 2D and all three in 3D. Use `10px system-ui` for the labels and a radius `2` origin dot in `#d5e0ea`. The axes follow orientation while remaining anchored at the same canvas position during zoom and pan.

## Acceptance using specification-derived data

1. Inspect the saved structure and field scene declarations. Verify the roles, data sources, Edges visibility dependency, and annotation settings. Open an independently authored model in the browser, select a named object, and record its Settings and camera. With checked mesh available, turn Mesh off while Edges is on, set Edges off, turn Mesh back on, and then enable Edges. Verify fills with the wire strokes absent and present in the respective states. Switch to field and back and verify Edges preference, selection, Settings, and any unchanged camera. On a scene without checked mesh, verify the controls that depend on mesh data are disabled with a reason.
2. Normalize the two-tetrahedron mesh below. Assert the exact seven-face source order shown beneath it. With a browser canvas or an instrumented paint test, verify distinct ordinary tag colors, the interface color, paired fill and stroke depth order, and neutral mesh colors with a field displayed. Use a second synthetic sample with two neighboring `Q` cells of different physical tags to check 2D source order and colors.
3. Paint a synthetic normalized list of `18001` triangles. Verify the preview draws `9001` from indices `0, 2, …, 18000`, the count message reports both counts, and Edges uses that same subset. Toggle visibility and rotate the scene. Verify the chosen subset is unchanged and a resolvable entity beyond the subset still selects correctly.
4. In the browser, use a unit box with a `0.5 × 0.5 × 0.001` box joined on its top face at `(0.25, 0.25, 1)`. Verify CAD shows the exposed ridges including the thin feature at `z = 1.001`, and that triangulation diagonals appear only as applicable to the mesh. Select the geometry and an addressable face and verify their highlight styles. Zoom in and out and rotate the scene. Check that the orientation axes stay anchored and follow rotation, and Reset view restores the fit while retaining the selected row and its Settings.

```text
FULLMETA_MESH/1
V,0,0,0,0
V,1,1,0,0
V,2,0,1,0
V,3,0,0,1
V,4,0,0,-1
T,1,0,1,2,3,1
T,2,0,1,2,4,2
F,1,0,1,3,3
I,1,0,1,2,1,2
```

| Triangle nodes | Surface kind | Expected style in structure scene |
| --- | --- | --- |
| `0,1,3` | Explicit boundary, tag `3` | Ordinary hue `141` |
| `0,1,2` | Material interface | Interface palette |
| `0,2,3` | Derived exterior, tag `1` | Ordinary hue `47` |
| `1,2,3` | Derived exterior, tag `1` | Ordinary hue `47` |
| `0,1,4` | Derived exterior, tag `2` | Ordinary hue `94` |
| `0,2,4` | Derived exterior, tag `2` | Ordinary hue `94` |
| `1,2,4` | Derived exterior, tag `2` | Ordinary hue `94` |
