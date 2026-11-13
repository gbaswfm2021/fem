/** Construct the declared Study kind and link it to the active Physics interface. */
export function studyTreeItem({ model, destinationPath, variant, makeId, makeName }) {
  const physicsId = model?.model?.physics?.id;
  if (!variant || !['frequencyDomain', 'stationary'].includes(variant.type)
      || typeof variant.label !== 'string' || !variant.label.trim()
      || typeof physicsId !== 'string' || !/^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(physicsId)
      || typeof makeId !== 'function' || typeof makeName !== 'function') return undefined;
  return { id: makeId('study'), name: makeName(destinationPath, variant.label), type: variant.type, physicsId };
}

export function sweepTreeItem({ model, target, makeId }) {
  const path = target?.itemPath;
  const study = Array.isArray(path) ? path.reduce((value, part) => value?.[part], model) : null;
  const physics = model?.model?.physics;
  if (study?.type !== 'frequencyDomain' || study.parameterSweep !== undefined
      || study.physicsId !== physics?.id || typeof makeId !== 'function') return undefined;
  const parameters = (model.model.parameters || []).filter(parameter =>
    typeof parameter?.symbol === 'string' && /^[A-Za-z_][A-Za-z0-9_]*$/.test(parameter.symbol)
    && typeof parameter.expression === 'string' && parameter.expression.trim()
    && typeof parameter.unit === 'string' && parameter.unit.trim());
  const expression = String(physics?.frequency?.expression || '');
  const tokens = expression.matchAll(/(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?|[A-Za-z_][A-Za-z0-9_]*/g);
  const references = new Set();
  for (const token of tokens) {
    if (/^[A-Za-z_]/.test(token[0]) && !/^\s*\(/.test(expression.slice(token.index + token[0].length))) references.add(token[0]);
  }
  const selected = parameters.find(parameter => references.has(parameter.symbol))
    || parameters.find(parameter => ['Hz', 'kHz', 'MHz', 'GHz'].includes(parameter.unit)) || parameters[0];
  if (!selected) return undefined;
  return { id: makeId('parameterSweep'), name: 'Parameter sweep', type: 'parameterSweep',
    parameters: [{ parameter: selected.symbol, values: { type: 'values', values: [
      { expression: selected.expression, unit: selected.unit },
    ] } }] };
}
