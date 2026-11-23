import assert from 'node:assert/strict';
import {existsSync, readFileSync, statSync} from 'node:fs';
import {dirname, extname, isAbsolute, relative, resolve, sep} from 'node:path';
import {test} from 'node:test';
import {fileURLToPath} from 'node:url';

test('package-local JavaScript imports resolve throughout the entrypoint graph', () => {
  const packageRoot = resolve(process.env.FEM_PACKAGE_ROOT || fileURLToPath(new URL('../../', import.meta.url)));
  const entrypoint = resolve(packageRoot, 'package.js');
  const importPattern = /^\s*import\s+(?:[^'";\r\n]+?\s+from\s+)?(['"])([^'"]+)\1\s*;?\s*$/gm;
  const namedImportPattern = /^\s*import\s*\{([^}]+)\}\s*from\s*(['"])([^'"]+)\2\s*;?\s*$/gm;
  const namedExportPattern = /^\s*export\s+(?:async\s+)?(?:function|class|const|let|var)\s+([\w$]+)/gm;
  const exportListPattern = /^\s*export\s*\{([^}]+)\}(?:\s*from\s*(['"][^'"]+['"]))?\s*;?/gm;
  const visited = new Set();
  const missing = [];
  const missingExports = [];
  const isPackageLocal = target => {
    const pathFromRoot = relative(packageRoot, target);
    return pathFromRoot === '' || (pathFromRoot !== '..' && !pathFromRoot.startsWith(`..${sep}`) && !isAbsolute(pathFromRoot));
  };
  const resolveLocalImport = (sourcePath, specifier) => {
    if (!specifier.startsWith('.')) return { packageLocal: false };
    const cleanSpecifier = specifier.split(/[?#]/, 1)[0];
    const target = resolve(dirname(sourcePath), cleanSpecifier);
    if (!isPackageLocal(target)) return { packageLocal: false };
    const candidates = extname(target)
      ? [target]
      : [target, `${target}.js`, `${target}.mjs`, `${target}.json`, resolve(target, 'index.js')];
    return {
      packageLocal: true,
      resolved: candidates.find(candidate => existsSync(candidate) && statSync(candidate).isFile()) || null,
    };
  };
  const exportedNames = source => {
    const names = new Set([...source.matchAll(namedExportPattern)].map(match => match[1]));
    for (const match of source.matchAll(exportListPattern)) {
      for (const item of match[1].split(',')) {
        const namesInItem = item.trim().split(/\s+as\s+/);
        if (namesInItem.at(-1)) names.add(namesInItem.at(-1));
      }
    }
    if (/^\s*export\s+default\b/m.test(source)) names.add('default');
    return names;
  };
  const inspect = sourcePath => {
    if (visited.has(sourcePath)) return;
    visited.add(sourcePath);
    const source = readFileSync(sourcePath, 'utf8');
    for (const match of source.matchAll(importPattern)) {
      const specifier = match[2];
      const resolution = resolveLocalImport(sourcePath, specifier);
      if (!resolution.packageLocal) continue;
      const resolved = resolution.resolved;
      if (!resolved) {
        missing.push(`${relative(packageRoot, sourcePath)} -> ${specifier}`);
      } else if (['.js', '.mjs'].includes(extname(resolved))) {
        inspect(resolved);
      }
    }
    for (const match of source.matchAll(namedImportPattern)) {
      const resolution = resolveLocalImport(sourcePath, match[3]);
      if (!resolution.packageLocal) continue;
      const resolved = resolution.resolved;
      if (!resolved || !['.js', '.mjs'].includes(extname(resolved))) continue;
      const names = exportedNames(readFileSync(resolved, 'utf8'));
      for (const item of match[1].split(',')) {
        const importedName = item.trim().split(/\s+as\s+/)[0];
        if (importedName && !names.has(importedName)) {
          missingExports.push(`${relative(packageRoot, sourcePath)} imports ${importedName} from ${relative(packageRoot, resolved)}`);
        }
      }
    }
  };
  inspect(entrypoint);
  assert.ok(visited.size > 1, 'package.js must reach local modules');
  assert.deepEqual(missing, [], `missing package-local imports: ${missing.join(', ')}`);
  assert.deepEqual(missingExports, [], `missing package-local named exports: ${missingExports.join(', ')}`);
});
