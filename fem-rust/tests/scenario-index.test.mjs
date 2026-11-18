import assert from 'node:assert/strict';
import {mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {isAbsolute, join, relative, sep} from 'node:path';
import {test} from 'node:test';
import {fileURLToPath} from 'node:url';
import {generateScenarioIndex, scenarioIndexText} from '../prebuilt-tools/scenario-index.mjs';

function fixture(t) {
  const base = realpathSync(tmpdir());
  const directory = mkdtempSync(join(base, 'fullmeta-index-'));
  t.after(() => {
    const resolved = realpathSync(directory);
    const inside = relative(base, resolved);
    assert.ok(inside && !isAbsolute(inside) && !inside.startsWith(`..${sep}`) && inside !== '..');
    rmSync(resolved, {recursive: true});
  });
  return directory;
}

function writePackage(dir, id, discovery) {
  writeFileSync(join(dir, `${id.replaceAll('-', '')}.json`), JSON.stringify({
    contractVersion: 'fullmeta-fem-scenario/1', id, discovery,
  }));
}

test('generates a deterministic registry and detects a new JSON package', t => {
  const dir = fixture(t);
  writePackage(dir, 'wave-3d', {order: 10, default: true});
  writePackage(dir, 'flux-2d', {order: 2});
  assert.throws(() => generateScenarioIndex(dir, {check: true}), /missing or stale/);
  assert.equal(generateScenarioIndex(dir), true);
  const manifest = JSON.parse(readFileSync(join(dir, 'registry.json'), 'utf8'));
  assert.deepEqual(manifest.packages.map(entry => entry.id), ['flux-2d', 'wave-3d']);
  assert.equal(manifest.packages[1].default, true);
  assert.equal(generateScenarioIndex(dir), false);
  writePackage(dir, 'new-physics', {});
  assert.throws(() => generateScenarioIndex(dir, {check: true}), /missing or stale/);
  assert.equal(generateScenarioIndex(dir), true);
  assert.deepEqual(JSON.parse(readFileSync(join(dir, 'registry.json'), 'utf8')).packages.map(entry => entry.id),
    ['flux-2d', 'wave-3d', 'new-physics']);
  assert.equal(generateScenarioIndex(dir, {check: true}), false);
});

test('rejects invalid identity, order, and ambiguous defaults', t => {
  const dir = fixture(t);
  writePackage(dir, 'first', {order: 1, default: true});
  writePackage(dir, 'second', {order: 1});
  assert.throws(() => scenarioIndexText(dir), /duplicate discovery.order/);
  writePackage(dir, 'second', {order: 2, default: true});
  assert.throws(() => scenarioIndexText(dir), /Exactly one/);
  writePackage(dir, 'second', {order: 2});
  writeFileSync(join(dir, 'bad.json'), JSON.stringify({contractVersion: 'fullmeta-fem-scenario/1', id: '../bad'}));
  assert.throws(() => scenarioIndexText(dir), /safe scenario id/);
});

test('indexes a JSON-only generic capability and rejects unsupported or ambiguous execution', t => {
  const dir = fixture(t);
  writePackage(dir, 'default', {default: true});
  const filename = join(dir, 'newscalar.json');
  const pkg = {contractVersion: 'fullmeta-fem-scenario/1', id: 'new-scalar',
    schemaDocument: {'x-fem-scenario': {execution: {kind: 'scalarSimplexP1'}}}};
  writeFileSync(filename, JSON.stringify(pkg));
  assert.equal(generateScenarioIndex(dir), true);
  assert.ok(JSON.parse(readFileSync(join(dir, 'registry.json'), 'utf8')).packages.some(row => row.id === 'new-scalar'));
  pkg.schemaDocument['x-fem-scenario'].execution.kind = 'vectorSimplexP1';
  writeFileSync(filename, JSON.stringify(pkg));
  assert.doesNotThrow(() => scenarioIndexText(dir));
  pkg.schemaDocument['x-fem-scenario'].execution.kind = 'orientedTetrahedronNedelec';
  writeFileSync(filename, JSON.stringify(pkg));
  assert.doesNotThrow(() => scenarioIndexText(dir));
  pkg.schemaDocument['x-fem-scenario'].execution.kind = 'unimplemented';
  writeFileSync(filename, JSON.stringify(pkg));
  assert.throws(() => scenarioIndexText(dir), /unsupported execution kind/);
  pkg.schemaDocument['x-fem-scenario'].execution.kind = 'scalarSimplexP1';
  pkg.schemaDocument['x-fem-scenario'].solverModule = 'newscalar';
  writeFileSync(filename, JSON.stringify(pkg));
  assert.throws(() => scenarioIndexText(dir), /mutually exclusive/);
});

test('existing packages generate the editor and native manifest without changing it', () => {
  const directory = fileURLToPath(new URL('../src/scenarios/', import.meta.url));
  const expected = JSON.parse(scenarioIndexText(directory));
  const actual = JSON.parse(readFileSync(join(directory, 'registry.json'), 'utf8'));
  assert.deepEqual(expected, actual);
  assert.ok(expected.packages.length > 0);
  assert.equal(expected.packages.filter(entry => entry.default).length, 1);
});
