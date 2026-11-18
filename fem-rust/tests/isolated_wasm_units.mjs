// Check candidate browser modules without replacing the installed WASM files.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const [corePath, modelPath] = process.argv.slice(2);
if (!corePath || !modelPath) throw new Error('Supply candidate core and model WASM paths.');
const encoder = new TextEncoder(), decoder = new TextDecoder();
async function load(path, prefix) {
  const { instance } = await WebAssembly.instantiate(await readFile(resolve(path)), {});
  const wasm = instance.exports;
  return request => {
    const data = encoder.encode(JSON.stringify(request));
    const pointer = wasm[`${prefix}_alloc`](data.length) >>> 0;
    new Uint8Array(wasm.memory.buffer, pointer, data.length).set(data);
    try { wasm[`${prefix}_execute`](pointer, data.length); }
    finally { wasm[`${prefix}_dealloc`](pointer, data.length); }
    const bytes = new Uint8Array(wasm.memory.buffer, wasm[`${prefix}_output_pointer`]() >>> 0, wasm[`${prefix}_output_length`]() >>> 0).slice();
    wasm[`${prefix}_clear_output`]();
    return JSON.parse(decoder.decode(bytes));
  };
}
const core = await load(corePath, 'core'), model = await load(modelPath, 'model');
for (const [unit, dimension] of [['W/m^3',[-1,1,-3,0,0]],['N/m^3',[-2,1,-2,0,0]]]) {
  const response = core({ operation:'get_unit', unit });
  assert.equal(response.ok, true, JSON.stringify(response));
  assert.deepEqual(response.value.dimension, dimension);
  const validated = model({ operation:'validate_semantic_quantities', input:{ kind:'quantity',
    value:{ expression:'2', unit }, expectedUnit:unit, dimensionTolerance:1e-10, symbols:{}, allowedFunctions:[] } });
  assert.equal(validated.ok, true, JSON.stringify(validated));
  assert.deepEqual(validated.value.errors, []);
  assert.equal(validated.value.value, 2);
}
console.log('Candidate browser modules validate source units.');
