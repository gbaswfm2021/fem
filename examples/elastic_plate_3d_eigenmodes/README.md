# Free elastic plate: eigenmodes

Open `fem_model.json` and run **Two eigenmodes near 1152 Hz**. This source-free, free-boundary modal model uses the geometry, mesh, density, and elastic constants of [`../elastic_plate_3d/`](../elastic_plate_3d/). It requests the two modes nearest a temporal-eigenvalue shift around the driven-response peak. The saved `uz` magnitude plot lets you inspect each mode.

The plate is 200 × 100 × 4 mm, with density 2700 kg/m³, Young's modulus 70 GPa, and Poisson ratio 0.33. It uses the same 4 mm maximum mesh size and first-order tetrahedra as the driven example. Harmonic density loss is set to zero here: the universal eigenfrequency Study uses the time-domain elastic operator and returns lossless modal rates. The driven frequency response retains its illustrative 1% density loss factor.

This plate is free, so it has six rigid-body modes at zero frequency. The requested shift targets nonzero modes near the first driven-response peak. Each mode has arbitrary amplitude and phase. The shift's imaginary component is in inverse seconds (numerically `2πf`); its persisted unit token is `Hz`. The model keeps the required Physics frequency field at a fixed 1152 Hz; the eigenfrequency Study does not use that harmonic value. The shift is placed near, but not exactly on, a natural mode.

The checked coarse mesh has 3,413 nodes and 10,215 tetrahedra. The returned modal rates are approximately `5.03e-8 + 6896.89889i s^-1` (1097.676 Hz) and `-5.88e-6 + 8540.85183i s^-1` (1359.318 Hz). Their relative residuals are below `1.4e-19`; the first mode's nonconstant `uz` field is saved for plotting. Its frequency is about 2.7 Hz above the documented 1095 Hz driven-response peak.

A driven peak depends on load coupling and damping, so it does not have to coincide exactly with a mode frequency. The 4 mm mesh and P1 tetrahedra are intentionally coarse and do not resolve plate bending accurately; use the mesh-refinement discussion in the driven example before treating these values as predictions.
