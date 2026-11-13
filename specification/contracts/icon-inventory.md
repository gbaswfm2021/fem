# target application icon assets

This catalog is a normative visual specification for the target application interface. Assign the listed token to its named UI role and preserve its SVG payload exactly. Icon appearance is part of the required behavior; do not substitute a neutral icon or redraw an asset.

## UI role map

| UI role | Icon token |
| --- | --- |
| Acoustic physics | ICON-001 |
| Selection and address selection | ICON-002 |
| Box primitive | ICON-003 |
| Circle primitive | ICON-004 |
| Circular curve | ICON-005 |
| Cylinder primitive | ICON-006 |
| Disk surface | ICON-007 |
| Electromagnetic physics | ICON-008 |
| Thermal study | ICON-009 |
| Integration operator and field sampling | ICON-010 |
| Field result | ICON-011 |
| Harmonic study | ICON-012 |
| Geometry features collection, including composed Features | ICON-013 |
| Intersection feature | ICON-014 |
| Line segment | ICON-015 |
| Mesh size control | ICON-016 |
| Periodic pair | ICON-017 |
| Mesh collection | ICON-018 |
| Material assignment | ICON-019 |
| Parameter collection | ICON-020 |
| Pointwise variable | ICON-021 |
| Results collection | ICON-022 |
| Study collection | ICON-023 |
| Variable collection | ICON-024 |
| Parameter sweep | ICON-025 |
| Planar rectangle | ICON-026 |
| Rectangle primitive | ICON-027 |
| Dirichlet boundary condition | ICON-028 |
| Impedance boundary condition | ICON-029 |
| Neumann boundary condition | ICON-030 |
| Periodic boundary condition referencing a pair | ICON-031 |
| Physics interface | ICON-032 |
| Sweep plot | ICON-033 |
| Sweep result | ICON-034 |
| Union feature | ICON-035 |
| Document or project root | ICON-036 |

## Schema Editor tree bindings

Bind the SVG to the declaration that the Schema Editor projects for each row. Store a declaration icon under `x-fullmeta-ui-behavior.payload.presentation.icon` on that declaration. For a row backed by a `$ref`, verify the resolved declaration displayed by the shared projection. A projected condition or collection must have its icon at its own projected declaration. The declaration paths below identify the owning schema fields by their model role and name. Resolve them through the authored references when recording the saved schema pointer.

| Projected schema row | Owning declaration | Icon token |
| --- | --- | --- |
| Document title | Document root `#` | ICON-036 |
| Definitions | Definitions group descriptor in `x-fullmeta-ui-tree` | No icon |
| Parameters | Model `parameters` property | ICON-020 |
| Global variables | Model `variables` property | ICON-024 |
| Pointwise variables | Model `pointwiseVariables` property | ICON-021 |
| Geometry features | Geometry `features` property bound to the Geometry features heading | ICON-013 |
| Rectangle, Circle, Box, Cylinder | Concrete feature variant for each primitive | ICON-027, ICON-004, ICON-003, ICON-006 respectively |
| Circular curve, Line, Disk, Planar rectangle | Concrete feature variant for each primitive | ICON-005, ICON-015, ICON-007, ICON-026 respectively |
| Union, Intersection | Concrete operation variant for each operation | ICON-035, ICON-014 respectively |
| Features under Union or Intersection | Each operation variant's `features` property | ICON-013 on both declarations |
| Selections and each named-selection variant | Model `selections` property and each concrete selection declaration | ICON-002 |
| Integration operators and each integration-operator variant | Model `integrationOperators` property and each concrete operator declaration | ICON-010 |
| Materials and each material variant | Model `materials` property and each concrete material declaration | ICON-019 |
| Physics | The resolved Physics declaration bound to the `physics` property | ICON-032 |
| Dirichlet, Neumann, Impedance, Periodic condition | Each concrete condition declaration as projected from Physics | ICON-028, ICON-030, ICON-029, ICON-031 respectively |
| Mesh | Resolved Mesh declaration | ICON-018 |
| Mesh size control, Periodic pair | Each concrete Mesh item declaration | ICON-016, ICON-017 respectively |
| Study | Model `study` property | ICON-023 |
| Harmonic study, Thermal stationary study | Each concrete Study variant declaration | ICON-012, ICON-009 respectively |
| Parameter sweep | Resolved `parameterSweep` declaration under the applicable Study | ICON-025 |
| Results | Model `results` property | ICON-022 |
| Field result, Sweep result, Sweep plot | Each concrete result or plot declaration | ICON-011, ICON-034, ICON-033 respectively |

Use the shared Schema Editor presentation authoring on the owning declaration or the root contract authoring tools for the root envelope. After Preview, Apply and Save, reopen the schema and expand the declarations to verify the saved bindings. The folder icon belongs to the document title. The Definitions descriptor leaves the icon unset. `ICON-001` and `ICON-008` belong to explicitly acoustic and electromagnetic family roles where exposed. The shared Physics row uses `ICON-032` for every family.

## Model Editor tree bindings

Render the project title with `ICON-036` and the Definitions heading without an icon. Use the collection and item roles in the Schema Editor table for the corresponding project rows. A collection glyph belongs to its heading. A concrete item's selected variant determines its glyph after active profile and item resolution. In particular, Geometry and each Union or Intersection Features heading use `ICON-013`. Nested Box, Rectangle, Circle and Cylinder rows use `ICON-003`, `ICON-027`, `ICON-004` and `ICON-006` respectively at every supported nesting depth. Apply the same concrete-variant rule to the other primitive and operation rows. Collection and add-action artwork belongs to the collection and action surfaces.

The Physics row at `/model/physics` uses `ICON-032` in every supported profile. Each boundary condition resolves its own glyph. A Parameter sweep row at `/model/study/<index>/parameterSweep` uses `ICON-025` whenever that object is present. Preserve this binding through active profile and Study variant resolution. A Study without a parameter sweep has no sweep row.

## Icon acceptance

Check both surfaces. In the saved Schema Editor projection, record the row path, resolved source declaration or group descriptor, visible row label, expected token and observed SVG for each binding in the table. For the collection rows that share `ICON-013`, separately verify that the Geometry collection reads **Geometry features** and that the collections inside Union and Intersection read **Features**. In the rendered tree, verify the SVG against the matching literal payload below after decoding the image source and normalizing line endings and surrounding whitespace. Check the absence of an icon on Definitions as a separate assertion. A count of SVG payloads or populated icon fields does not establish the row bindings.

Exercise the Model Editor with disposable documents written from the specification. Cover the concrete primitive rows at root and within composed geometry, both Union and Intersection Features collections, Physics across supported profiles, condition variants, and a harmonic Study with and without a sweep. For each exercised row, record the model path, role, visible label, and decoded SVG or expected absence. In each editor, capture the expanded Geometry features and nested Features rows in the same tree state so the evidence shows both headings and their parent paths. A valid Schema Editor projection and a passing Model Editor tree are separate observations.

## Exact SVG payloads

### ICON-001


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round"><path d="M2 6h2.5L8 3v10l-3.5-3H2z"/><path d="M10 5.2a4 4 0 0 1 0 5.6M11.8 3.5a6.3 6.3 0 0 1 0 9"/></svg>
```

### ICON-002


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round"><path d="M2 5V2h3M11 2h3v3M14 11v3h-3M5 14H2v-3"/><circle cx="8" cy="8" r="2.2"/><path d="M8 4v1.5M8 10.5V12M4 8h1.5M10.5 8H12"/></svg>
```

### ICON-003


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linejoin="round"><path d="m8 1.8 6 3.1v6.2l-6 3.1-6-3.1V4.9z"/><path d="m2 4.9 6 3.2 6-3.2M8 8.1v6.1"/></svg>
```

### ICON-004


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5"><circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="1" fill="#8bdacb" stroke="none"/></svg>
```

### ICON-005


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.6" stroke-linecap="round"><path d="M12.6 4.8A5.8 5.8 0 1 0 13.7 9"/><path d="m11.6 3.3 1.6 1.5-2.2.5"/></svg>
```

### ICON-006


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4"><path d="M2.5 4v8c0 1.2 2.5 2.2 5.5 2.2s5.5-1 5.5-2.2V4"/><ellipse cx="8" cy="4" rx="5.5" ry="2.2"/><path d="M2.5 8c0 1.2 2.5 2.2 5.5 2.2s5.5-1 5.5-2.2" opacity=".6"/></svg>
```

### ICON-007


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4"><circle cx="8" cy="8" r="5.5"/><path d="M8 8V2.5A5.5 5.5 0 0 1 13.5 8z" fill="#8bdacb" fill-opacity=".25"/></svg>
```

### ICON-008


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round"><path d="M2 8h1.5c0-4 4.5-4 4.5 0s4.5 4 4.5 0H14M2 11h1.5c0-4 4.5-4 4.5 0"/><circle cx="13.7" cy="8" r="1" fill="#8bdacb" stroke="none"/></svg>
```

### ICON-009


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2 2.5v10.8h12"/><path d="m3.5 10.5 2.4-3.2 2.3 1.8 2.2-2H14"/><circle cx="13.2" cy="7.1" r=".8" fill="#8bdacb"/></svg>
```

### ICON-010


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M7.2 2.2c-2.5 0-1.4 2.1-2.6 4.6-1 2.1-2.5 2.5-2.5 4.1 0 1.3 1.3 2 2.5 2h1.2M8.8 2.2c2.5 0 1.4 2.1 2.6 4.6 1 2.1 2.5 2.5 2.5 4.1 0 1.3-1.3 2-2.5 2h-1.2"/><path d="M2 14h12"/></svg>
```

### ICON-011


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.2"><rect x="2" y="2" width="12" height="12" rx="1"/><path d="M6 2v12M10 2v12M2 6h12M2 10h12"/><path d="M6 6h4v4H6z" fill="#8bdacb" fill-opacity=".35"/></svg>
```

### ICON-012


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round"><path d="M1.5 8h2c1.5 0 1.2-4.5 2.7-4.5S7.4 12.5 9 12.5s1.3-9 2.8-9 1.2 4.5 2.7 4.5h.5"/></svg>
```

### ICON-013


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.2" stroke-linejoin="round"><path d="m5.5 2 4 2.2v4.4l-4 2.2-4-2.2V4.2zM1.5 4.2l4 2.3 4-2.3M5.5 6.5v4.3"/><circle cx="11.5" cy="11.2" r="3"/><path d="M11.5 8.2v6M8.5 11.2h6"/></svg>
```

### ICON-014


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none" stroke="#75cfc4" stroke-width="1.5">
  <circle cx="7" cy="10" r="5.4"/>
  <circle cx="13" cy="10" r="5.4"/>
  <path d="M10 5.1a5.4 5.4 0 0 1 0 9.8 5.4 5.4 0 0 1 0-9.8" fill="#75cfc4" fill-opacity=".3" stroke="none"/>
</svg>
```

### ICON-015


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round"><path d="m3 12.5 10-9"/><circle cx="3" cy="12.5" r="1.5" fill="#121e2a"/><circle cx="13" cy="3.5" r="1.5" fill="#121e2a"/></svg>
```

### ICON-016


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="m8 2 6 11H2z"/><path d="M8 5v3M8 10.5v.1"/><circle cx="8" cy="8.8" r="5.9" opacity=".4"/></svg>
```

### ICON-017


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3v10M14 3v10M4 5h7M9 3l2 2-2 2M12 11H5M7 9l-2 2 2 2"/></svg>
```

### ICON-018


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.2" stroke-linejoin="round"><path d="m8 1.5 6 3.3v6.4L8 14.5l-6-3.3V4.8zM2 4.8l6 3.4 6-3.4M8 8.2v6.3M5 3.1 11 6.5M11 3.1 5 6.5M2 8l6-3.2L14 8M2 10.8l6-3.1 6 3.1"/></svg>
```

### ICON-019


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linejoin="round"><path d="m8 2 5.5 2.8L8 7.6 2.5 4.8z"/><path d="m2.5 7.2 5.5 2.8 5.5-2.8M2.5 9.8l5.5 2.8 5.5-2.8"/></svg>
```

### ICON-020


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 3h12M2 8h8M2 13h6"/><circle cx="12" cy="11.5" r="2"/></svg>
```

### ICON-021


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 13.5h12M2.5 13V3"/><path d="M3.5 10.5c2-6 3 3 5-2s3 2 5-4"/><circle cx="8" cy="8.5" r="1" fill="#8bdacb" stroke="none"/></svg>
```

### ICON-022


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"><path d="M2 2v12h12"/><path d="m4 10 2.5-3 2.2 1.8L13 4.5"/><circle cx="4" cy="10" r=".8" fill="#8bdacb"/><circle cx="6.5" cy="7" r=".8" fill="#8bdacb"/><circle cx="8.7" cy="8.8" r=".8" fill="#8bdacb"/><circle cx="13" cy="4.5" r=".8" fill="#8bdacb"/></svg>
```

### ICON-023


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="2" width="11" height="12" rx="1.5"/><path d="M5 5h6M5 8h3"/><path d="m8 10 3 1.8L8 13.5z" fill="#8bdacb" fill-opacity=".3"/></svg>
```

### ICON-024


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="6"/><path d="M2.5 6h11M2.5 10h11M8 2c2.6 2.8 2.6 9.2 0 12M8 2c-2.6 2.8-2.6 9.2 0 12"/></svg>
```

### ICON-025


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5a5.5 5.5 0 0 1 9.5-1.2L14 5M13 11a5.5 5.5 0 0 1-9.5 1.2L2 11"/><path d="M14 2.5V5h-2.5M2 13.5V11h2.5"/><circle cx="8" cy="8" r="1.6"/></svg>
```

### ICON-026


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linejoin="round"><path d="m2 5 8-3 4 9-8 3z"/><path d="m2 5 4 9" opacity=".6"/></svg>
```

### ICON-027


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linejoin="round"><rect x="2" y="3" width="12" height="10" rx=".5"/></svg>
```

### ICON-028


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 2v12M6 3h7v10H6z"/><path d="M8 5v6M10 5v6M12 5v6"/></svg>
```

### ICON-029


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 4h10L3 12h10"/><path d="M2 2v12"/></svg>
```

### ICON-030


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12h12M8 12V3M5.5 5.5 8 3l2.5 2.5"/><path d="M4 14v-2M12 14v-2"/></svg>
```

### ICON-031


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2 5h10M9 2l3 3-3 3M14 11H4M7 8l-3 3 3 3"/></svg>
```

### ICON-032


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v4.3M8 10.2v4.3M1.5 8h4.3M10.2 8h4.3"/><circle cx="8" cy="1.5" r=".7" fill="#8bdacb"/><circle cx="14.5" cy="8" r=".7" fill="#8bdacb"/><circle cx="8" cy="14.5" r=".7" fill="#8bdacb"/><circle cx="1.5" cy="8" r=".7" fill="#8bdacb"/></svg>
```

### ICON-033


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2 2v12h12"/><path d="M4 10.5 6.5 7 9 9l3-4"/><path d="M10.5 5H12v1.5"/></svg>
```

### ICON-034


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M2 2v12h12"/><path d="m4 11 2.5-3 2 1.4 3.8-5"/><circle cx="6.5" cy="8" r=".8" fill="#8bdacb"/><circle cx="8.5" cy="9.4" r=".8" fill="#8bdacb"/><circle cx="12.3" cy="4.4" r=".8" fill="#8bdacb"/></svg>
```

### ICON-035


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="none" stroke="#75cfc4" stroke-width="1.5" stroke-linejoin="round">
  <path d="M3 5.2 8 2.5l5 2.7v5.6l-5 2.7-5-2.7z"/>
  <path d="m7 9.1 5-2.7 5 2.7v5.6l-5 2.7-5-2.7z"/>
  <path d="M8 13.5v-5M12 17.4v-5" opacity=".55"/>
</svg>
```

### ICON-036


```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none" stroke="#8bdacb" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round"><path d="M1.5 4a1.5 1.5 0 0 1 1.5-1.5h3l1.5 2H13A1.5 1.5 0 0 1 14.5 6v6A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12z"/><path d="M1.8 6h12.4"/></svg>
```



