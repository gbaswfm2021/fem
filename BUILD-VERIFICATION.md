# FEM build verification notes

## Runtime contract schema and read-only result resolution — resolved

The FEM runtime contract requires `editorApi.resultResolve.readOnlyDuringOperation: true` for result reads during a Study (`specification/contracts/runtime-integration-boundary.md`, Resolve field/global results). The shared runtime JSON Schema at `platform/contracts/fullmeta-model-runtime-1.schema.json` has SHA-256 `5FB06D3C7C78DADAB150C0211507A65054662B101449BAFC2D7D4BC269EF9797` and does not permit that property on endpoint objects. The shared Kernel source checks the same property in `platform/kernel-server-rust/src/main.rs`.

The shared runtime schema now permits the boolean `readOnlyDuringOperation` endpoint property; its updated SHA-256 is `20A551BD77FCA8773E1EBC5EEF02D5263AC3799C42EF8BE222432A34A918B9E2`. `node --test applications/fem/tests/project-factory.test.js` passes all 21 cases, including rejection of a non-boolean value. The original failure was `$.editorApi.resultResolve: does not match an allowed schema variant. $.editorApi.resultResolve: must be string.` This validation does not establish whether a particular Kernel executable supports the behavior; live runtime acceptance remains separate.
