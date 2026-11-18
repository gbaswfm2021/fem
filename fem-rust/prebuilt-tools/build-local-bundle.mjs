// Build a reviewable FEM-owned native bundle without changing platform DLLs or
// the active FEM runtime. Invoke build-wasm.mjs separately for browser WASM.
import { copyFileSync, mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { root, run, target, toolchain } from './common.mjs';
import { generateScenarioIndex } from './scenario-index.mjs';

const args = new Set(process.argv.slice(2));
for (const arg of args) if (!['--offline', '--stage'].includes(arg)) throw new Error(`Unknown option: ${arg}`);
const workspace = resolve(root, '../../..');
const targetDir = resolve(root, 'target/fem-local-native');
const output = resolve(root, 'target/fem-local-bundle', target, 'release');
const cargo = process.env.CARGO || 'cargo';
const env = { ...process.env, CARGO_TARGET_DIR: targetDir };
generateScenarioIndex(resolve(root, 'src/scenarios'));
for (const manifest of [
  'platform/core-rust/Cargo.toml',
  'platform/model-rust/Cargo.toml',
  'platform/operation-contracts-rust/Cargo.toml',
  'applications/fem/fem-rust/Cargo.toml',
]) {
  const isFem = manifest.startsWith('applications/');
  run(cargo, [`+${toolchain}`, 'build', '--manifest-path', resolve(workspace, manifest),
    '--locked', '--release', '--target', target, ...(args.has('--offline') ? ['--offline'] : []),
    ...(isFem ? ['--bin', 'fullmeta-fem'] : ['--lib'])], { cwd: workspace, env, stdio: 'inherit' });
}
const files = ['fullmeta-fem.exe', 'fullmeta_core.dll', 'fullmeta_model.dll', 'fullmeta_operation_contracts.dll'];
mkdirSync(output, { recursive: true });
for (const name of files)
  copyFileSync(resolve(targetDir, target, 'release', name), resolve(output, name));
console.log(`Built FEM-local candidate bundle: ${output}`);
if (args.has('--stage')) {
  const destination = resolve(root, '..', '.runtime', target, 'release');
  mkdirSync(destination, { recursive: true });
  const backup = mkdtempSync(resolve(root, 'target/runtime-before-local-bundle-'));
  const existing = [];
  for (const name of files) {
    const source = resolve(destination, name);
    try { copyFileSync(source, resolve(backup, name)); existing.push(name); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  if (existing.length !== 0 && existing.length !== files.length)
    throw new Error(`The active FEM runtime has an incomplete bundle; its existing files were copied to ${backup}.`);
  try {
    for (const name of files) copyFileSync(resolve(output, name), resolve(destination, name));
  } catch (error) {
    for (const name of existing) copyFileSync(resolve(backup, name), resolve(destination, name));
    if (!existing.length) for (const name of files) rmSync(resolve(destination, name), { force: true });
    throw error;
  }
  console.log(`Staged the FEM-local bundle in ${destination}; previous files: ${backup}`);
}
