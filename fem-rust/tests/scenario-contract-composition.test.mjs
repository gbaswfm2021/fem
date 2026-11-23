import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {test} from 'node:test';
import {fileURLToPath} from 'node:url';
import {composeScenarioContracts} from '../../js/scenario-discovery.js';

test('all registered scenario packages compose with the generic FEM schema', () => {
  const appRoot = fileURLToPath(new URL('../../', import.meta.url));
  const scenarioDirectory = join(appRoot, 'fem-rust/src/scenarios');
  const registry = JSON.parse(readFileSync(join(scenarioDirectory, 'registry.json'), 'utf8'));
  const packages = registry.packages.map(registryEntry => ({
    registry: registryEntry,
    contract: JSON.parse(readFileSync(join(scenarioDirectory, registryEntry.file), 'utf8')),
  }));
  const root = JSON.parse(readFileSync(join(appRoot, 'fem.schema.json'), 'utf8'));
  const composed = composeScenarioContracts(root, packages);
  assert.equal(composed.scenarioDocuments.length, 8);
  assert.equal(Object.keys(composed.schema.$defs.femProfiles).length, 8);
});
