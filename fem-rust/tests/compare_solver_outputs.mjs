// Compare disposable solver artifacts, allowing numerical roundoff but requiring
// the same field names, array dimensions, metadata and CSV layout.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const [left, right, ...requested] = process.argv.slice(2);
if (!left || !right) throw new Error('Supply two solver output directories.');
// A field-scaled allowance handles values near zero when the same solve has
// much larger values elsewhere in that field; metadata remains pointwise.
const close = (a, b, residual = false, fieldScale = 0) => Number.isFinite(a) && Number.isFinite(b)
  && Math.abs(a-b) <= Math.max((residual ? 1e-10 : 1e-11) + 1e-8 * Math.max(Math.abs(a), Math.abs(b)), 1e-9 * fieldScale);
function compare(a, b, path, fieldScale = 0) {
  if (typeof a === 'number' && typeof b === 'number') {
    if (!close(a, b, path.endsWith('.residual'), fieldScale)) throw new Error(`${path}: ${a} differs from ${b}`);
    return;
  }
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) throw new Error(`${path}: array lengths differ`);
    a.forEach((value, i) => compare(value, b[i], `${path}[${i}]`, fieldScale));
    return;
  }
  if (a !== null && b !== null && typeof a === 'object' && typeof b === 'object') {
    const keys = Object.keys(a).sort();
    if (JSON.stringify(keys) !== JSON.stringify(Object.keys(b).sort())) throw new Error(`${path}: keys differ`);
    let dataScale = 0;
    let dataKeys = [];
    if (Array.isArray(a.real) && Array.isArray(a.imag)
        && Array.isArray(b.real) && Array.isArray(b.imag)) dataKeys = ['real', 'imag'];
    else if (typeof a.unit === 'string' && Array.isArray(a.values) && Array.isArray(b.values)) dataKeys = ['values'];
    if (dataKeys.length) {
      const pending = dataKeys.flatMap(key => [a[key], b[key]]);
      while (pending.length) {
        const value = pending.pop();
        if (Array.isArray(value)) for (const entry of value) pending.push(entry);
        else if (typeof value === 'number' && Number.isFinite(value)) dataScale = Math.max(dataScale, Math.abs(value));
      }
    }
    for (const key of keys) compare(a[key], b[key], `${path}.${key}`, dataKeys.includes(key) ? dataScale : fieldScale);
    return;
  }
  if (a !== b) throw new Error(`${path}: ${JSON.stringify(a)} differs from ${JSON.stringify(b)}`);
}
const load = (dir, name) => readFileSync(resolve(dir, name), 'utf8');
const rows = (text) => text.trim().split(/\r?\n/).map(row => row.split(','));
const names = requested.length ? requested : ['fields.json', 'summary.csv'];
for (const name of names) {
  if (name.endsWith('.json')) {
    compare(JSON.parse(load(left, name)), JSON.parse(load(right, name)), name);
  } else if (name.endsWith('.csv')) {
    const first = rows(load(left, name));
    const second = rows(load(right, name));
    if (first.length !== second.length) throw new Error(`${name}: CSV row counts differ`);
    for (let i = 0; i < first.length; i++) {
      if (first[i].length !== second[i].length) throw new Error(`${name}: CSV row ${i} has a different length`);
      for (let j = 0; j < first[i].length; j++) {
        const a = first[i][j], b = second[i][j];
        if (a !== b && !(a.trim() && b.trim() && close(Number(a), Number(b), first[0][j] === 'residual'))) throw new Error(`${name}[${i}][${j}]: ${a} differs from ${b}`);
      }
    }
  } else {
    throw new Error(`Unsupported output type: ${name}`);
  }
}
console.log(`${names.length} output files match within numerical tolerance.`);
