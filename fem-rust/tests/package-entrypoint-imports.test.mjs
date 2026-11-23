import assert from 'node:assert/strict';
import {existsSync, readFileSync, statSync} from 'node:fs';
import {dirname, extname, isAbsolute, relative, resolve, sep} from 'node:path';
import {test} from 'node:test';
import {fileURLToPath} from 'node:url';

test('package-local JavaScript imports resolve throughout the entrypoint graph', () => {
  const packageRoot = fileURLToPath(new URL('../../', import.meta.url));
  const entrypoint = resolve(packageRoot, 'package.js');
  const importPattern = /^\s*import\s+(?:[^'";\r\n]+?\s+from\s+)?(['"])([^'"]+)\1\s*;?\s*$/gm;
  const visited = new Set();
  const missing = [];
  const isPackageLocal = target => {
    const pathFromRoot = relative(packageRoot, target);
    return pathFromRoot === '' || (pathFromRoot !== '..' && !pathFromRoot.startsWith(`..${sep}`) && !isAbsolute(pathFromRoot));
  };
  const inspect = sourcePath => {
    if (visited.has(sourcePath)) return;
    visited.add(sourcePath);
    const source = readFileSync(sourcePath, 'utf8');
    for (const match of source.matchAll(importPattern)) {
      const specifier = match[2];
      if (!specifier.startsWith('.')) continue;
      const cleanSpecifier = specifier.split(/[?#]/, 1)[0];
      const target = resolve(dirname(sourcePath), cleanSpecifier);
      if (!isPackageLocal(target)) continue;
      const candidates = extname(target)
        ? [target]
        : [target, `${target}.js`, `${target}.mjs`, `${target}.json`, resolve(target, 'index.js')];
      const resolved = candidates.find(candidate => existsSync(candidate) && statSync(candidate).isFile());
      if (!resolved) {
        missing.push(`${relative(packageRoot, sourcePath)} -> ${specifier}`);
      } else if (['.js', '.mjs'].includes(extname(resolved))) {
        inspect(resolved);
      }
    }
  };
  inspect(entrypoint);
  assert.ok(visited.size > 1, 'package.js must reach local modules');
  assert.deepEqual(missing, [], `missing package-local imports: ${missing.join(', ')}`);
});
