# Free rectangle: plane-stress response

Choose the **plane-stress** example from the FEM application's Examples menu to create and open a project copy, then run the **Driven response** Study. The companion plane-strain example uses the same geometry, material and loading. The result `Maximum transverse displacement` plots the maximum nodal magnitude of `uy`; the field result selects `uy` near the first peak. Other declared displacement and stress components can be selected for a field result. Each example has its own project folder and generated run storage.

The rectangle is 200 × 100 mm, with illustrative density 2700 kg/m³, Young's modulus 70 GPa and Poisson ratio 0.33. A uniform harmonic `+y` traction of 2500 Pa acts on the complete edge at `x = 0`; the other in-plane edges are free. The density loss factor is 0.01, using `rho*(1 - i*0.01)` with `exp(+i omega t)`. This is an illustrative effective loss, not measured aluminium damping. The sweep runs from 2 to 20 kHz at 250 Hz intervals and saves a field for each point.

The solver uses physical surface traction and works **per unit out-of-plane depth**; the model does not store thickness. The applied resultant is 250 N/m of depth. If interpreting the plane-stress result as a sheet 4 mm thick, that corresponds to a total force of 1 N. Plane strain instead represents a different out-of-plane constraint. These are in-plane motions; neither model computes the out-of-plane flexural modes of the Elastic3D plate example.

## Numerical comparison

The independent `reference_modes.py` script solves a free-rectangle in-plane Rayleigh–Ritz eigenproblem with a tensor-product Legendre basis. It predicts the first mode coupled by this uniform edge load at about **8281.8 Hz** for plane stress and **8677.5 Hz** for plane strain at polynomial degree 10. Increasing degree from 8 to 10 shifts those values by less than 0.04 Hz. It is an independent approximation to the same 2D continuum, not a 3D reference. Symmetry makes the edge coupling to some other modes nearly zero.

| Formulation | Maximum triangle size | Frequency spacing | First response peak | Displacement DOFs |
| --- | ---: | ---: | ---: | ---: |
| Plane stress | 10 mm | 25 Hz | 8350 Hz | 550 |
| Plane stress | 5 mm | 25 Hz | 8300 Hz | 1994 |
| Plane strain | 10 mm | 25 Hz | 8750 Hz | 550 |
| Plane strain | 5 mm | 25 Hz | 8700 Hz | 1994 |

Refining the 5 mm scans from 25 Hz to 10 Hz did not change the sampled peaks. These are damped response maxima; the reference values are undamped eigenfrequencies, so they need not coincide exactly. The coarse example's 250 Hz sampling selects 8250 Hz and 8750 Hz, respectively. After projecting out the three rigid motions, the complex displacement shapes at those samples correlate with the first reference mode by 0.999965 and 0.999993. Further refinement is needed for a quantitative accuracy claim, especially for stresses, higher modes, bending-dominated shapes or nearly incompressible plane strain.

Run `python reference_modes.py --formulation planeStress --degree 10` (or `planeStrain`); NumPy is required. With `--compare-field PATH`, the script compares a saved displacement-field JSON to the first reference mode. The reference assumes the dimensions and material listed above.

The scalar impedance option is separate from this free-body example. Constrained normal-incidence waveguide checks for both formulations gave less than 1% reflected amplitude when matched and about one third when the impedance was doubled. Those checks do not establish absorption for oblique or mixed modes. Periodic conditions, incident impedance excitation, componentwise constraints, body forces and variable thickness are not implemented in this scenario.
