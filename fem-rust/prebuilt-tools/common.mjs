import {resolve} from 'node:path';
import {spawnSync} from 'node:child_process';

export const target = 'x86_64-pc-windows-gnu';
export const toolchain = process.env.FULLMETA_RUST_TOOLCHAIN || 'stable-x86_64-pc-windows-gnu';
export const root = resolve(import.meta.dirname, '..');
export const bundle = resolve(root, '../../../platform/core-rust/lib');
export const modelLib = resolve(root, '../../../platform/model-rust/lib');
export const contractsLib = resolve(root, '../../../platform/operation-contracts-rust/lib');

export function run(command, args, options = {}) {
  const result = spawnSync(command, args, {cwd: root, encoding: 'utf8', windowsHide: true, ...options});
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const detail = result.stderr?.trim();
    throw new Error(`${command} failed with exit code ${result.status}${detail ? `: ${detail}` : ''}`);
  }
  return result.stdout;
}
