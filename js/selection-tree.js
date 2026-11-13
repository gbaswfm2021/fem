/** Create an address selection using the profile-independent tree declaration. */
export function selectionTreeItem({ model, destinationPath, variant, makeId, makeName, dimension, domainFeatureTypes }) {
  if (!variant || !['domain', 'boundary', 'edge', 'point'].includes(variant.entity)
      || typeof variant.label !== 'string' || !variant.label.trim()
      || ![2, 3].includes(dimension) || variant.entity === 'edge' && dimension !== 3
      || typeof makeId !== 'function' || typeof makeName !== 'function') return undefined;
  const feature = (model?.model?.geometry?.features || []).find(item =>
    domainFeatureTypes.includes(item?.type) && typeof item.id === 'string' && item.id.length > 0);
  return { id: makeId(`${variant.entity}Selection`), name: makeName(destinationPath, variant.label),
    entity: variant.entity, addresses: [{ membership: feature ? { [feature.id]: variant.entity === 'domain' ? 'inside' : 'boundary' } : {} }] };
}

export function integrationTreeItem({ model, destinationPath, makeId, makeName, dimension }) {
  if (![2, 3].includes(dimension)) return undefined;
  const eligible = new Set(dimension === 3 ? ['domain', 'boundary', 'edge', 'point'] : ['domain', 'boundary', 'point']);
  const selection = model?.model?.selections?.find(item => eligible.has(item?.entity)
    && typeof item.id === 'string' && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(item.id));
  if (!selection || typeof makeId !== 'function' || typeof makeName !== 'function') return undefined;
  return { id: makeId('integrationOperator'), name: makeName(destinationPath, 'Integration operator'), selectionId: selection.id };
}

export function meshSizeTreeItem({ model, destinationPath, makeId, makeName, dimension }) {
  if (![2, 3].includes(dimension)) return undefined;
  const mesh = model?.model?.mesh;
  const eligible = new Set(dimension === 3 ? ['domain', 'boundary', 'edge'] : ['domain', 'boundary']);
  const used = new Set((mesh?.meshSizeControls || []).map(item => item?.selectionId));
  const selection = model?.model?.selections?.find(item => eligible.has(item?.entity) && !used.has(item.id)
    && typeof item.id === 'string' && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(item.id));
  if (!selection || !mesh?.maxSize || typeof mesh.maxSize !== 'object'
      || typeof makeId !== 'function' || typeof makeName !== 'function') return undefined;
  return { id: makeId('meshSizeControl'), name: makeName(destinationPath, 'Mesh size control'),
    selectionId: selection.id, size: structuredClone(mesh.maxSize) };
}

export function periodicPairTreeItem({ model, destinationPath, makeId, makeName }) {
  const boundaries = (model?.model?.selections || []).filter(item => item?.entity === 'boundary'
    && typeof item.id === 'string' && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(item.id));
  const lower = boundaries[0], upper = boundaries.find(item => item.id !== lower?.id);
  if (!lower || !upper || typeof makeId !== 'function' || typeof makeName !== 'function') return undefined;
  return { id: makeId('periodicPair'), name: makeName(destinationPath, 'Periodic pair'),
    lowerSelectionId: lower.id, upperSelectionId: upper.id,
    translation: Array.from({ length: 3 }, () => ({ expression: '0', unit: 'm' })) };
}
