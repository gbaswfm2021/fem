# Project-independent function-contract coverage method

Use this method to make each required application function precise before authoring schema declarations or application code. Assess coverage against the written requirements.

## Allowed inputs

Use the normative FEM specification, the supplied native interfaces and linked FullMeta platform contracts. Write disposable examples directly from the Model 2 contract. Measure coverage against the complete set of required functions, including their applicable states and variants. Record an undefined path or behavior as a specification gap. [Build sequence](../README.md)

## One coverage row per function

| Column | Required content |
| --- | --- |
| Function ID | Stable requirement identifier. |
| Purpose | The user need, data-integrity risk, or operational guarantee. |
| Inputs | Exact model path or runtime input, type, required status, values, units, and references. |
| Expected outcome | Exact serialized value, visible state, generated artifact, or diagnostic. |
| Schema/UI contract | Field declaration, tree path, title, visibility, ordering, widget, units, action, graphics layer, or result view. |
| Validation contract | Rule purpose, predicate, stage, diagnostic, and valid/invalid evidence; state explicitly when no validation applies. |
| Owner | Schema/FullMeta, generic platform, target application `js/`, target application `ui/`, native solver, mesher, or shared documented service. |
| Evidence | Spec-derived disposable test, UI steps, observed artifact/log, and pass criteria. For a user-facing function, record the actual Model Editor browser surface and version, canonical project path, visible outcome, and saved/reopened state where applicable. List declaration and handler checks separately. |
| Status | Specified, authored, validated, passed, N/A with reason, or blocked with reproduction. |

No row may use vague outcomes such as “works normally” or “handled by UI.” Name the actual input, component, result, and check.

## Coverage procedure

1. Inventory each function named in the normative specification: model data, field editing, tree behavior, actions, validation, project creation, graphics, native operations, result loading, and storage.
2. Inventory every reachable path and variant in the Model 2 contract. Include optional fields, empty collections, all profile and discriminator branches, stored versus effective defaults, and settings actions and failure states. Add a coverage row for each reachable behavior and a minimal spec-derived example that exercises it. Then add composite cases for every declared interaction where multiple features can alter one tree, Settings, validation, graphics, dispatch, or result behavior. Link each coverage row to the case IDs; do not treat a collection of isolated single-feature cases as coverage of their interactions. [Model 2 project JSON contract](../../docs/fullmeta-model-2.md)
3. For each field, specify the machine key separately from its display title; include type, optionality, constraints, units, widget behavior, references, and round-trip expectation.
4. For every visible tree item, specify its backing path, parent, role, order, visibility rule, selection behavior, Settings content, and graphics highlight behavior where applicable. Grouping nodes without serialized data must be explicitly presentation-only.
5. For every validation, describe purpose, inputs, predicate, diagnostic, stage, and isolated valid/invalid evidence. Assign declarative structural/reference checks to schema/FullMeta and geometry/numerical checks to the documented native boundary.
6. For every action, specify discoverability, target, inputs/defaults, owner, output/artifacts, progress/cancellation, and failure/empty state.
7. Verify each row has an authoring location and evidence step. Mark unsupported or unspecified behavior as unresolved; do not guess or inspect prohibited sources to fill it.

Before freeze, cover both sides of each visibility and enablement condition, every permitted action and its stored result, every validation predicate, and each runtime terminal state. A required user-facing workflow passes only after its prescribed browser evidence passes. An inapplicable case needs a declared profile or data condition; a required workflow blocked by the browser or host is not N/A. A behavior described only by a counter or by a fixture name remains open. [Project-level command gate](tree-context-menu-contract.md#project-level-command-gate)

Apply the same coverage template to every variant in a schema branch. Derive checks for the input dimensions, fields, ownership, UI actions, validation, and runtime effects from each variant's contract. Where variants share declarations, exercise the shared function for every applicable discriminator.

## Settings and renderer coverage

For a JSX-backed surface, record the declaration/source path, resolved projected record, public provider/compiled component, normal callback/command and visible outcome separately. Include the frontend revision and source map identity. Use the [visible replay interface](../../../../platform/docs/frontend-provider-contract.md) for reproducible state and command evidence, identifying session and fixture/live services. Retain browser focus, drag/reorder, dialogs, layout, canvas pixels and saved/reopened results; projection snapshots alone cannot cover them.

Build the expected inventory from the written Model 2, tree, Settings, action, and graphics contracts before running the candidate. Key each entry by profile, model path pattern, discriminator, and applicable state. For each path, enumerate visible, hidden, optional, and read-only controls, plus any package-owned actions, messages, or drawing it can trigger. Give each entry a spec reference and owner. A missing expected value is a specification gap and stays open. Use the coverage inventory to select disposable inputs. Use [Settings controls](settings-control-contract.md) for the required field signatures and the [graphics checks](graphics-and-results-schema-acceptance.md) for scene and paint behavior.

| Surface | Expected state to record and compare |
| --- | --- |
| Tree | Row order, level, text, disclosure, selected path, disabled state, and scoped menu entries. |
| Settings | Heading and ordered groups, each field's machine path and literal visible label, control role, editability, help where specified, exact choice labels and values, expression/unit pairing, optional visibility, and expected saved change after an edit. Use the visible control and accessible name as evidence. |
| Graphics | Scene and checked data identity, exact focus targets, visible layers and disabled reasons, framing and orientation, and what is painted within and around a target. Identify palette, line pattern, region coverage, and layer order when these affect the result. |
| Operation and result | Trigger, ordered visible states, controls available in each state, exact result source and case, status or diagnostic, and serialized or generated outputs. |

For each entry, first inspect the saved declaration and its effective schema branch. Then exercise the corresponding browser behavior. Record the starting state, action, expected state, actual state, and exact model diff after Save/reopen where applicable. Exercise an absent optional value and its explicit value, enabled and disabled reference choices, and available and unavailable artifacts wherever they change the UI. Reuse a check across variants only after verifying that the relevant declaration and runtime behavior are equivalent, and record which variants it covers. Report required entries without evidence alongside failures.

Treat a selection target and its drawing as separate assertions. For example, a periodic-condition check verifies the two resolved boundary IDs, then tests the regular face grid at a fixed camera and confirms that adding unrelated geometry leaves the grid unchanged. Test normalized drawing properties such as face coverage, line directions, stroke widths, and absence of internal seams. Witness the canvas in the browser too. This distinguishes declaration or focus errors from renderer errors.

On a failure, report the first difference in this order where applicable. Start with the saved declaration, then the effective binding, then the visible control or canvas, and finally the persisted result. Identify the component that owns the difference. Use one reproduction per distinct behavior and vary one state at a time when isolating it. Capture interacting states when a behavior depends on more than one input, such as Edges with Mesh off and an available mesh. Keep the required result and its test case in the spec so they can be replayed in a later clean build.

## Schema-editor evidence

After each schema-authoring batch, run available validation, inspect the changed tree branch and settings, record TDUV using the definitions currently displayed by the editor, and inspect the editor's branch audit for the authored schema. TDUV and branch inventory describe only what was authored; they are evidence for review, not completion criteria. Acceptance requires each normative function row to be implemented and supported by its stated evidence.

## Blocker format

When a required behavior cannot be expressed or verified, record:

- function ID and specification section;
- exact document or disposable fixture and steps;
- expected behavior and observed behavior;
- exact error text and relevant UI location;
- whether the blocker belongs to Schema Editor, application-owned package/UI, generic platform, or native program; and
- the smallest spec clarification or platform capability needed to proceed.

Repair application-owned declarations and code. Record a correctly formed request blocked by a fixed shared/native component as an unresolved dependency failure; do not create a hidden editor patch or undocumented workaround.
