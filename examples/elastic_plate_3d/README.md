# Free elastic plate: driven frequency response

Open `fem_model.json` as a FEM model and run its **Driven response** Study. The sweep plots the maximum magnitude of transverse displacement, and the field result selects `uz` at 1095 Hz, near the peak of this example's coarse mesh. Stress components are also available in the field output. The sweep is a driven-response calculation; it is not an eigenfrequency Study. It saves a field file for each of its 91 frequencies, so the full run can require substantial disk space.

The plate measures 200 × 100 × 4 mm and uses an illustrative isotropic material: density 2700 kg/m³, Young's modulus 70 GPa and Poisson ratio 0.33. The density loss factor is 0.01, with effective density `rho*(1 - i*0.01)` under `exp(+i omega t)`; this illustrative damping is not a measured aluminium property. A uniform 2500 Pa transverse traction is applied to the x = 0 side face, giving a nominal 1 N resultant. The remaining faces are free. The sweep covers 450–1800 Hz at 15 Hz intervals.

## Accuracy and verification

This example deliberately uses a manageable 4 mm maximum mesh size and first-order tetrahedra. Its frequencies are **not converged** and should not be used as quantitative predictions. Three separate scans of the same plate showed:

| Maximum mesh size | Observed response peak | Frequency spacing |
| --- | ---: | ---: |
| 4 mm | 1095 Hz | 15 Hz |
| 2.5 mm | 1050 Hz | 50 Hz |
| 1.5 mm | 640 Hz | 10 Hz |

The independent Kirchhoff–Love Rayleigh–Ritz calculation in `reference_modes.py` gives approximately 527 Hz for the first bending mode coupled by this symmetric edge traction. This thin-plate model is an approximation to the 3D solid, not an exact 3D reference. After removing rigid translation and rotation, its first bending shape has a complex correlation of 0.99945 with the top-face transverse displacement at the coarse-mesh peak. The shape supports identifying the mode, while the strong frequency shift shows that more mesh refinement or a suitable higher-order solid element is needed. Refine frequency spacing around the peak as well. A small reported linear-solver residual does not establish discretization accuracy.

The reference needs Python and NumPy. Run `python reference_modes.py --degree 10`; optional `--compare-field PATH` compares a saved elastic displacement field JSON with its first bending mode. The reference comparison assumes the example's geometry and material values. A separate tetrahedral waveguide test measures normal-incidence longitudinal and shear reflection: a matched scalar impedance gives less than 1% reflected amplitude, and doubling it gives approximately one third. That test constrains the cross-section to uniform motion; it does not establish absorption for oblique waves or plate modes.

The scans above refine the overall maximum mesh size together; they do not independently vary the through-thickness and in-plane resolutions. Further resolution studies and a trusted 3D reference are needed before assigning an accuracy to the predicted frequencies. The existing P1 formulation can over-stiffen bending, so a higher-order displacement element is a practical candidate for a future extension.
