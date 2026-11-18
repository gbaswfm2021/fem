// Generate the manifest consumed by both FEM schema loaders. This is an
// index of packages, not a numerical solver or a substitute for validation.
import {lstatSync, readFileSync, readdirSync, renameSync, unlinkSync, writeFileSync} from 'node:fs';
import {basename, join, resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {root} from './common.mjs';

const format = 'fullmeta-fem-scenario-registry/1';
const contract = 'fullmeta-fem-scenario/1';
const safeId = /^[a-z][a-z0-9-]{0,63}$/;

function packageEntries(directory) {
  const entries = [];
  const ids = new Set();
  const orders = new Set();
  const names = readdirSync(directory).filter(name => name.endsWith('.json') && name !== 'registry.json');
  for (const name of names) {
    const path = join(directory, name);
    const stat = lstatSync(path);
    if (!stat.isFile() || stat.isSymbolicLink()) throw new Error(`${name}: scenario package must be a regular file, not a link.`);
    let pkg;
    try { pkg = JSON.parse(readFileSync(path, 'utf8')); }
    catch (error) { throw new Error(`${name}: invalid JSON: ${error.message}`); }
    if (!pkg || typeof pkg !== 'object' || Array.isArray(pkg) || pkg.contractVersion !== contract) {
      throw new Error(`${name}: unsupported or missing scenario contractVersion.`);
    }
    const id = pkg.id;
    if (typeof id !== 'string' || !safeId.test(id) || name !== `${id.replaceAll('-', '')}.json`) {
      throw new Error(`${name}: expected a safe scenario id and its same-stem filename.`);
    }
    if (ids.has(id)) throw new Error(`${name}: duplicate scenario id ${id}.`);
    ids.add(id);
    const metadata = pkg.schemaDocument?.['x-fem-scenario'];
    if (metadata && Object.hasOwn(metadata, 'execution')) {
      const execution = metadata.execution;
      if (!execution || typeof execution !== 'object' || Array.isArray(execution)
          || !['scalarSimplexP1', 'vectorSimplexP1', 'orientedTetrahedronNedelec'].includes(execution.kind)) {
        throw new Error(`${name}: unsupported execution kind ${JSON.stringify(execution?.kind)}.`);
      }
      if (Object.hasOwn(metadata, 'solverModule')) {
        throw new Error(`${name}: execution and solverModule are mutually exclusive.`);
      }
    }
    const discovery = pkg.discovery ?? {};
    if (!discovery || typeof discovery !== 'object' || Array.isArray(discovery)) {
      throw new Error(`${name}: discovery must be an object.`);
    }
    if (discovery.default !== undefined && typeof discovery.default !== 'boolean') {
      throw new Error(`${name}: discovery.default must be a boolean.`);
    }
    const order = discovery.order;
    if (order !== undefined && (!Number.isSafeInteger(order) || order < 0)) {
      throw new Error(`${name}: discovery.order must be a non-negative safe integer.`);
    }
    if (order !== undefined && orders.has(order)) throw new Error(`${name}: duplicate discovery.order ${order}.`);
    if (order !== undefined) orders.add(order);
    entries.push({id, file: name, default: discovery.default === true, order});
  }
  if (!entries.length) throw new Error('At least one scenario package is required.');
  if (entries.filter(entry => entry.default).length !== 1) throw new Error('Exactly one scenario must declare discovery.default true.');
  entries.sort((a, b) => (a.order ?? Number.MAX_SAFE_INTEGER) - (b.order ?? Number.MAX_SAFE_INTEGER)
    || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  return entries.map(({id, file, default: isDefault}) => ({id, file, default: isDefault}));
}

export function scenarioIndexText(directory) {
  return `${JSON.stringify({format, packages: packageEntries(directory)}, null, 2)}\n`;
}

export function generateScenarioIndex(directory, {check = false} = {}) {
  const target = join(directory, 'registry.json');
  const text = scenarioIndexText(directory);
  let current;
  try {
    const stat = lstatSync(target);
    if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('registry.json must be a regular file, not a link.');
    current = readFileSync(target, 'utf8');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  // Ignore newline encoding when checking a checkout: both are valid manifests.
  if (current?.replaceAll('\r\n', '\n') === text) return false;
  if (check) throw new Error('Scenario registry is missing or stale. Run node prebuilt-tools/scenario-index.mjs, then retry.');
  const temporary = join(directory, `.registry.${process.pid}.${Date.now()}.tmp`);
  try {
    writeFileSync(temporary, text, {encoding: 'utf8', flag: 'wx'});
    renameSync(temporary, target);
  } catch (error) {
    try { unlinkSync(temporary); } catch (cleanup) { if (cleanup.code !== 'ENOENT') throw cleanup; }
    throw error;
  }
  return true;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const args = process.argv.slice(2);
  if (args.some(arg => arg !== '--check') || args.length > 1) throw new Error(`Usage: ${basename(process.argv[1])} [--check]`);
  const changed = generateScenarioIndex(resolve(root, 'src/scenarios'), {check: args.includes('--check')});
  console.log(args.includes('--check') ? 'Scenario registry is current.' : changed ? 'Generated scenario registry.' : 'Scenario registry is unchanged.');
}
