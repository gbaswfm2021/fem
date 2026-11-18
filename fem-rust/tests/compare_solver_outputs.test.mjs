import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

test('complex field tolerance admits roundoff but rejects material errors and shape changes', t => {
  const root = realpathSync(resolve(fileURLToPath(new URL('../target/', import.meta.url))));
  const work = mkdtempSync(join(root, 'compare-test-'));
  t.after(() => {
    const resolved = realpathSync(work);
    const child = relative(root, resolved);
    assert.ok(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`));
    rmSync(resolved, { recursive:true });
  });
  const left = join(work, 'left'), right = join(work, 'right');
  mkdirSync(left); mkdirSync(right);
  const field = (small, shape = [1,2]) => ({
    fields:{stress:{real:[800, small], imag:[0, 0]}},
    stressByCell:{real:[[800, small]], imag:[[0, 0]]},
    fluxByCell:{unit:'W/m^2', values:[[2,small / 1000]]}, shape,
  });
  const file = directory => join(directory, 'result.json');
  const compare = () => spawnSync(process.execPath,
    [fileURLToPath(new URL('./compare_solver_outputs.mjs', import.meta.url)), left, right, 'result.json'], { encoding:'utf8' });
  writeFileSync(file(left), JSON.stringify(field(0.0083)));
  writeFileSync(file(right), JSON.stringify(field(0.00830000025)));
  assert.equal(compare().status, 0);
  writeFileSync(file(right), JSON.stringify(field(0.0093)));
  assert.notEqual(compare().status, 0);
  writeFileSync(file(right), JSON.stringify(field(0.0083, [1,2,3])));
  assert.notEqual(compare().status, 0);
});
