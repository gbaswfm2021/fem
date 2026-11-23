# Scenario validation alignment

All eight scenario packages opt into generated metadata checks for the element
orders they declare and semantic count checks for their primary boundary-vector
inputs. The checks are generated from `capabilities.mesh.fieldFormulations`
and `boundaryInputs`, respectively; the sparse assembler is unchanged. The
generator verifies that declared vector layouts and defaults agree with the
input count. It does not rewrite existing project models.

| Scenario | Primary vector inputs checked | Element-order metadata |
| --- | --- | --- |
| acoustic 2D | Neumann velocity, 2 components | pressure: 1 |
| acoustic 3D | Neumann velocity, 3 components | pressure: 1 |
| EM 2D | Neumann derived field, 2 components for `Ez` and `Hz` | `Ez`, `Hz`: 1 |
| EM 3D | Dirichlet and Neumann fields, 3 components | `electric`: 1, 2 |
| elastic 2D | Dirichlet displacement and Neumann traction, 2 components for each formulation | `planeStress`, `planeStrain`: 1 |
| elastic 3D | Dirichlet displacement and Neumann traction, 3 components | `displacement`: 1 |
| thermal 2D | Neumann heat flux, 2 components | `T`: 1 |
| thermal 3D | Neumann heat flux, 3 components | `T`: 1 |

The semantic vector-count check applies to enabled conditions with a present
primary `fieldValues` vector. It checks the vector type and count semantically, allowing malformed stored vectors to open in the editor. Toggle “Specify field components” off and on to recreate the scenario defaults; this replaces the current components. Disabled conditions skip these semantic checks. Structural checks on individual component contents and the solver's boundary-input checks remain in place. For EM 3D, a declared legacy array fallback through `value` is also
checked when `fieldValues` is absent; a scalar `value` remains valid. The solver
retains its own count checks for both paths.

The accidental acoustic dependency on an electromagnetic `Hz` metadata rule was
removed. `Hz` order validation is declared by EM 2D; acoustic scenarios declare
their own pressure order validation. The electric-only incident-polarization
count remains in the electromagnetic packages and was removed from acoustic
packages. Native metadata isolation tests exercise each of the eight packages.

The acoustic material-presence and excluded-property obligations were retained:
although the composed structural schema covers the same invalid properties in
normal validation, semantic-only callers rely on the obligations and their
diagnostics. Do not remove the equivalent EM or thermal obligations solely on
the basis of schema-first validation either. A generic replacement requires a
separate proof that all supported validation entry points retain behavior.

Run the four FEM-owned generation checks after editing scenario declarations:

```text
node prebuilt-tools/sync-boundary-count-validation.mjs --check
node prebuilt-tools/sync-vector-component-guards.mjs --check
node prebuilt-tools/sync-formulation-order-validation.mjs --check
node prebuilt-tools/sync-thermal-validation.mjs --check
```

Use `--write` with the relevant command to regenerate after an intentional
declaration change. These checks do not constitute full numerical or deployment
acceptance; see [the acoustic cleanup plan](plan_acoustic_validation_cleanup.md)
for the remaining end-to-end gates and limitations.
