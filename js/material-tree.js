/** Create a material assignment from the active profile's declared defaults. */
export function materialTreeItem({ model, destinationPath, makeId, makeName, profile }) {
  const domainIds = new Set(Array.isArray(model?.model?.physics?.domainSelectionIds) ? model.model.physics.domainSelectionIds : []);
  const assigned = new Set((Array.isArray(model?.model?.materials) ? model.model.materials : []).map(item => item?.selectionId));
  const selections = Array.isArray(model?.model?.selections) ? model.model.selections : [];
  const selection = selections.find(item => item?.entity === 'domain' && typeof item.id === 'string'
    && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(item.id) && domainIds.has(item.id) && !assigned.has(item.id));
  const properties = profile?.materials?.properties;
  if (!selection || !Array.isArray(properties) || !properties.length || typeof makeId !== 'function' || typeof makeName !== 'function') return undefined;
  const defaults = {};
  for (const property of properties) {
    if (property?.optional === true) continue;
    if (typeof property?.key !== 'string' || !property.key || !Object.hasOwn(property, 'default')) return undefined;
    defaults[property.key] = structuredClone(property.default);
  }
  return { id: makeId('material'), name: makeName(destinationPath, 'Material Assignment'), selectionId: selection.id, ...defaults };
}
