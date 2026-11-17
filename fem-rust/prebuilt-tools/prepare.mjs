// Full-source developer command: publish the three Windows DLLs to platform lib/.
import {copyFileSync, mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {contractsLib, modelLib, root, run, target, toolchain} from './common.mjs';

const args = new Set(process.argv.slice(2));
for (const arg of args) if (arg !== '--offline') throw new Error(`Unknown option: ${arg}`);
run(process.env.POWERSHELL || 'powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
  '-File', resolve(root, '../../../platform/core-rust/build-native.ps1'), '-SkipSdkUpdate',
  ...(args.has('--offline') ? ['-Offline'] : [])], {cwd:root, stdio:'inherit'});
const crates = [
  ['model-rust', modelLib, 'fullmeta_model.dll'],
  ['operation-contracts-rust', contractsLib, 'fullmeta_operation_contracts.dll'],
];
for (const [crate, destination, dll] of crates) {
  const directory = resolve(root, '../../../platform', crate);
  const targetDirectory = resolve(directory, '.build/native');
  run(process.env.CARGO || 'cargo', [`+${toolchain}`, 'build', ...(args.has('--offline') ? ['--offline'] : []),
    '--locked', '--release', '--target', target, '--manifest-path', resolve(directory, 'Cargo.toml'), '--target-dir', targetDirectory, '--lib'],
    {cwd: root, stdio: 'inherit'});
  mkdirSync(destination, {recursive: true});
  copyFileSync(resolve(targetDirectory, target, 'release', dll), resolve(destination, dll));
}
copyFileSync(resolve(root, '../../../platform/model-rust/model-native.h'), resolve(modelLib, 'model-native.h'));
copyFileSync(resolve(root, '../../../platform/operation-contracts-rust/contracts-native.h'), resolve(contractsLib, 'contracts-native.h'));
run(process.execPath, [resolve(root, '../../../platform/sdk-rust/update.mjs')], {cwd:root, stdio:'inherit'});
console.log('Published the three platform DLLs.');
