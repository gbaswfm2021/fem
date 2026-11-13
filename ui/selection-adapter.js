const entityKinds = new Set(['domain', 'boundary', 'edge', 'point']);
const domainFeatureTypes = new Set(['rectangle','circle','box','cylinder','union','intersection']);

const featureId = feature => typeof feature?.id === 'string' ? feature.id : null;

function featureIndex(geometryFeatures) {
  const byId = new Map();
  const duplicates = new Set();
  const visit = features => {
    if (!Array.isArray(features)) return;
    for (const feature of features) {
      const id = featureId(feature);
      if (id) {
        if (byId.has(id)) duplicates.add(id);
        else byId.set(id, feature);
      }
      visit(feature?.features);
    }
  };
  visit(geometryFeatures);
  return { byId, duplicates };
}

function membershipEntries(address) {
  const membership = address?.membership;
  if (!membership || typeof membership !== 'object' || Array.isArray(membership)) return [];
  return Object.entries(membership);
}

function selectedFor(entity, value) {
  if (entity === 'domain') return value === 'inside';
  return ['boundary', 'edge', 'point'].includes(entity) && value === 'boundary';
}

/** Project serialized FEM addresses without expanding sparse membership or changing their order. */
export function decodeSelection(selection, geometryFeatures) {
  const entity = selection?.entity;
  const addresses = Array.isArray(selection?.addresses) ? selection.addresses : [];
  const { byId, duplicates } = featureIndex(geometryFeatures);
  const diagnostics = [];
  if (!entityKinds.has(entity)) diagnostics.push({ code: 'invalid-entity', message: 'Selection entity is missing or unsupported.' });

  const addressRows = addresses.map((address, addressIndex) => {
    if (!address?.membership || typeof address.membership !== 'object' || Array.isArray(address.membership)) {
      diagnostics.push({ code: 'invalid-membership-object', addressIndex,
        message: `Address ${addressIndex + 1} must contain a membership object.` });
    }
    const membership = Object.fromEntries(membershipEntries(address));
    const entityIds = [];
    for (const [id, value] of Object.entries(membership)) {
      if (!['outside', 'inside', 'boundary'].includes(value)) {
        diagnostics.push({ code: 'invalid-membership', addressIndex, featureId: id,
          message: `Address ${addressIndex + 1} has an unsupported membership value for “${id}”.` });
        continue;
      }
      if (entity === 'domain' && value === 'boundary') {
        diagnostics.push({ code: 'membership-entity-mismatch', addressIndex, featureId: id,
          message: `Address ${addressIndex + 1} uses a membership value incompatible with the ${entity} entity.` });
        continue;
      }
      if (!byId.has(id)) {
        diagnostics.push({ code: 'missing-feature', addressIndex, featureId: id,
          message: `Address ${addressIndex + 1} refers to missing geometry feature “${id}”.` });
        continue;
      }
      if (duplicates.has(id)) {
        diagnostics.push({ code: 'duplicate-feature-id', addressIndex, featureId: id,
          message: `Geometry feature ID “${id}” is ambiguous.` });
        continue;
      }
      if(!domainFeatureTypes.has(byId.get(id).type)){
        diagnostics.push({code:'ineligible-feature',addressIndex,featureId:id,message:`Address ${addressIndex+1} refers to construction geometry “${id}” instead of a domain feature.`});
        continue;
      }
      if (selectedFor(entity, value)) entityIds.push(id);
    }
    if(entity!=='domain'&&entityKinds.has(entity)&&!Object.values(membership).includes('boundary'))diagnostics.push({code:'missing-boundary-membership',addressIndex,message:`Address ${addressIndex+1} needs at least one On boundary membership.`});
    return { addressIndex, membership, entityIds };
  });

  const entityIds = [...new Set(addressRows.flatMap(row => row.entityIds))];
  const expression = selection?.coordinateExpression;
  return {
    dimension: entity,
    entityIds,
    addressRows,
    coordinateFilter: { expression: typeof expression === 'string' ? expression : '0', explicit: typeof expression === 'string' },
    diagnostics,
  };
}

function sceneTargetIndex(scene) {
  const source = scene?.selectionTargets;
  const index = new Map();
  const add = (id, value) => {
    if (typeof id !== 'string') return;
    const values = Array.isArray(value) ? value : [value];
    const targets = values.map(target => typeof target === 'string' ? target : target?.targetId ?? target?.id)
      .filter(target => typeof target === 'string' && target.length);
    if (targets.length) index.set(id, targets);
  };
  if (source instanceof Map) {
    for (const [id, target] of source) add(id, target);
    return index;
  }
  if (Array.isArray(source)) {
    for (const target of source) {
      if (typeof target?.featureId === 'string') add(target.featureId, target.targetIds ?? target.targetId ?? target.id);
    }
  } else if (source && typeof source === 'object') {
    for (const [id, target] of Object.entries(source)) add(id, target);
  }
  return index;
}

/** Resolve selection IDs only through the scene's stable feature-to-target mapping. */
export function selectionTargets({ selection, selectedAddress, geometryFeatures, scene } = {}) {
  const decoded = decodeSelection(selection, geometryFeatures);
  let ids = decoded.entityIds;
  if (selectedAddress !== undefined && selectedAddress !== null) {
    if (Number.isSafeInteger(selectedAddress)) ids = decoded.addressRows[selectedAddress]?.entityIds || [];
    else if (typeof selectedAddress === 'object') {
      const wanted = selectedAddress.membership || selectedAddress;
      const row = decoded.addressRows.find(item => {
        const keys = Object.keys(wanted);
        return keys.length === Object.keys(item.membership).length
          && keys.every(key => Object.hasOwn(item.membership, key) && item.membership[key] === wanted[key]);
      });
      ids = row?.entityIds || [];
    } else ids = [];
  }

  const targets = sceneTargetIndex(scene);
  const targetIds = [];
  for (const id of ids) {
    for (const target of targets.get(id) || []) {
      if (!targetIds.includes(target)) targetIds.push(target);
    }
  }
  return targetIds;
}

export const selectionAdapter = Object.freeze({ decodeSelection, selectionTargets });
