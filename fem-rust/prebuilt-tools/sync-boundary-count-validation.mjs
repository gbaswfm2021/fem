// Generate early vector-length checks from opted-in scenario input contracts.
// The scenario's boundaryInputs remain the authority for component counts.
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
import {compileDeclarativeValidation} from '../../../../platform/ui/widgets/validation-program/declarative-rules.js';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const safe = value => value.replace(/[^A-Za-z0-9_]/g, '_');
export function syncBoundaryCountValidation({check = true, scenarioRoot = root} = {}) {
  const registry = JSON.parse(readFileSync(resolve(scenarioRoot, 'src/scenarios/registry.json'), 'utf8'));
  for (const entry of registry.packages) {
    const file = resolve(scenarioRoot, 'src/scenarios', entry.file);
    const pkg = JSON.parse(readFileSync(file, 'utf8'));
    if (pkg.validationGeneration?.boundaryVectorCounts !== true) continue;
    pkg.schemaContributions ||= {};
    let program = pkg.schemaContributions['x-fullmeta-rules'];
    let dirty = false;
    if (program === undefined) {
      program = pkg.schemaContributions['x-fullmeta-rules'] = {declarative:{obligations:[]},rules:[]};
      dirty = true;
    }
    if (!Array.isArray(program?.declarative?.obligations) || !Array.isArray(program.rules)) throw Error(`${entry.id}: invalid validation program`);
    const expectedIds = new Set();
    for (const [formulation, boundaries] of Object.entries(pkg.boundaryInputs || {})) {
      for (const [type, input] of Object.entries(boundaries)) {
        const vector = input?.prescribed?.vector;
        if (!vector) continue;
        // A fallback is selected only when the primary input is absent and
        // the fallback is an array, matching the generic boundary compiler.
        if ((input.prescribed.vectorPrecedence !== undefined && input.prescribed.vectorPrecedence !== 'vector') ||
            !/^\/[A-Za-z][A-Za-z0-9_]*$/.test(vector.path || '') ||
            (vector.fallbackPath !== undefined && !/^\/[A-Za-z][A-Za-z0-9_]*$/.test(vector.fallbackPath)) ||
            !Number.isSafeInteger(vector.components) || vector.components < 2 || vector.components > 3) {
          throw Error(`${entry.id}/${formulation}/${type}: unsupported vector count declaration`);
        }
        const legacyCounts = vector.legacyComponentCounts ?? [];
        if (!Array.isArray(legacyCounts) || legacyCounts.some(count => !Number.isSafeInteger(count) || count < 1 || count >= vector.components) ||
            new Set(legacyCounts).size !== legacyCounts.length) {
          throw Error(`${entry.id}/${formulation}/${type}: invalid legacy vector component counts`);
        }
        const allowedCounts = [...new Set([...legacyCounts, vector.components])].sort((a,b) => a-b);
        const minimumCount = allowedCounts[0];
        // The declarative validator currently expresses a contiguous count
        // interval. Reject sparse legacy sets rather than silently accepting
        // lengths the solver contract does not support.
        if (allowedCounts.some((count, index) => count !== minimumCount + index)) {
          throw Error(`${entry.id}/${formulation}/${type}: legacy vector component counts must form a contiguous interval`);
        }
        if (legacyCounts.length && (!Array.isArray(vector.missingComponentDefaults) ||
            vector.missingComponentDefaults.length !== vector.components - minimumCount)) {
          throw Error(`${entry.id}/${formulation}/${type}: legacy vector defaults do not fill the declared component counts`);
        }
        const layout = pkg.schemaDocument?.['x-fem-physics']?.boundaryFieldLayouts?.[type];
        if (layout) {
          const layoutLegacyCounts = layout.legacyComponentCounts ?? [];
          if (!Array.isArray(layout.shape) || layout.shape.length !== 1 || !Number.isSafeInteger(layout.shape[0]) ||
              !Array.isArray(layoutLegacyCounts) || layoutLegacyCounts.some(count => !Number.isSafeInteger(count) || count < 1 || count >= layout.shape[0]) ||
              (!allowedCounts.includes(layout.shape[0]) && !layoutLegacyCounts.includes(vector.components))) {
            throw Error(`${entry.id}/${formulation}/${type}: vector layout disagrees with input components`);
          }
        }
        const defaults = pkg.schemaDocument?.['x-fem-physics']?.boundaryDefaultsByFormulation?.[formulation]?.[type];
        const defaultVector = defaults?.[vector.path.slice(1)];
        if (defaultVector !== undefined && (!Array.isArray(defaultVector) || !allowedCounts.includes(defaultVector.length))) {
          throw Error(`${entry.id}/${formulation}/${type}: vector default disagrees with input components`);
        }
        const present = {type:'memberOf',subject:{literal:vector.path.slice(1)},other:{field:''}};
        const array = {type:'countAtMost',subject:{field:vector.path},other:{literal:Number.MAX_SAFE_INTEGER}};
        const specs = [
          {path:vector.path,suffix:'_type',conditions:[present],typeOnly:true},
          {path:vector.path,suffix:'',conditions:[present,array]},
        ];
        if (vector.fallbackPath) specs.push({path:vector.fallbackPath,suffix:'_fallback',conditions:[
          {type:'notMemberOf',subject:{literal:vector.path.slice(1)},other:{field:''}},
          {type:'countAtMost',subject:{field:vector.fallbackPath},other:{literal:Number.MAX_SAFE_INTEGER}}]});
        for (const spec of specs) {
        const id = `boundary_vector_count_${safe(entry.id)}_${safe(formulation)}_${safe(type)}${spec.suffix}`;
        if (expectedIds.has(id)) throw Error(`${entry.id}: duplicate generated vector-count ID ${id}`);
        expectedIds.add(id);
        const countMessage = allowedCounts.length === 1
          ? `exactly ${vector.components}`
          : `${minimumCount} to ${vector.components}`;
        const message = spec.typeOnly ? 'Boundary vector must be an array.' : `Boundary vector must contain ${countMessage} components.`;
        const obligation = {id,name:'Boundary vector component count',stage:'model',
          scope:{source:'root',path:'/model/physics/*',nested:[],when:{all:[
            {type:'equals',subject:{field:'/type'},other:{literal:type}},
            {type:'notEquals',subject:{field:'/enabled'},other:{literal:false}},
            ...spec.conditions]}},
          when:{all:[{type:'equals',subject:{source:'root',path:'/scenario/profileId'},other:{literal:entry.id}},
            {type:'equals',subject:{source:'root',path:'/model/physics/formulation'},other:{literal:formulation}}]},
          constraint:spec.typeOnly
            ? {type:'countAtMost',subject:{field:spec.path},other:{literal:Number.MAX_SAFE_INTEGER}}
            : {type:'countRange',subject:{field:spec.path},minimum:minimumCount,maximum:vector.components},
          diagnostic:{path:spec.path,message}};
        const compiled = compileDeclarativeValidation([obligation])[0];
        const obligations = program.declarative.obligations;
        const index = obligations.findIndex(row => row.id === id);
        const old = index < 0 ? undefined : obligations[index];
        const oldCompiled = old && compileDeclarativeValidation([old])[0];
        const matches = program.rules.map((row,i) => isDeepStrictEqual(row,oldCompiled || compiled) ? i : -1).filter(i => i >= 0);
        if (matches.length > 1) throw Error(`${entry.id}: duplicate compiled vector count rule`);
        if (!isDeepStrictEqual(old, obligation) || matches.length !== 1 || !isDeepStrictEqual(program.rules[matches[0]], compiled)) {
          dirty = true;
          if (index < 0) obligations.push(obligation); else obligations[index] = obligation;
          if (!matches.length) program.rules.push(compiled); else program.rules[matches[0]] = compiled;
        }
        }
      }
    }
    for (const old of [...program.declarative.obligations]) {
      if (!old.id?.startsWith(`boundary_vector_count_${safe(entry.id)}_`) || expectedIds.has(old.id)) continue;
      const compiled = compileDeclarativeValidation([old])[0];
      const matches = program.rules.map((row,i)=>isDeepStrictEqual(row,compiled)?i:-1).filter(i=>i>=0);
      if (matches.length !== 1) throw Error(`${entry.id}: stale vector-count rule has no unique executable`);
      program.rules.splice(matches[0],1);
      program.declarative.obligations.splice(program.declarative.obligations.indexOf(old),1);
      dirty = true;
    }
    if (dirty && check) throw Error(`${entry.id}: boundary count validation is stale; run node prebuilt-tools/sync-boundary-count-validation.mjs --write`);
    if (dirty) writeFileSync(file, `${JSON.stringify(pkg,null,2)}\n`);
  }
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.length > 3 || (process.argv[2] && !['--check','--write'].includes(process.argv[2]))) throw Error('Use --check or --write.');
  syncBoundaryCountValidation({check:process.argv[2] !== '--write'});
}
