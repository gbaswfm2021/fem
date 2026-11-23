// Shared authored thermal validation is copied into each independently
// deployable scenario package. Executable rule data is retained for native
// hosts; the browser regenerates it from the declarative obligations.
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { composeScenarioContracts } from '../../js/scenario-discovery.js';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
export function syncThermalValidation({check = true} = {}) {
  const shared = JSON.parse(readFileSync(resolve(root, 'validation/thermal-common.json'), 'utf8'));
  if (shared.version !== 'fullmeta-fem-validation-contribution/1' || !Array.isArray(shared.rows)) {
    throw new Error('Invalid shared thermal validation contribution.');
  }
  const ids = new Set();
  for (const row of shared.rows) {
    const id = row?.obligation?.id;
    if (typeof id !== 'string' || ids.has(id)) throw new Error('Invalid or duplicated shared thermal rule.');
    ids.add(id);
  }
  const sourcePackage = JSON.parse(readFileSync(resolve(root, 'src/scenarios/thermal2d.json'), 'utf8'));
  const sourceObligations = sourcePackage.schemaContributions['x-fullmeta-rules'].declarative.obligations;
  for (const row of shared.rows) {
    const index = sourceObligations.findIndex(value => value.id === row.obligation.id);
    if (index < 0) throw new Error(`thermal2d: missing shared rule ${row.obligation.id}.`);
    sourceObligations[index] = structuredClone(row.obligation);
  }
  const registry = JSON.parse(readFileSync(resolve(root, 'src/scenarios/registry.json'), 'utf8'));
  const entry = registry.packages.find(value => value.id === sourcePackage.id);
  const base = JSON.parse(readFileSync(resolve(root, '../fem.schema.json'), 'utf8'));
  const {schema} = composeScenarioContracts(base, [{registry: entry, contract: sourcePackage}]);
  const generatedProgram = schema['x-fullmeta-rules'];
  // Derived symbol bindings may compile to multiple executable rules. Match
  // each shared check by its unique diagnostic, never by an obligation index.
  const findRule = (rules, obligation, label) => {
    const message = JSON.stringify(obligation.diagnostic.message);
    const matches = rules.flatMap((value, i) => JSON.stringify(value).includes(message) ? [i] : []);
    if (matches.length !== 1) throw new Error(`${label}: expected one rule for ${obligation.id}, found ${matches.length}.`);
    return matches[0];
  };
  for (const name of ['thermal2d', 'thermal3d']) {
    const path = resolve(root, `src/scenarios/${name}.json`);
    const packageData = JSON.parse(readFileSync(path, 'utf8'));
    const program = packageData.schemaContributions['x-fullmeta-rules'];
    const obligations = program.declarative.obligations;
    let dirty = false;
    for (const row of shared.rows) {
      const rule = generatedProgram.rules[findRule(generatedProgram.rules, row.obligation, 'Generated validation')];
      const indices = obligations.flatMap((value, i) => value.id === row.obligation.id ? [i] : []);
      if (indices.length !== 1) throw new Error(`${name}: shared rule ${row.obligation.id} must occur once.`);
      const i = indices[0];
      const ruleIndex = findRule(program.rules, row.obligation, name);
      if (JSON.stringify(obligations[i]) !== JSON.stringify(row.obligation) ||
          JSON.stringify(program.rules[ruleIndex]) !== JSON.stringify(rule)) {
        dirty = true;
        obligations[i] = structuredClone(row.obligation);
        program.rules[ruleIndex] = structuredClone(rule);
      }
    }
    if (dirty && check) throw new Error(`${name}: shared thermal validation is stale; run node prebuilt-tools/sync-thermal-validation.mjs --write`);
    if (dirty) writeFileSync(path, `${JSON.stringify(packageData, null, 2)}\n`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length > 3 || (process.argv[2] && !['--check','--write'].includes(process.argv[2]))) throw new Error('Use --check or --write.');
  syncThermalValidation({check: process.argv[2] !== '--write'});
}
