/** Allocate definition symbols across all definition collections and active reserved symbols. */
export function uniqueDefinitionSymbol(model, prefix, profile, capabilities) {
  if (!['p', 'v', 'u'].includes(prefix)) throw new Error('Unsupported definition symbol prefix.');
  const used = new Set(['x', 'y', 'z', 'pi', 'e']);
  for (const key of ['parameters', 'variables', 'pointwiseVariables']) {
    for (const item of Array.isArray(model?.model?.[key]) ? model.model[key] : []) if (typeof item?.symbol === 'string') used.add(item.symbol);
  }
  for (const item of Array.isArray(model?.model?.integrationOperators) ? model.model.integrationOperators : []) {
    if (typeof item?.id === 'string') used.add(item.id);
    if (typeof item?.name === 'string') used.add(item.name);
  }
  for (const key of ['modelValueFunctions', 'globalVariableFunctions']) {
    for (const name of Array.isArray(capabilities?.expressions?.[key]) ? capabilities.expressions[key] : []) used.add(name);
  }
  const fields = profile?.fields?.formulations?.[model?.model?.physics?.formulation];
  for (const key of ['fieldVariables', 'derivedFieldVariables', 'zeroFieldVariables']) {
    for (const field of Array.isArray(fields?.[key]) ? fields[key] : []) if (typeof field?.symbol === 'string') used.add(field.symbol);
  }
  for (const property of Array.isArray(profile?.materials?.properties) ? profile.materials.properties : []) if (typeof property?.symbol === 'string') used.add(property.symbol);
  for (let index = 1; index <= 100000; index++) if (!used.has(`${prefix}${index}`)) return `${prefix}${index}`;
  throw new Error(`No available ${prefix} definition symbol was found.`);
}

export function definitionTreeItem({ model, variant, makeId, profile, capabilities }) {
  if (!variant || !['parameter', 'globalVariable', 'pointwiseVariable'].includes(variant.kind)
      || !['p', 'v', 'u'].includes(variant.prefix) || typeof makeId !== 'function') return undefined;
  const symbol = uniqueDefinitionSymbol(model, variant.prefix, profile, capabilities);
  return { id: makeId(variant.kind), name: symbol, symbol,
    expression: variant.kind === 'parameter' ? '1' : '0', unit: '1', description: '' };
}
