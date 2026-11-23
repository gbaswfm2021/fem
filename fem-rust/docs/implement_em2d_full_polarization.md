# EM2D full-polarization formulation and Study compatibility plan

## Status

**Implementation, automated acceptance, latest-bundle UI replay, and the non-gating fiber demonstration pass.** The EM2D scenario contract exposes `Ez`, `Hz`, and `Full polarization`; the shared runtime routes prescribed-\(k_z\) Harmonic, Transient, and temporal Eigenfrequency through Full, and routes Propagation Mode through Full with \(k_z\) itself as the eigenvalue. Explicit Full-at-zero selection is retained, and a zero-\(k_z\) spectral comparison matches both scalar polarization branches. Nonzero fixed \(k_z\) with `Ez` or `Hz` is rejected during preparation. The zero-\(k_z\) scalar paths remain available.

Current automated evidence includes the complete Rust suite with the matching FEM-local runtime DLLs (382 passed, 0 failed, 3 ignored), all 79 FEM Node tests, a successful staged optimized release build, and `--help` and `--capabilities` executable smoke checks. The in-app UI replay on the local 8765 host confirmed the Physics selector, Full formulation validation, valid Propagation Mode preparation, and clear rejection of Propagation Mode with `Ez`. After the stale-setting fix was staged, the saved Full-polarization circular-fiber acceptance project prepared successfully and completed six modes in 64.7 s; Mode 1 returned \(k_z=5{,}904{,}508.6065-4.68\times10^{-7}i\ \mathrm{rad/m}\) on 321 nodes and 580 triangles, within 0.00761% of the exact HE\(_{11}\) reference. The separate non-gating fiber demonstration returned eight modes in five vector families, each within 0.05% of independent exact Bessel roots. Mesh and cladding-box refinement records appear below. A follow-up regression test covers a stale `useLongitudinalWavenumber: false` value after an editor Study-type switch; the scenario-neutral Propagation Mode adapter now forces its internal cloned sample Study to use each trial \(k_z\). The focused adapter test, plus end-to-end acoustic and EM2D propagation tests using that stale state, pass, as does the full Rust suite. The top-level `npm run verify:fullmeta` command was attempted but cannot load the unrelated missing module `applications/fem3/schema-documents.js`. The in-app replay used the local 8765 application. Port 8766 is now reachable, but it serves an older FEM scenario registry with five packages rather than the current eight. Its New Model flow omits the current Elastic 2D, Elastic 3D, and Thermal 3D profiles, and applying `Full` to EM2D is rejected with `model.physics.formulation: must be one of "Ez", "Hz"`. This proves the 8766 application bundle is stale; compatibility of the current eight-package FEM bundle on that older host remains unverified.

The Settings projection decides whether to show a Study's prescribed-longitudinal-wavenumber checkbox and value input from the scenario JSON declaration and selected formulation. This is generic across scenarios. When Full is selected, leaving the checkbox off runs the Full operator with \(k_z=0\); selecting it keeps the prescribed-\(k_z\) route active even when its expression evaluates to zero. Scalar formulations retain their legacy zero-\(k_z\) routes and do not display these controls. Existing projects with a saved wavenumber value are projected with the checkbox enabled so their behavior is preserved.

The current Full transient contract declares one `elementSpace` for the form and two vector state fields, `electric` and `magnetic`. The triangle adapter allocates an independent copy of that same Whitney direct-sum space for each field block, producing a product space \(V_h\times V_h\); the assembler takes the block couplings from the JSON weak-form terms. It does not yet support declaring a different element space for each field block.

The correction is not to remove the existing zero-\(k_z\) formulations. It is to stop advertising a coupled finite-\(k_z\) solve as though it were either decoupled scalar formulation.

## Physical decision

EM2D permits spatially varying material properties and different assignments in subdomains of the \(x\)-\(y\) cross-section. At finite \(k_z\), the longitudinal electric and magnetic field components generally couple. Special homogeneous or symmetric configurations may still have pure modes, but that cannot be assumed for an arbitrary geometry and material map.

For the current scalar-isotropic material contract, define

$$
\mathbf C_k\mathbf E
= (\nabla_t-i k_z\mathbf e_z)\times\mathbf E,
\qquad
\mathbf C_k\!\left(\mu^{-1}\mathbf C_k\mathbf E\right)
-\omega^2\epsilon_c\mathbf E
=-i\omega\mathbf J_e,
\qquad
\mathbf H=\frac{i}{\omega\mu}\mathbf C_k\mathbf E.
$$

This harmonic equation solves all three electric components together and recovers all three magnetic components. A magnetic-current source keeps its separately declared Maxwell weak-load route and sign; it must not be relabelled as an electric current.

At \(k_z=0\), the scalar-isotropic equations split into the existing \(E_z\) and \(H_z\) blocks. The full formulation contains both blocks. A load confined to one block must reproduce that legacy scalar result, with the other block zero; loads in both blocks must superpose. This is a zero-\(k_z\) reduction test, not a reason to identify the full formulation with either scalar choice.

The initial Full-polarization contract remains limited to scalar-isotropic \(\epsilon\), \(\mu\), and conductivity, including spatially varying and piecewise assignments. Tensor and nonreciprocal constitutive laws remain future work.

For transient and temporal eigenfrequency Studies, begin with real prescribed \(k_z\), the usual Fourier wavenumber for a translation-invariant axial direction. Harmonic response may retain complex prescribed \(k_z\), and Propagation Mode returns generally complex \(k_z\). Allowing complex prescribed \(k_z\) in transient/eigenfrequency would represent exponential growth or decay along \(z\); it requires an explicit physical input interpretation and is not assumed by the initial plan.

## Where the formulation is selected

The current EM2D contract stores the formulation at **model.physics.formulation**, and Study settings inherit that Physics choice. Keep that single Physics-level selector and add **Full polarization** beside **Ez** and **Hz**.

The selector describes the field space used by the model. It is not a choice made implicitly by the assembler from the numerical value of \(k_z\). The Study selects which supported operator regime to run, and the shared assembler consumes that operator and field layout from the active contract.

## Study compatibility

| Physics formulation | Harmonic, \(k_z=0\) | Harmonic, prescribed-\(k_z\) regime | Transient | Temporal eigenfrequency | Propagation mode |
|---|---:|---:|---:|---:|---:|
| \(E_z\) | Supported scalar block | Rejected | Supported scalar block | Supported scalar block | Rejected |
| \(H_z\) | Supported scalar block | Rejected | Supported scalar block | Supported scalar block | Rejected |
| Full polarization | Supported; contains both uncoupled blocks at zero \(k_z\) | Supported | Supported six-field Maxwell state at zero or prescribed finite \(k_z\) | Supported at zero or fixed finite \(k_z\) when coefficients are time-independent | Supported; solves for \(k_z\) itself |

The formulation remains a project-level Physics choice and therefore applies consistently to all Study steps in that model. A model containing a prescribed-\(k_z\) Harmonic Study or any Propagation Mode Study must use Full polarization. If a scalar formulation is selected, those Study options must be unavailable or produce an actionable preparation error. They must never run by silently changing the formulation.

For Full polarization, Harmonic, Transient, and temporal Eigenfrequency Studies may use either \(k_z=0\) or a prescribed finite \(k_z\). In Transient and temporal Eigenfrequency, \(k_z\) is fixed input data; it is not an eigenvalue. All material data, sources, initial fields, and boundary data must share the declared longitudinal dependence. These finite-\(k_z\) routes are implemented in the shared FEM runtime, with scenario-specific field and weak-form details read from the EM2D JSON contract; native checks cover their finite-\(k_z\) behavior. The latest-bundle UI smoke check also selected the Full Transient step with \(k_z=10\ \mathrm{rad/m}\), prepared it, and completed one 0–1 s run in 15.65 s. That fixture had zero initial fields, so this verifies the editor-to-runtime route only; nontrivial transient physics remains covered by the native plane-wave, convergence, and energy tests.

### Longitudinal-wavenumber interface

Use one consistent control named **Use prescribed longitudinal wavenumber** on Harmonic, Transient, and temporal Eigenfrequency Study steps when Full polarization is selected.

- Unchecked means \(k_z=0\), with no independent \(k_z\) value.
- Checked reveals a \(k_z\) input in \(\mathrm{rad/m}\). Harmonic response uses the existing complex value. Transient and temporal eigenfrequency initially use a real prescribed value.
- The checked regime remains Full polarization for every parameter-sweep sample, including one whose expression evaluates to zero. This keeps the field layout and sparse pattern stable.
- For Transient, the initial state, sources, and boundary data must carry the same longitudinal factor as the evolved field.
- For temporal Eigenfrequency, the prescribed \(k_z\) is fixed while the solver returns temporal rate \(s\).
- With \(E_z\) or \(H_z\), this control is unavailable and the formulation is strictly \(z\)-invariant.
- Do not label the control simply **Finite \(k_z\)**: a typed expression or sweep can evaluate to zero.

### Harmonic Study

Keep one common Harmonic Study type for both \(k_z=0\) and prescribed \(k_z\).

- With \(E_z\) or \(H_z\), the Study is strictly \(z\)-invariant and uses its existing scalar operator.
- With Full polarization, the Study can use the full operator at zero or prescribed \(k_z\).

If the project-level formulation is changed from Full polarization to \(E_z\) or \(H_z\), any prescribed-\(k_z\) Harmonic step must be reported as incompatible until the user explicitly changes the Study regime or selects Full polarization again.

### Transient and temporal eigenfrequency

Add a Full-polarization first-order Maxwell state containing all six fields \((E_x,E_y,E_z,H_x,H_y,H_z)\) on the 2D cross-section. For a prescribed \(k_z\), use \(\partial_z\mapsto-i k_z\) in the full Maxwell curl operators. The implementation must derive and verify compatible discrete spaces for the electric and magnetic fields; it must not assume that the harmonic second-order electric-field element layout automatically defines a stable six-field transient layout.

A compatible 2D de Rham candidate is \(E_t=(E_x,E_y)\) in an oriented Whitney \(H(\mathrm{curl})\) edge space, \(E_z\) in a compatible nodal \(H^1\) space, \(H_t=(H_x,H_y)\) in a compatible \(H(\mathrm{div})\) or rotated-complex space, and \(H_z\) in its compatible scalar space. Treat this as a design candidate, not an automatic element prescription: verify discrete curl/rotation maps, interface traces, boundary traces, mass pairing, divergence constraints, and the zero-\(k_z\) block reduction before fixing the contract's element declarations.

With \(\mathbf C_k\mathbf F=(\nabla_t-i k_z\mathbf e_z)\times\mathbf F\), the source-driven first-order equations are

$$
\partial_t(\boldsymbol\epsilon\mathbf E)-\mathbf C_k\mathbf H=-\mathbf J_e,
\qquad
\partial_t(\boldsymbol\mu\mathbf H)+\mathbf C_k\mathbf E=-\mathbf J_m.
$$

For time-dependent material tensors or scalar coefficients, retain the product derivatives \(\partial_t(\boldsymbol\epsilon\mathbf E)\) and \(\partial_t(\boldsymbol\mu\mathbf H)\). The initial material contract uses scalar-isotropic coefficients; the operator notation preserves the constitutive placement needed for future tensor contracts.

At \(k_z=0\) under the scalar-isotropic contract, the full state contains the two existing decoupled blocks. It must support simultaneous electric- and magnetic-current loads with the contract-declared signs. At finite \(k_z\), preserve the coupled terms in both curl equations. Materials may vary in \(x\), \(y\), and time but remain invariant in \(z\); initial fields, sources, and boundary data must use the same longitudinal factor. Material and source time dependence continues through shared transient coefficient and load mechanisms.

The temporal eigenfrequency Study reuses the Full-polarization transient spatial operators at the selected fixed \(k_z\), when the coefficients are independent of time and the unknown eigenvalue. It retains the universal first-order pencil and returns temporal rate \(s\), not \(s^2\). Verify its zero-\(k_z\) block spectra against the \(E_z\) and \(H_z\) scalar routes and verify finite-\(k_z\) modes against analytic dispersion and the original first-order residual.

After compatible discretization and essential/periodic constraints, the source-free semidiscrete transient system is written

$$
\mathsf M(k_z)\,\dot{\mathbf u}+\mathsf K(k_z)\,\mathbf u=0,
\qquad
\mathbf u=(\mathbf E,\mathbf H).
$$

At fixed \(k_z\), a modal state \(\mathbf u(t)=\boldsymbol\phi e^{st}\) gives

$$
\bigl(\mathsf K(k_z)+s\mathsf M(k_z)\bigr)\boldsymbol\phi=0,
\qquad \boldsymbol\phi\ne0.
$$

The spatial operators are sparse and can be shared by transient execution and temporal eigenfrequency preparation. The unknown remains \(s\); this is distinct from fixed-frequency Propagation Mode, whose unknown is \(k_z\).

### Fixed-frequency propagation modes

Propagation Mode is available only with Full polarization. Retain the complete \(k_z\)-dependent operator, including its linear and quadratic terms, in the generic sparse polynomial/companion path. The returned eigenvalue is complex \(k_z\), not \(k_z^2\). Do not assume reciprocity, Hermiticity, real roots, or opposite-sign partners.

#### Propagation Mode interface

- The user adds a separate **Propagation Mode** Study step from the Study tree. It is not a Harmonic Study with a prescribed-\(k_z\) checkbox.
- The step takes a fixed real frequency and mode-search settings, including requested mode count, complex \(k_z\) target/shift in \(\mathrm{rad/m}\), iteration/Krylov limits, and residual tolerance.
- The step does not ask the user to enter \(k_z\); the solver returns \(k_z\) as each mode's eigenvalue. A returned mode may be propagating or evanescent.
- The Physics formulation remains selected once in **Physics > Formulation**. With Full polarization selected, the Propagation Mode step is enabled and its Settings identify the coupled formulation. With \(E_z\) or \(H_z\) selected, the Study-tree add action must hide or disable Propagation Mode with a short explanation.
- If a model already contains a Propagation Mode step and the Physics formulation is changed to \(E_z\) or \(H_z\), preserve the step and its data, but show a preparation error that identifies the conflicting formulation and directs the user to select Full polarization. Never silently delete the step or reinterpret it as a scalar study.
- Propagation Mode remains a Full-polarization study even when a returned eigenvalue is \(k_z=0\). The eigenvalue search must not switch field layout based on the value of a computed root.
- The result view displays all six declared cross-sectional field amplitudes, the complex \(k_z\), fixed frequency, residual, and mode normalization. It must not present an \(E_z\)-only or \(H_z\)-only selector as though the solver had searched a decoupled scalar subspace.

## Scenario contract changes

Keep scenario-specific mathematical data in **em2d.json**. Add a Full-polarization formulation entry with:

- all six result fields and their units and primary/derived roles for each Study;
- finite-element spaces, components, coefficient layout, and compatible constraints for each numerical family;
- combined transient fields and first-order operator terms;
- the full-polarization harmonic operator, longitudinal test/trial substitutions, and both source routes;
- boundary-condition meanings, field traces, and supported components;
- explicit support or rejection of each Study by formulation;
- zero-\(k_z\) compatibility and result mapping to the legacy \(E_z\) and \(H_z\) data.

Move the harmonicLongitudinalWavenumber and propagationModes declarations from the legacy \(E_z\)/\(H_z\) entries to Full polarization. Keep the legacy scalar Harmonic, Transient, and temporal Eigenfrequency contracts unchanged for \(k_z=0\). The shared Rust assembler must not branch on the EM2D scenario ID.

## Existing-project compatibility

- Existing \(k_z=0\) projects selecting \(E_z\) or \(H_z\) retain their formulation and Study behavior.
- A saved model requesting prescribed \(k_z\) with \(E_z\) or \(H_z\) receives a clear validation error and an explicit repair path to Full polarization. Do not silently rewrite its Physics selection or reinterpret saved results.
- A Propagation Mode step with a scalar formulation is incompatible and offers the same repair path.
- Preserve existing result files and readers. New Full-polarization results carry an explicit formulation ID and all declared field components.

## Implementation and verification sequence

1. **Implemented:** Physics-level selector and Study inheritance use the `Full` formulation declared in EM2D JSON.
2. **Implemented:** scenario JSON declares Full field outputs and the compatibility rules for all relevant Study types.
3. **Implemented and UI-checked:** JSON-gated prescribed-\(k_z\) checkbox and value visibility, including explicit Full-at-zero selection, plus clear preparation errors for incompatible combinations.
4. **Implemented:** finite-\(k_z\) Harmonic and Propagation Mode declarations are associated with Full; the sparse propagation solver returns \(k_z\), not \(k_z^2\).
5. **Implemented and natively checked:** Full Transient uses two declared vector field blocks, one copy of the shared triangle Whitney direct-sum space per block, and contract-declared curl couplings. Native checks cover zero and nonzero \(k_z\), discrete complex plane-wave phase and amplitude under time-step refinement, heterogeneous selected sources with mesh refinement, sparse residuals, periodic constraints, and discrete energy balance.
6. **Implemented and accepted:** temporal Eigenfrequency reuses the transient spatial form and holds prescribed \(k_z\) fixed; tests check fixed finite \(k_z\) and analytic box-mode convergence.
7. **Implemented:** preparation, assembly, and Study checks are contract-driven; the generic paths do not select EM2D by scenario ID.
8. **Native checks pass; visible editor replay complete:** sparse workspace/pattern reuse, mesh refinement, and shared interface DOFs pass native checks. The UI successfully applied Full and preserved a usable Study tree and settings panel after the formulation change. A separate latest-bundle UI smoke run selected the finite-\(k_z\) Full Transient step at \(10\ \mathrm{rad/m}\), passed preparation, and completed one 0–1 s case in 15.65 s; because the fixture's initial fields were zero, this is workflow evidence rather than a physical acceptance case.
9. **Native acceptance passes:** homogeneous propagation dispersion, mesh refinement, loss, below-cutoff behavior, two-material finite-\(k_z\) assembly, source and boundary routes, temporal rates, Full-at-zero scalar-spectrum comparison, transient plane-wave phase, heterogeneous transient convergence, periodic constraints, and energy balance are covered.
10. **Complete in the in-app browser:** the Physics selector showed `Ez`, `Hz`, and `Full polarization`; Full passed model validation and Propagation Mode preparation. Switching a true Propagation Mode Study to `Ez` produced the explicit preparation error `Propagation Mode requires the Full formulation; selected "Ez"`. Restoring Full made preparation pass again. The fiber run also exercised the fixed-frequency `k_z` eigenvalue search and result-field viewer.
11. **Complete; non-gating user demonstration:** the circular step-index optical-fiber example was built and run through the in-app UI. Five vector-mode families were identified and compared with independent exact Bessel roots; every reported real \(k_z\) is within the stated 0.05% tolerance. Mesh and cladding-box convergence are summarized below.
12. **Updated:** this scenario note reflects the implemented contracts and remaining limits.

## Final user-perspective demonstration: circular step-index fiber

This is a final demonstration after debugging and acceptance tests. It is **not an acceptance test** and its success does not replace the automated and analytic acceptance suite.

The completed two-material circular step-index fiber was built in the FullMeta UI with Full polarization and a fixed-frequency Propagation Mode Study. Its benchmark parameters were:

| Quantity | Starting value |
|---|---:|
| Core radius \(a\) | \(5\,\mu\mathrm m\) |
| Core refractive index \(n_\mathrm{core}\) | \(1.46\) |
| Cladding refractive index \(n_\mathrm{clad}\) | \(1.44\) |
| Vacuum wavelength \(\lambda_0\) | \(1.55\,\mu\mathrm m\) |
| Frequency | \(c_0/\lambda_0\) |

These values give normalized frequency \(V=(2\pi a/\lambda_0)\sqrt{n_\mathrm{core}^2-n_\mathrm{clad}^2}=4.881262589\), so several vector-mode families are guided. The reference values below were computed from the exact step-index characteristic equations before comparison with the FEM results.

Independent high-precision reference values of \(k_z\) were computed from the exact vector characteristic equations. The core solution uses Bessel \(J_m\) functions and the decaying cladding solution uses modified Bessel \(K_m\) functions. The UI searches resolved the fundamental HE\(_{11}\), TE\(_{01}\), TM\(_{01}\), HE\(_{21}\), and higher hybrid HE\(_{12}\) families. These comparisons use exact vector equations rather than weak-guidance LP approximations. For reference equations and mode classification, see [the step-index fiber vector-mode derivation](https://publications.lib.chalmers.se/records/fulltext/256179/256179.pdf).

For the starting values above, the exact isotropic vector equations give \(V=4.881262589\). The characteristic variables are \(U=a\sqrt{k_0^2n_\mathrm{core}^2-k_z^2}\), \(W=a\sqrt{k_z^2-k_0^2n_\mathrm{clad}^2}\), and \(U^2+W^2=V^2\). With \(\mu_r=1\), the TE/TM equations are \(J_1(U)/(U J_0(U))+K_1(W)/(W K_0(W))=0\) and \(n_\mathrm{core}^2J_1(U)/(U J_0(U))+n_\mathrm{clad}^2K_1(W)/(W K_0(W))=0\). For hybrid order \(m\ge1\), use

$$
\left(\frac{J'_m(U)}{U J_m(U)}+\frac{K'_m(W)}{W K_m(W)}\right)
\left(n_\mathrm{core}^2\frac{J'_m(U)}{U J_m(U)}+n_\mathrm{clad}^2\frac{K'_m(W)}{W K_m(W)}\right)
=m^2\frac{k_z^2}{k_0^2}\left(\frac{1}{U^2}+\frac{1}{W^2}\right)^2.
$$

Solving those equations gives the following reference roots. Values are rounded for reporting; the error comparisons below use the unrounded roots. Each \(m\ge1\) hybrid root has two cross-sectional orientation partners, so repeated \(k_z\) values are expected.

| Mode | \(U\) | \(W\) | \(n_\mathrm{eff}=k_z/k_0\) | \(k_z\) (rad/m) |
|---|---:|---:|---:|---:|
| HE\(_{11}\) | 1.989982207 | 4.457207117 | 1.456695003 | 5,904,957.834 |
| HE\(_{12}\) | 4.156924298 | 2.558652936 | 1.445522833 | 5,859,669.564 |
| EH\(_{11}\) | 4.400081333 | 2.113293337 | 1.443769837 | 5,852,563.503 |
| TE\(_{01}\) | 3.137219248 | 3.739596215 | 1.451771967 | 5,885,001.478 |
| TM\(_{01}\) | 3.146763372 | 3.731568670 | 1.451721685 | 5,884,797.651 |
| HE\(_{21}\) | 3.144860096 | 3.733172838 | 1.451731724 | 5,884,838.348 |

These roots are the unbounded-cladding reference. The UI model uses a finite cladding region; the completed demonstration increased its square-box half-width and refined the cross-section mesh, with both changes reported below. The nearly clustered TE\(_{01}\), TM\(_{01}\), and HE\(_{21}\) roots make this a useful check that the full-polarization solve resolves distinct vector families rather than returning only an LP approximation.

#### Completed demonstration record

The model was built through the FullMeta in-app UI with \(a=5\,\mu\mathrm m\), \(n_\mathrm{core}=1.46\), \(n_\mathrm{clad}=1.44\), \(\lambda_0=1.55\,\mu\mathrm m\), Full polarization, first-order triangular elements, and six requested modes. The cladding was truncated by a square box with half-width \(R\); it is not a circular outer boundary. The exact references above assume unbounded cladding.

At \(R=20\,\mu\mathrm m\), the refined mesh (maximum size \(h=1.5\,\mu\mathrm m\), 1,764 triangles) produced the following results. Relative errors are computed from \(\operatorname{Re}(k_z)\) against the listed exact root.

| Family | FEM mode(s) | FEM \(\operatorname{Re}(k_z)\) (rad/m) | Exact \(k_z\) (rad/m) | Relative error |
|---|---:|---:|---:|---:|
| HE\(_{11}\) | 1 | 5,904,678.381 | 5,904,957.834 | 0.00473% |
| HE\(_{11}\) | 2 | 5,904,641.123 | 5,904,957.834 | 0.00536% |
| HE\(_{12}\) | 1 (targeted search) | 5,857,554.593 | 5,859,669.564 | 0.03609% |
| HE\(_{12}\) | 2 (targeted search) | 5,857,253.969 | 5,859,669.564 | 0.04123% |
| HE\(_{21}\) | 3 | 5,883,925.350 | 5,884,838.348 | 0.01552% |
| HE\(_{21}\) | 5 | 5,883,762.088 | 5,884,838.348 | 0.01829% |
| TM\(_{01}\) | 4 | 5,883,820.116 | 5,884,797.651 | 0.01662% |
| TE\(_{01}\) | 6 | 5,883,641.503 | 5,885,001.478 | 0.02311% |

In the first search, modes 1 and 2 form the HE\(_{11}\) orientation pair, modes 3 and 5 form HE\(_{21}\), mode 4 is TM\(_{01}\), and mode 6 is TE\(_{01}\). Their longitudinal-field azimuthal orders and relative component strengths distinguish the vector families. The visible mode-3 field plot showed the expected four-lobed \(|E_z|\) HE\(_{21}\) pattern. A second search targeted the higher HE\(_{12}\) root and returned its two orientations as modes 1 and 2; the plotted \(|E_z|\) fields show the higher radial structure. The original six-mode search had residuals below \(6.4\times10^{-17}\); across both searches the largest reported imaginary part of \(k_z\) was below \(2.2\times10^{-5}\,\mathrm{rad/m}\). All eight real propagation constants are within the declared 0.05% tolerance.

For the original six-mode search, mesh refinement at fixed \(R=20\,\mu\mathrm m\) changed the base mesh from \(h=2.0\,\mu\mathrm m\) (1,002 triangles) to \(h=1.5\,\mu\mathrm m\) (1,764 triangles). Matching the identified guided roots, the error range decreased from approximately 0.0074–0.0383% to 0.0047–0.0231%; the near-degenerate TE\(_{01}\)/TM\(_{01}\)/HE\(_{21}\) cluster became easier to distinguish by its vector fields. The separate HE\(_{12}\) targeted search used the same refined mesh and its two returned roots were within 0.05% of the exact value. Increasing the square-box half-width from \(R=15\) to \(20\,\mu\mathrm m\) at \(h=2.0\,\mu\mathrm m\) changed the six returned real \(k_z\) values by at most 58.73 rad/m (about 10 ppm). This supports negligible cladding-truncation error at those box sizes for these guided modes; it does not convert the square truncation into an exact radiation boundary.

The six-mode UI search completed in 18 min 54 s; the targeted two-mode HE\(_{12}\) search completed in 17 min 55 s. Both used the same 1,764-triangle mesh and square cladding box. This user-perspective demonstration is separate from the automated acceptance suite. The saved model is `projects/em2d_full_ui_acceptance/fiber_modes_demo.json` in the local FullMeta project store and currently opens to the targeted HE\(_{12}\) result view.

## Design point for discussion

This plan recommends retaining the existing single Physics-level formulation choice. It gives all Study steps in a model one consistent field layout and matches the current Model 2 contract. If a real workflow needs one model to use \(E_z\) for one Study and Full polarization for another, formulation would need to become a Study-level setting; that requires an explicit data-model and UI design before implementation.

Complex prescribed \(k_z\) is initially limited to Harmonic response. Transient and temporal eigenfrequency use real prescribed \(k_z\) in the initial contract; supporting complex axial growth/decay in those Studies can be decided separately.
