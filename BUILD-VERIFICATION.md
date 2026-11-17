# FEM build verification notes

This is a historical dependency-repair record. Its hashes and test counts
identify the candidate observed at the time; they do not certify the current
combined FEM executable or complete application acceptance. Additional local
implementation records may be present under the Git-ignored `implementation/`
directory.

## Core DLL migration — local checks, 2026-11-16

The staged `fullmeta-fem.exe` had SHA-256
`C845D5E4368F5716EAD059ABAC754669A7081316A2F98B0E14AB98BF5CBBFC99`.
It was built with the core, model and operation-contracts public SDKs and
three DLLs. A separate source-free build succeeded with no platform
implementation sources or supplied core `.rlib` bundle. The core SDK tests
passed 3/3; the FEM release tests passed 56 with 2 intentionally ignored.

An isolated workspace extracted from the Store 0.2.2.0 MSIX ran that kernel,
accepted a test FEM archive through its application installer, discovered the
application and started the exact staged dispatcher. The three DLLs were present
beside it, and startup succeeded with invalid model and contracts DLLs in the
isolated platform library paths. A partial local DLL set was rejected. This
checks installation and startup; the test archive and check did not exercise
meshing, the browser editor, or an end-to-end Study.

Five alternating runs of a synthetic 210,000-point integration test gave median
times of 0.155 seconds for the previous build and 0.109 seconds for the new
build, with identical reported numerical results. In a separate 300,000-sample
unit-aware quantity microbenchmark, static scalar evaluation took about 0.031
seconds and the DLL batch path about 0.034 seconds. These measurements do not
predict the elapsed time of a full Study. A 21-frequency Study on the deployed
Store platform remains to be checked by the user after FEM deployment.

## Runtime contract schema and read-only result resolution — resolved

The FEM runtime contract requires `editorApi.resultResolve.readOnlyDuringOperation: true` for result reads during a Study (`specification/contracts/runtime-integration-boundary.md`, Resolve field/global results). The previously observed shared runtime JSON Schema at `platform/contracts/fullmeta-model-runtime-1.schema.json` had SHA-256 `5FB06D3C7C78DADAB150C0211507A65054662B101449BAFC2D7D4BC269EF9797` and did not permit that property on endpoint objects. The shared Kernel source checks the same property in `platform/kernel-server-rust/src/main.rs`.

The shared runtime schema now permits the boolean `readOnlyDuringOperation` endpoint property; its updated SHA-256 is `20A551BD77FCA8773E1EBC5EEF02D5263AC3799C42EF8BE222432A34A918B9E2`. `node --test applications/fem/tests/project-factory.test.js` passes all 21 cases, including rejection of a non-boolean value. The original failure was `$.editorApi.resultResolve: does not match an allowed schema variant. $.editorApi.resultResolve: must be string.` This validation does not establish whether a particular Kernel executable supports the behavior; live runtime acceptance remains separate.
