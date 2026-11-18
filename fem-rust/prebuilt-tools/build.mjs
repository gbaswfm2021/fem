// Build FEM against the public DLL SDKs; no prebuilt Rust core crate is used.
import {copyFileSync, existsSync, mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {bundle, contractsLib, modelLib, root, run, target, toolchain} from './common.mjs';
import {generateScenarioIndex} from './scenario-index.mjs';

const args = new Set(process.argv.slice(2));
for (const arg of args) if (!['--offline', '--stage'].includes(arg)) throw new Error(`Unknown option: ${arg}`);
const libraries = [
  [resolve(bundle, 'fullmeta_core.dll'), 'fullmeta_core.dll'],
  [resolve(modelLib, 'fullmeta_model.dll'), 'fullmeta_model.dll'],
  [resolve(contractsLib, 'fullmeta_operation_contracts.dll'), 'fullmeta_operation_contracts.dll'],
];
for (const [source] of libraries) if (!existsSync(source)) throw new Error(`Missing platform DLL: ${source}`);
generateScenarioIndex(resolve(root, 'src/scenarios'));
const output = resolve(root, 'target', 'dll-build');
run(process.env.CARGO || 'cargo', [`+${toolchain}`, 'build', ...(args.has('--offline') ? ['--offline'] : []),
  '--locked', '--release', '--target', target, '--manifest-path', resolve(root, 'Cargo.toml'), '--bin', 'fullmeta-fem'],
  {cwd: root, env: {...process.env, CARGO_TARGET_DIR: output}, stdio: 'inherit'});
const executable = resolve(output, target, 'release', 'fullmeta-fem.exe');
const buildDir = resolve(output, target, 'release');
for (const [source, name] of libraries) copyFileSync(source, resolve(buildDir, name));
console.log(`Built ${executable}`);
if (args.has('--stage')) {
  const destination = resolve(root, '..', '.runtime', target, 'release');
  mkdirSync(destination, {recursive: true});
  copyFileSync(executable, resolve(destination, 'fullmeta-fem.exe'));
  for (const [source, name] of libraries) copyFileSync(source, resolve(destination, name));
  console.log(`Staged ${resolve(destination, 'fullmeta-fem.exe')} and three platform DLLs`);
}
