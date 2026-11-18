import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { getCoreUnit, resolveParameterTable } from '../../js/runtime/core-api.js';
import * as femModel from '../../js/runtime/model-api.js';
import * as platformModel from '../../../../platform/model-rust/js/api.js';

test('FEM-owned browser code imports FEM-local WASM bridges', () => {
  const app = fileURLToPath(new URL('../../', import.meta.url));
  const files = [join(app, 'package.js'), join(app, 'schema-documents.js')];
  function collect(dir) {
    for (const entry of readdirSync(dir, {withFileTypes:true})) {
      const path = join(dir, entry.name);
      if (entry.isDirectory() && entry.name !== 'runtime') collect(path);
      else if (entry.isFile() && entry.name.endsWith('.js')) files.push(path);
    }
  }
  collect(join(app, 'js'));
  collect(join(app, 'ui'));
  for (const file of files) assert.doesNotMatch(readFileSync(file, 'utf8'),
    /platform\/(?:core-rust|model-rust)\/js\/api\.js/, `${file} bypasses the FEM-local bridge`);
});

test('FEM-local core evaluates the declared source units', () => {
  assert.deepEqual(getCoreUnit('W/m^3').dimension, [-1, 1, -3, 0, 0]);
  assert.deepEqual(getCoreUnit('N/m^3').dimension, [-2, 1, -2, 0, 0]);
  const result = resolveParameterTable([{id:'force',name:'Force',symbol:'F0',expression:'2',unit:'N/m^3'}], [], [], 'parameters');
  assert.equal(result.F0.value, 2);
});

test('FEM and shared model WASM retain independent schema handles', () => {
  const femSchema = {$id:'https://example.test/fem.schema.json',type:'object',required:['femValue'],properties:{femValue:{type:'number'}}};
  const platformSchema = {$id:'https://example.test/platform.schema.json',type:'object',required:['platformValue'],properties:{platformValue:{type:'string'}}};
  femModel.configureModelSchema(femSchema);
  platformModel.configureModelSchema(platformSchema);
  assert.deepEqual(femModel.validateModelSchema({femValue:1}), []);
  assert.ok(femModel.validateModelSchema({platformValue:'x'}).length);
  assert.deepEqual(platformModel.validateModelSchema({platformValue:'x'}), []);
  assert.ok(platformModel.validateModelSchema({femValue:1}).length);
});

