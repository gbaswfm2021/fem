const owners = new WeakMap();
let nextOwner = 0;
function supportedConditionTypes(profile) {
  return new Set([
    ...(Array.isArray(profile?.supportedBoundaryTypes) ? profile.supportedBoundaryTypes : []),
    ...(Array.isArray(profile?.supportedFeatureTypes) ? profile.supportedFeatureTypes : []),
  ]);
}

/** Build the immutable Physics action route once when the application package loads. */
export function conditionSpecsFromMenu(entries) {
  if (!Array.isArray(entries)) throw new Error('The FEM Physics action menu is malformed.');
  const specs = new Map();
  for (const entry of entries) {
    if (!entry?.conditionType && !entry?.itemName) continue;
    if (typeof entry.action !== 'string' || !entry.action.trim()
      || typeof entry.conditionType !== 'string' || !entry.conditionType.trim()
      || typeof entry.itemName !== 'string' || !entry.itemName.trim()) {
      throw new Error('The FEM Physics condition action is incomplete.');
    }
    const previous = specs.get(entry.action);
    if (previous && (previous.conditionType !== entry.conditionType
      || previous.itemName !== entry.itemName)) {
      throw new Error(`FEM Physics action ${entry.action} targets multiple condition types.`);
    }
    if (previous && JSON.stringify(previous) !== JSON.stringify(entry)) {
      throw new Error(`FEM Physics action ${entry.action} has conflicting declarations.`);
    }
    specs.set(entry.action, entry);
  }
  return specs;
}

function ownerId(model) {
  if (!model || typeof model !== 'object') return null;
  if (!owners.has(model)) owners.set(model, `fem-draft-${++nextOwner}`);
  return owners.get(model);
}

export function guardPhysicsCondition(model, target, profile) {
  const path = target?.itemPath;
  if (!Array.isArray(path) || path.length !== 3 || path[0] !== 'model' || path[1] !== 'physics' || typeof path[2] !== 'string') return null;
  const physics = model?.model?.physics;
  const item = physics && Object.hasOwn(physics, path[2]) ? physics[path[2]] : null;
  if (!item || typeof item !== 'object' || !supportedConditionTypes(profile).has(item.type) || typeof item.id !== 'string') return null;
  return {
    owner: ownerId(model), id: item.id, profileId: model?.scenario?.profileId, type: item.type,
    handling: profile?.featureHandlingByType?.[item.type],
  };
}

export function conditionToggleAvailability(model, target, profile) {
  const actual = guardPhysicsCondition(model, target, profile);
  if (!actual || !target?.femConditionGuard || actual.owner !== target.femConditionGuard.owner
    || actual.id !== target.femConditionGuard.id || actual.type !== target.femConditionGuard.type
    || actual.profileId !== target.femConditionGuard.profileId
    || actual.handling !== target.femConditionGuard.handling) {
    return { disabled: true, reason: 'The Physics condition or its document changed; reopen its menu.' };
  }
  return { disabled: false, reason: '' };
}

export function togglePhysicsCondition(model, target, profile) {
  const availability = conditionToggleAvailability(model, target, profile);
  if (availability.disabled) throw new Error(availability.reason);
  const path = target.itemPath, item = model.model.physics[path[2]], document = structuredClone(model);
  document.model.physics[path[2]].enabled = item.enabled === false;
  const label = item.name || item.id;
  return { document, path: [...path], label,
    message: `${document.model.physics[path[2]].enabled ? 'Enabled' : 'Disabled'} ${label}. Save to keep the change.` };
}

export function guardPhysicsRoot(model, target) {
  const path = target?.itemPath, physics = model?.model?.physics;
  if (!Array.isArray(path) || path.length !== 2 || path[0] !== 'model' || path[1] !== 'physics'
      || !physics || typeof physics !== 'object' || typeof physics.id !== 'string') return null;
  return { owner: ownerId(model), id: physics.id };
}

export function preparePhysicsCondition({ model, target, spec, profile, makeId }) {
  const guard = guardPhysicsRoot(model, target);
  if (!guard || guard.owner !== target?.femPhysicsGuard?.owner || guard.id !== target?.femPhysicsGuard?.id)
    return { reason: 'The Physics interface or its document changed; reopen its menu.' };
  const type = spec?.conditionType, name = spec?.itemName;
  if (typeof type !== 'string' || !type || typeof name !== 'string' || !name || typeof makeId !== 'function')
    return { reason: 'The condition action is not fully declared.' };
  if (!supportedConditionTypes(profile).has(type)) return { reason: `${name} is not supported by the active profile.` };
  const allowed = profile?.supportedFeatureTypes?.includes(type) ? profile?.supportedFeatureTypes : profile?.supportedBoundaryTypes;
  if (!Array.isArray(allowed) || !allowed.includes(type)) return { reason: `${name} is not supported by the active profile.` };
  const physics = model.model.physics;
  const selections = Array.isArray(model.model.selections) ? model.model.selections : [];
  const pairs = Array.isArray(model.model.mesh?.periodicPairs) ? model.model.mesh.periodicPairs : [];
  const conditions = Object.values(physics).filter(item => item && typeof item === 'object'
    && supportedConditionTypes(profile).has(item.type) && item.enabled !== false);
  let condition;
  if (profile?.featureHandlingByType?.[type] === 'periodicPair') {
    const used = new Set(conditions.filter(item => profile?.featureHandlingByType?.[item.type] === 'periodicPair').map(item => item.periodicPairId));
    const assigned = new Set(conditions.filter(item => !['periodicPair', 'domainSource'].includes(profile?.featureHandlingByType?.[item.type])).map(item => item.selectionId));
    const enabled = new Set(Array.isArray(model.model.mesh?.periodicPairIds) ? model.model.mesh.periodicPairIds : []);
    const boundaries = new Set(selections.filter(item => item?.entity === 'boundary').map(item => item.id));
    const pair = pairs.find(item => enabled.has(item?.id) && !used.has(item.id)
      && typeof item.id === 'string' && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(item.id)
      && item.lowerSelectionId !== item.upperSelectionId && boundaries.has(item.lowerSelectionId) && boundaries.has(item.upperSelectionId)
      && !assigned.has(item.lowerSelectionId) && !assigned.has(item.upperSelectionId));
    if (!pair) return { reason: 'Enable an unused periodic pair with available boundary selections in Mesh before adding this condition.' };
    condition = { type, periodicPairId: pair.id };
  } else if (profile?.featureHandlingByType?.[type] === 'domainSource') {
    const selection = selections.find(item => item?.entity === 'domain' && typeof item.id === 'string'
      && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(item.id));
    if (!selection) return { reason: 'Add a domain selection before adding this source.' };
    const defaults = profile?.boundaryDefaultsByFormulation?.[physics.formulation]?.[type];
    if (!defaults || typeof defaults !== 'object') return { reason: `${name} has no defaults for the active formulation.` };
    condition = { type, selectionId: selection.id, ...structuredClone(defaults) };
  } else {
    const used = new Set(conditions.filter(item => !['periodicPair', 'domainSource'].includes(profile?.featureHandlingByType?.[item.type])).map(item => item.selectionId));
    const activePairs = new Set(conditions.filter(item => profile?.featureHandlingByType?.[item.type] === 'periodicPair').map(item => item.periodicPairId));
    for (const pair of pairs) if (pair && activePairs.has(pair.id)) { used.add(pair.lowerSelectionId); used.add(pair.upperSelectionId); }
    const selection = selections.find(item => item?.entity === 'boundary' && typeof item.id === 'string'
      && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(item.id) && !used.has(item.id));
    if (!selection) return { reason: 'Add an unused boundary selection before adding this condition.' };
    const defaults = profile?.boundaryDefaultsByFormulation?.[physics.formulation]?.[type];
    if (!defaults || typeof defaults !== 'object') return { reason: `${name} has no defaults for the active formulation.` };
    condition = { type, selectionId: selection.id, ...structuredClone(defaults) };
  }
  const names = new Set(Object.values(physics).filter(item => item && typeof item === 'object').map(item => item.name));
  let uniqueName = name, suffix = 2;
  while (names.has(uniqueName)) uniqueName = `${name} ${suffix++}`;
  let id = makeId(type), index = 2;
  while (Object.hasOwn(physics, id) && index < 1000) id = makeId(`${type}_${index++}`);
  if (typeof id !== 'string' || !/^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(id) || Object.hasOwn(physics, id))
    return { reason: 'Could not allocate a unique condition ID.' };
  return { condition: { id, name: uniqueName, ...condition } };
}

export function addPhysicsCondition(options) {
  const prepared = preparePhysicsCondition(options);
  if (!prepared.condition) throw new Error(prepared.reason);
  const document = structuredClone(options.model), condition = prepared.condition;
  document.model.physics[condition.id] = condition;
  return { document, path: ['model', 'physics', condition.id], label: condition.name,
    message: `Added ${condition.name}. Save to keep the change.` };
}

export function duplicatePhysicsCondition(model, target, makeId, profile) {
  const availability = conditionToggleAvailability(model, target, profile);
  if (availability.disabled) throw new Error(availability.reason);
  const original = model.model.physics[target.itemPath[2]];
  if (target?.femConditionGuard?.handling === 'periodicPair') throw new Error('Periodic conditions cannot be duplicated.');
  if (typeof makeId !== 'function') throw new Error('A unique ID generator is required.');
  const physics = model.model.physics, names = new Set(Object.values(physics)
    .filter(item => item && typeof item === 'object').map(item => item.name));
  const baseName = original.name || original.id;
  let name = `${baseName} 2`, suffix = 3;
  while (names.has(name)) name = `${baseName} ${suffix++}`;
  let id = makeId(original.id), index = 2;
  while (Object.hasOwn(physics, id) && index < 1000) id = makeId(`${original.id}_${index++}`);
  if (typeof id !== 'string' || !/^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(id) || Object.hasOwn(physics, id))
    throw new Error('Could not allocate a unique condition ID.');
  const document = structuredClone(model);
  document.model.physics[id] = { ...structuredClone(original), id, name };
  return { document, path: ['model', 'physics', id], label: name,
    message: `Duplicated ${baseName}. Save to keep the change.` };
}

export function prepareConditionDeletion({ model, target, profile, validate, maximumErrors }) {
  const guard = conditionToggleAvailability(model, target, profile);
  if (guard.disabled) return { reason: guard.reason };
  if (typeof validate !== 'function' || !Number.isInteger(maximumErrors) || maximumErrors < 1)
    return { reason: 'The model reference checks are unavailable; the condition cannot be deleted.' };
  const item = model.model.physics[target.itemPath[2]], document = structuredClone(model);
  delete document.model.physics[target.itemPath[2]];
  try {
    const prior = validate(model), after = validate(document);
    if (!Array.isArray(prior) || !Array.isArray(after) || prior.length >= maximumErrors || after.length >= maximumErrors)
      return { reason: 'Resolve model validation errors before checking condition references.' };
    const counts = new Map();
    for (const error of prior) counts.set(error, (counts.get(error) || 0) + 1);
    const introduced = after.find(error => {
      const remaining = counts.get(error) || 0;
      if (!remaining) return true;
      counts.set(error, remaining - 1);
      return false;
    });
    if (introduced) return { reason: `Cannot delete ${item.name || item.id}: ${introduced}` };
  } catch (error) {
    return { reason: `Cannot check condition references: ${error.message}` };
  }
  return { document, label: item.name || item.id };
}

export function deletePhysicsCondition(options) {
  const prepared = prepareConditionDeletion(options);
  if (!prepared.document) throw new Error(prepared.reason);
  return { document: prepared.document, path: null, label: '',
    message: `Removed ${prepared.label}. Save to keep the change.` };
}

export function guardArrayItem(model, target) {
  const path = target?.itemPath;
  if (!Array.isArray(path) || !Number.isInteger(path.at(-1)) || path.at(-1) < 0) return null;
  const arrayPath = path.slice(0, -1), collection = arrayPath.reduce((value, part) => value?.[part], model);
  const item = Array.isArray(collection) ? collection[path.at(-1)] : null;
  if (!item || typeof item !== 'object' || typeof item.id !== 'string') return null;
  return { owner: ownerId(model), id: item.id };
}

export function duplicateArrayAvailability(model, target, cloneItem) {
  const actual = guardArrayItem(model, target), captured = target?.femArrayGuard;
  if (!actual || !captured || actual.owner !== captured.owner || actual.id !== captured.id)
    return { disabled: true, reason: 'The item or its document changed; reopen its menu.' };
  if (typeof cloneItem !== 'function') return { disabled: true, reason: 'The item duplication service is unavailable.' };
  return { disabled: false, reason: '' };
}

export function duplicateArrayItem({ model, target, cloneItem, insertItem }) {
  const available = duplicateArrayAvailability(model, target, cloneItem);
  if (available.disabled) throw new Error(available.reason);
  if (typeof insertItem !== 'function') throw new Error('The item insertion service is unavailable.');
  const path = target.itemPath, parent = path.slice(0, -1), item = parent.reduce((value, part) => value[part], model)[path.at(-1)];
  const copy = cloneItem(item, parent), index = path.at(-1) + 1;
  if (!copy || typeof copy.id !== 'string' || copy.id === item.id) throw new Error('Duplication must create a new stable ID.');
  const document = insertItem(model, parent, copy, index);
  return { document, path: [...parent, index], label: copy.name || copy.id,
    message: `Duplicated ${item.name || item.id}. Save to keep the change.` };
}
