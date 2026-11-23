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

## Universal transient, single executable and Store-host compatibility — local checks, 2026-11-20

The interrupted boundary-preparation regression passed 1/1:
`study_prepare::multi_study_preparation_tests::transient_boundary_preparation_uses_declared_laws_and_per_case_parameters`.
The full Rust test suite passed 304 tests, with 0 failures and 3 ignored
(`fem-rust/target/transient_full_suite.log`).

The native operation acceptance passed all 16 cases. It stages and invokes one
`fullmeta-fem.exe` for FEM and mesh operations, and runs that same executable in
`--mesh-only` mode to produce and verify a real mesh. The installed executable
SHA-256 was
`4FB56D56EADED902BC49FE037CD787BCB88A557BB6F276964B59A8DDE15D7883`.
The mesh operation is a mode of the shared dispatcher; no separate mesher
binary is staged.

A fresh FEM payload was installed through `Install-FullMeta-Application.ps1`
into an isolated clone of the Store 0.2.2.0 host. The cloned Store Kernel
(`D25643103C1534432A5B49F6CAE974002DE715063DF35698457394D4E1A293B3`)
served the new FEM `package.js` and `fem.schema.json` with HTTP 200 and ran a
disposable acoustic-2D harmonic Study through `/api/model/study/run`. It
completed successfully and wrote the expected pressure field. The installed
runtime contained one `fullmeta-fem.exe` and the three required FEM DLLs. This
checks old-Kernel package loading and a real Study request. The old-host smoke
covered one acoustic Study; the Rust suite is the cross-scenario transient
verification. The visible browser editor was not part of this check.

Detailed local evidence is in the ignored workspace file
`.scratch/fem-store-compat-current-fem/compatibility-evidence.json`.
