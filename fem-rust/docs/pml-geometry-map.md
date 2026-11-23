# FullMeta PML geometry charts and complex stretch map

**Role:** mathematical design note linked from
[implement_perfectly_matched_layers.md](implement_perfectly_matched_layers.md).
This specifies the proposed geometry-to-map contract for a shared 2D/3D
implementation. It is a design recipe, not code or solved-model validation.

## 1. Separate real layer geometry from complex stretching

There are two maps and they have different jobs:

1. A **real geometry chart** constructs the meshable PML layer with user-given
   real thicknesses. It determines the real outer simulation boundary.
2. A **complex stretch map** changes the differential operator inside that
   real layer. It does not move the real mesh boundary or set its thickness.

For a PML element with reference coordinates
\(\boldsymbol\xi\in\widehat K\), let
\(\mathbf X_e(\boldsymbol\xi)\) map the reference element to its real
position \(\mathbf x\). The chart is locally invertible in the element. Let
\(G_e\) be the complex stretch expressed in those chart coordinates. Define

\[
\widetilde{\boldsymbol\xi}=G_e(\boldsymbol\xi),\qquad
\Phi(\mathbf x)=\mathbf X_e(\widetilde{\boldsymbol\xi}),\qquad
\boxed{J(\mathbf x)=D\mathbf X_e(\widetilde{\boldsymbol\xi})\,
DG_e(\boldsymbol\xi)\,[D\mathbf X_e(\boldsymbol\xi)]^{-1}}.
\]

This is the chain rule for
\(\Phi=\mathbf X_e\circ G_e\circ\mathbf X_e^{-1}\), so it is one rule for
dimension \(d=2\) or \(3\), for affine or curved charts, and for rotated
charts. The geometry service supplies \(\mathbf X_e\); the model/scenario
contract supplies real layer extents, active chart coordinates and profiles;
the assembler evaluates \(J\) at quadrature points. For a non-affine chart,
the chart representation must have a well-defined analytic continuation to
the complex coordinates used by \(G_e\), or provide the equivalent
continuation and derivatives directly.

The real outer boundary is \(\mathbf X_e\) evaluated at the **real outer
chart coordinates**. It is not \(\Phi\) evaluated at a complex point. The
mesh remains real; \(J\), \(J^{-1}\), and \(\det J\) enter the weak-form
pullback. At the inner PML interface require \(G_e(\boldsymbol\xi)=\boldsymbol\xi\)
and \(DG_e=I\) in the newly activated normal coordinates. This gives
\(\Phi=\mathbf x\) and \(J=I\) there, preventing an artificial jump at the
interface.

## 2. Boundary data and real outer-boundary construction

The input is the oriented physical boundary mesh/geometry, its entity
adjacency, the user's selection of which physical boundary entities launch a
PML, a generic active-direction policy, and real thickness \(L_j\) for each
active local direction. No entity is labeled “rectangle”, “circle”, or
“corner”. Outward orientation and adjacency come from the geometry/mesh
topology. The chart generator uses the same local coordinates for both the
outer geometry and the complex map.

### 2.1 Smooth boundary patch

For a codimension-one smooth patch with boundary coordinates \(\boldsymbol\eta\),
position \(\mathbf q(\boldsymbol\eta)\), and unit outward normal
\(\mathbf n(\boldsymbol\eta)\), the real normal-layer chart is

\[
\mathbf X(\boldsymbol\eta,\rho)=
\mathbf q(\boldsymbol\eta)+\rho\,\mathbf n(\boldsymbol\eta),
\qquad 0\le\rho\le L.
\]

Thus the real outer patch is explicitly

\[
\mathbf x_{\rm out}(\boldsymbol\eta)=
\mathbf q(\boldsymbol\eta)+L\,\mathbf n(\boldsymbol\eta).
\]

This is a meshable geometric offset. If the chosen stretch direction is a
user-specified outward vector \(\mathbf d\), replace \(\mathbf n\) by
\(\mathbf d\), and require \(\mathbf d\cdot\mathbf n>0\) so the extrusion
points outward. When several active directions meet, use a product chart
with one real coordinate per direction; derive that product chart from the
boundary entity's adjacency, rather than asking the user to draw or name a
corner region.

### 2.2 Explicit polygon construction from vertices

For a simple convex polygon in counterclockwise order with vertices
\(\mathbf v_i\), define edge tangent and outward normal

\[
\mathbf t_i=\frac{\mathbf v_{i+1}-\mathbf v_i}
{|\mathbf v_{i+1}-\mathbf v_i|},\qquad
\mathbf n_i=(t_{i,y},-t_{i,x}),\qquad
b_i=\mathbf n_i\cdot\mathbf v_i.
\]

Let \(L_i\) be the requested real thickness for edge \(i\), with
\(L_i=0\) for an unselected/inactive edge. The translated outer supporting
line is

\[
\ell_i^{\rm out}:\quad \mathbf n_i\cdot\mathbf x=b_i+L_i.
\]

At vertex \(\mathbf v_i\), the real outer vertex is the intersection of the
two adjacent translated lines. In closed form, with the rows ordered as
\(i-1,i\),

\[
A_i=\begin{bmatrix}\mathbf n_{i-1}^{\mathsf T}\\
                    \mathbf n_i^{\mathsf T}\end{bmatrix},\qquad
\boxed{\mathbf v_i^{\rm out}=\mathbf v_i+
A_i^{-1}\begin{bmatrix}L_{i-1}\\L_i\end{bmatrix}}.
\]

The formula follows because \(A_i\mathbf v_i=(b_{i-1},b_i)^{\mathsf T}\).
It computes outer vertices from inner vertices, boundary normals, selections,
and thicknesses. It needs no separately supplied outer polygon. Collinear
consecutive edges must first be merged; nearly parallel adjacent normals
make \(A_i\) ill-conditioned and must be diagnosed.

For a rectangle \(\mathbf c+Q([-a,a]\times[-b,b])\), \(Q\) is the
orthogonal frame inferred from its edge directions. Set \(L_i=L_x\) on the
two opposite edges whose normals are \(\pm Q\mathbf e_x\), and set the
other two \(L_i=0\). The formula gives

\[
\mathbf c+Q([-a-L_x,a+L_x]\times[-b,b]),
\]

so there are only two x-directed stripes, no top/bottom PML stripes, and no
corner PML regions. If all four edges are active, it gives

\[
\mathbf c+Q([-a-L_x,a+L_x]\times[-b-L_y,b+L_y]),
\]

with product corner charts. Translating \(\mathbf c\) translates every
generated point; rotating \(Q\) rotates the construction without changing
the local equations.

This adjacent-line formula directly covers convex polygons. A nonconvex
boundary uses the same local face/vertex charts and mesh adjacency, but its
offset must be built as a non-self-intersecting boundary-mesh extrusion (a
global intersection of all supporting half-spaces would incorrectly convexify
it). If offsets collide, reverse orientation, or change topology, the v1
geometry service must reject the layer with a diagnostic rather than invent
an invalid chart. General nonconvex offset resolution is an explicit
geometry-service requirement, not an assembler heuristic.

### 2.3 Product charts at polygon corners and 3D edges/vertices

At a 2D vertex where two active edges meet, the adjacent outward normals are
linearly independent. Let \(A_i\) be the matrix above and \(D_i=A_i^{-1}\).
The corner chart is

\[
\mathbf X_i(\rho_{i-1},\rho_i)=
\mathbf v_i+D_i\begin{bmatrix}\rho_{i-1}\\\rho_i\end{bmatrix},
\qquad 0\le\rho_j\le L_j.
\]

Since \(\mathbf n_k\cdot\mathbf d_j=\delta_{kj}\) for the columns
\(\mathbf d_j\) of \(D_i\), the point at all real outer coordinates is
exactly the intersection of the adjacent shifted supporting lines. If one
edge is inactive its thickness is zero and the product chart degenerates to
the shared side chart; do not create a zero-volume corner element.

The face strip joins these vertex charts. At either endpoint, its endpoint
curve is the corresponding corner-chart edge; interpolate the two endpoint
curves along the face coordinate \(\eta\). This gives a conforming real
chart whose inner curve is the original edge and whose outer curve lies on
\(\ell_i^{\rm out}\). For a rectangle the dual directions are the ordinary
orthogonal normals, and the chart is a standard tensor-product strip/corner
decomposition. For a nonorthogonal polygon corner, \(D_i\) supplies the
oblique local directions automatically.

In 3D, apply the same incidence construction to oriented boundary faces.
Face prisms use the face's two surface coordinates and one layer coordinate;
edge charts use the two incident active face-normal coordinates plus the
edge tangent; vertex charts use up to three independent active normals. A
chart with more than \(d\) active normals must be split by a deterministic
triangulation of its normal cone into charts with at most \(d\) independent
directions, or rejected if a conforming nonoverlapping split cannot be
constructed. Rank-deficient or badly conditioned local direction matrices
are errors, not an invitation to choose arbitrary axes.

The generated layer is the union of these real charts over the selected
boundary patches. The outer simulation boundary is the union of chart faces
where all active layer coordinates reach their real extents \(L_j\). Faces
parallel to unselected directions remain lateral boundaries; existing
periodic/port/symmetry conditions on them are retained and mapped through the
generated layer where required.

## 3. Complex stretch and Jacobian at any PML point

Let \(\rho_j\) be the real distance coordinate of active chart direction
\(j\). Its complex stretch factor is

\[
s_j(\rho_j)=\frac{d\widetilde\rho_j}{d\rho_j},\qquad
\widetilde\rho_j=g_j(\rho_j)=\int_0^{\rho_j}s_j(t)\,dt.
\]

Take \(s_j(0)=1\), so \(g_j(0)=0\) and the map is the identity at the
inner interface. All tangent/unselected chart coordinates are unchanged.
In the chart coordinates,

\[
G(\boldsymbol\eta,\boldsymbol\rho)=
(\boldsymbol\eta,g_1(\rho_1),\ldots,g_m(\rho_m)),\qquad
DG=\operatorname{diag}(I_{\eta},s_1,\ldots,s_m).
\]

For any real PML point \(\mathbf x\), invert its unique real chart to get
\(\boldsymbol\xi=\mathbf X_e^{-1}(\mathbf x)\), evaluate \(G\), and compute

\[
\boxed{J(\mathbf x)=D\mathbf X_e(G(\boldsymbol\xi))\,
DG(\boldsymbol\xi)\,[D\mathbf X_e(\boldsymbol\xi)]^{-1}}.
\]

This is the local Jacobian at a general point; it is not guessed from a
shape-name or region label. It includes curvature and oblique chart metrics
through \(D\mathbf X_e\). In an orthogonal product chart with constant
profiles, it reduces to the appropriate local principal stretches. In a
nonorthogonal corner chart with direction matrix \(D_i\), it reduces to
\(D_i\operatorname{diag}(s_{i-1},s_i)D_i^{-1}\), expressed in global
coordinates by that chart basis.

The profile definition must also fix the harmonic sign convention. For
example, with time dependence \(e^{-i\omega t}\), an illustrative polynomial
profile is

\[
s(\rho)=1+i\alpha(\rho/L)^p,\qquad
g(\rho)=\rho+i\frac{\alpha}{p+1}\frac{\rho^{p+1}}{L^p},
\]

where \(\alpha\ge0\) is dimensionless and \(p>0\). The sign of the
imaginary term reverses if the time-harmonic convention reverses. This
example defines the map but does not prescribe the FullMeta UI names or claim
that one profile is optimal for every physics. The user supplies real
geometric \(L\); the profile controls the complex displacement separately.

### 3.1 Rectangle and cuboid reduction

For an affine rectangular/cuboid chart with local orthogonal frame \(Q\),
\(D\mathbf X\) is constant. If the active chart coordinates are the axes
\(j\),

\[
DG=\operatorname{diag}(s_1,\ldots,s_d),\qquad
\boxed{J=Q\operatorname{diag}(s_1,\ldots,s_d)Q^{\mathsf T}}.
\]

One active axis pair gives two strips in 2D or a pair of faces in 3D. The
orthogonal coordinates are unchanged, so no corner charts are generated.
Activating two axes in 2D gives the side strips plus product corner charts;
activating all three axes in 3D gives faces, edge charts, and vertex charts.
The corners arise from boundary adjacency and product coordinates, not
manual corner declarations. For a varying profile, each \(s_j\) is evaluated
at its own local distance \(\rho_j\); the matrix is then point-dependent.

### 3.2 Circle and sphere reduction

For a circle or sphere centered at \(\mathbf c\), radius \(R\), and unit
radial direction \(\boldsymbol\omega\), the exact normal chart is

\[
\mathbf X(\boldsymbol\omega,\rho)=
\mathbf c+(R+\rho)\boldsymbol\omega,
\qquad r=R+\rho.
\]

The real outer radius is \(R+L\). Stretch the radial coordinate by

\[
\widetilde r=R+g(\rho)=R+\int_0^\rho s_r(t)\,dt.
\]

Differentiating \(\Phi=\mathbf c+\widetilde r\boldsymbol\omega\) gives

\[
\boxed{J=s_r\,\boldsymbol\omega\boldsymbol\omega^{\mathsf T}
+\frac{\widetilde r}{r}
\left(I-\boldsymbol\omega\boldsymbol\omega^{\mathsf T}\right)}.
\]

The radial eigenvalue is \(s_r\); every tangent eigenvalue is
\(\widetilde r/r\). This holds in 2D for a circle and in 3D for a sphere.
Translation by changing \(\mathbf c\) translates the geometry/map and leaves
\(J\) unchanged.

For a general smooth boundary the same result is written through its shape
operator. Let \(S=D_\Gamma\mathbf n\) be the derivative of its unit normal
restricted to the tangent space, with this sign convention. In the normal
chart, tangent differentials at real and stretched offsets are respectively
\((I+\rho S)D_\Gamma\mathbf q\) and
\((I+g(\rho)S)D_\Gamma\mathbf q\). Thus

\[
J=s_n\,\mathbf n\mathbf n^{\mathsf T}
+(I+g(\rho)S)(I+\rho S)^{-1}P_T,
\]

where \(P_T=I-\mathbf n\mathbf n^{\mathsf T}\) projects onto the tangent
space. For a circle/sphere, \(S=P_T/R\), yielding the boxed radial formula.
This is why the tangential tensor factors are generally not background
values, even for isotropic radial stretching.

### 3.3 Regular polygon approximation of a circle

Take a regular N-gon whose vertices lie on a circle of radius \(R\), and
offset every edge outward by the same real normal thickness \(L\). Its
inradius is \(R\cos(\pi/N)\). The adjacent-offset-line construction gives
outer circumradius

\[
R_N^{\rm out}=R+\frac{L}{\cos(\pi/N)}
\longrightarrow R+L\quad\text{as }N\to\infty.
\]

The local edge/vertex charts use the boundary tangents and adjacent normals;
their metric factors are evaluated by the same boxed \(J\) formula. Under
boundary refinement these chart metrics must converge to the smooth radial
factor \(\widetilde r/r\), and the normal factor must converge to \(s_r\).
The center can be translated arbitrarily; translation must not alter the
Jacobian. A finite polygon is treated as a polygon; vertices alone do not
certify that it was intended to be a circle.

The same convergence test applies to a sequence of refined polyhedral
approximations of a sphere. A curved geometry representation can instead use
the exact radial chart directly, without reconstructing the center from raw
vertices.

## 4. Chart transitions, validity, and assembler handoff

The generic geometry builder must follow this deterministic procedure:

1. Read selected physical boundary entities and their oriented adjacency;
   obtain unit normals/tangents from the geometry or boundary mesh.
2. Assign real extents \(L_j\) and active profile IDs from model settings.
   Unselected directions have \(L_j=0\) and an identity map.
3. Construct face charts from boundary patches; construct product charts at
   shared vertices/edges from the incident active directions using the local
   dual basis. Do this from mesh incidence, never from a user-supplied
   “corner” label.
4. Evaluate all chart faces at real \(\rho_j=L_j\) to obtain outer vertices,
   edges, and faces. Conformingly mesh the resulting layer and tag its PML
   elements and generated terminal caps.
5. Check chart determinant, orientation, overlaps, and transitions. Require
   chart positions to agree on shared interfaces; require the complex map to
   agree there as well. At a newly activated direction's start, enforce
   \(g_j(0)=0\), \(s_j(0)=1\). Reject self-intersection, a nonunique inverse
   chart, a singular/ill-conditioned direction matrix, or a nonfinite/zero
   complex determinant.
6. At each quadrature point, find its chart, invert \(\mathbf X_e\) to
   coordinates, evaluate the profile, and compute \(J\), \(J^{-1}\), and
   \(\det J\). The weak-form adapter then applies the physics-specific
   operator pullback.

The chart subdivision is geometric bookkeeping, not a scenario or shape
branch. It is required because polygon vertices/3D edges have multiple active
normal coordinates. A discontinuous chart derivative may be acceptable only
on an explicitly defined stratum interface with a conforming weak form; the
default target is a single-valued map with a well-defined piecewise Jacobian.
Never average incompatible corner Jacobians without deriving that averaging
as a valid map.

For a scalar divergence-form equation with coefficient \(A\), one common
pullback under this map is

\[
\text{volume factor}=\det J,\qquad
A_{\rm pullback}=\det(J)J^{-1}AJ^{-\mathsf T}.
\]

The exact transpose/conjugation convention depends on the weak-form and
time-harmonic conventions. This scalar formula is not a universal Maxwell,
elasticity, or transient rule. Each supported operator family must derive
its own transformation from the same \(J\).

## 5. Required mathematical checks

Before using this recipe in a PDE solve, implement geometry-only checks:

- For every chart, compare analytic \(J=D\Phi/D\mathbf x\) with finite
  differences of \(\Phi\) at interior points, near layer interfaces, and on
  each face/edge/corner stratum.
- Check \(J=I\) at the inner interface and continuity of \(\Phi\) across
  neighboring charts; test rotations and translations by covariance.
- Check the explicit rectangle cases: x-only yields the expanded x bounds
  only and \(J=Q\,\mathrm{diag}(s_x,1)Q^{\mathsf T}\); x/y-active corners
  yield \(Q\,\mathrm{diag}(s_x,s_y)Q^{\mathsf T}\).
- Check the exact radial circle/sphere formula and the regular polygon / refined
  polyhedron limits for both outer geometry and tangent/radial Jacobian
  eigenvalues.
- Check transverse periodic faces remain conformingly paired through
  unidirectional PML stripes; zero/natural termination is assigned only to
  generated terminal caps normal to active stretch directions.
- Check zero thickness, rank loss, bad orientation, self-intersection,
  chart collisions, and excessively distorted geometry fail with diagnostics.

These checks establish geometry/map consistency, not physical PML absorption.
Physical validation still requires solved benchmarks with a reflection metric
and mesh/thickness/profile refinement, as specified in the linked
implementation plan.
