// Build FEM's browser runtime from the current platform sources without
// replacing the platform's deployed WASM files.
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { toolchain } from './common.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const workspace = resolve(here, '../../../..');
const runtime = resolve(workspace, 'applications/fem/js/runtime');
const target = resolve(workspace, 'applications/fem/fem-rust/target/fem-wasm');
const cargo = process.env.CARGO || resolve(homedir(), '.cargo/bin', process.platform === 'win32' ? 'cargo.exe' : 'cargo');
for (const crate of ['core-rust', 'model-rust']) {
  const manifest = resolve(workspace, 'platform', crate, 'Cargo.toml');
  const result = spawnSync(cargo, [...(toolchain ? [`+${toolchain}`] : []), 'build', '--manifest-path', manifest,
    '--target', 'wasm32-unknown-unknown', '--release', '--offline', '--locked'],
  {cwd:workspace, env:{...process.env, CARGO_TARGET_DIR:target}, stdio:'inherit'});
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
const wasm = resolve(runtime, 'wasm');
mkdirSync(wasm, {recursive:true});
for (const [source, output] of [['fullmeta_core.wasm','fullmeta-core.wasm'], ['fullmeta_model.wasm','fullmeta-model.wasm']])
  copyFileSync(resolve(target, 'wasm32-unknown-unknown/release', source), resolve(wasm, output));
function bridge(crate, changes, output) {
  let text = readFileSync(resolve(workspace, 'platform', crate, 'js/api.js'), 'utf8');
  for (const [before, after] of changes) {
    if (!text.includes(before)) throw new Error(`Expected ${before} in ${crate} browser bridge.`);
    text = text.replace(before, after);
  }
  writeFileSync(resolve(runtime, output), text);
}
bridge('core-rust', [['../wasm/fullmeta-core.wasm', './wasm/fullmeta-core.wasm']], 'core-api.js');
bridge('model-rust', [
  ['../../js/document-state.js', '../../../../platform/js/document-state.js'],
  ["documentValue('model.validation'", "documentValue('fem.model.validation'"],
  ['../wasm/fullmeta-model.wasm', './wasm/fullmeta-model.wasm'],
], 'model-api.js');
console.log(`Built FEM browser modules in ${wasm}`);
