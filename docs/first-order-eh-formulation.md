# Coupled first-order electric–magnetic formulation

Status: current experimental formulation. This note describes its equations and implementation boundary; general stability and convergence remain unestablished.

This note describes the experimental `firstOrderEH` solver mode, originally developed with the periodic split-ring example. It uses the imported tetrahedral mesh and the same first-family, lowest-order Nédélec edge basis for both fields. The current boundary contract is a collection of selected impedance conditions; the split-ring input/output arrangement is an example rather than a required solver layout.

## Strong equations and convention

The phasor convention is `exp(+i omega t)`. In an isotropic material, with conductivity included in the complex admittivity, the source-free frequency-domain equations are

\[
\nabla\times E + i\omega\mu H=0,
\qquad
\nabla\times H - (\sigma+i\omega\epsilon)E=0.
\]

For each tetrahedron, each field has one tangential line-integral DOF on each of its six edges. Thus the local pair `(E,H)` has 12 coefficients. Shared edges and periodic edge pairs are assembled/identified globally, so the global unknown count is twice the periodic edge-DOF count, not 12 times the tetrahedron count.

## Discrete spaces

Use `V_h × V_h`, where `V_h` is the lowest-order Nédélec first-family space (`H(curl)`-conforming). Let `u_j` be its local basis functions, with globally oriented/periodically identified edge coefficients. The implementation uses the same real-valued basis for both fields. The first-order system is tested directly with `v,w ∈ V_h`:

\[
\int_\Omega v\cdot(\nabla\times E_h+i\omega\mu H_h)\,dV=0,
\]
\[
\int_\Omega (\nabla\times w)\cdot H_h
 - (\sigma+i\omega\epsilon)w\cdot E_h\,dV
 + \int_{\Gamma_p} w_t\cdot(\mathbf n\times H_h)\,dS=0.
\]

The second equation uses curl integration by parts. On each selected impedance boundary, impose the prescribed impedance relation, including an incident field when that boundary is driven:

\[
\mathbf n\times H + E_t/Z=2E_{inc,t}/Z
\quad\text{with incident field},
\qquad
\mathbf n\times H + E_t/Z=0
\quad\text{without incident field}.
\]

For assembly, multiply the second tested equation by `-i omega`. Its blocks then have the same scale and signs as the established electric-only weak form after eliminating `H`:

\[
A_{EE}=(-\omega^2\epsilon+i\omega\sigma)M_E+i\omega M_{\Gamma,1/Z},
\quad A_{EH}=C,
\quad A_{HE}=-i\omega C^T,
\quad A_{HH}=i\omega\mu M_E.
\]

Here `C_ij = ∫ u_i · curl(u_j) dV`, `M_E,ij = ∫ u_i · u_j dV`, and `M_{Γ,1/Z}` integrates the tangential electric trace mass weighted by `1/Z` over selected impedance boundaries. The incident RHS is `2 i omega ∫ u_i · E_inc,t / Z dS` on driven boundaries. Both impedance and complex incident amplitude are evaluated at boundary quadrature points. The model supplies the incident polarization and spatial phase; the solver does not infer a free-space mode. Selected periodic pairs impose their Floquet-weighted edge-DOF identifications.

The block system is complex and generally nonsymmetric. The formulation's schema settings select the complex sparse LU (Faer) backend. The shared RIVRS solver also has a symmetric augmentation for nonsymmetric operators, but that does not change the configured backend support for this experimental formulation. Using identical H(curl) trial/test spaces for both equations is a straightforward Galerkin discretization; its stability and convergence have not been established generally.

## Observables

The implementation obtains H directly from the second field block. The solver stores mesh coordinates internally in `[y,z,x]` order, but field expressions and exported field components use the model's global x/y/z axes. Model-defined pointwise/global variables and integration operators select and evaluate observables; the solver requires no monitor-plane arrangement or built-in T/R normalization. The E-only formulation instead reconstructs H from `i curl(E)/(omega mu)`.

For the historical normal-incidence split-ring example, a model may average tangential `E_z` and `H_y` on selected x-normal planes and define `a=(E_z-eta0 H_y)/2`, `b=(E_z+eta0 H_y)/2`, `t=a_out/a_in` and `r=b_in/a_in`. These formulas assume a +x wave with +z electric polarization, for which `H_y=-E_z/eta0`. They require appropriate changes for other impedances, polarizations or oblique propagation and are not universal solver rules.

## Request option

Set `"fieldFormulation": "firstOrderEH"` in a normal Rust request. Omit the property or use `"electric"` to retain the current E-only formulation. This mode requires `solverBackend: "faer"`.
