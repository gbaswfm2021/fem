const pointerEscape = part => String(part).replaceAll('~', '~0').replaceAll('/', '~1');
const pointer = path => `/${path.map(pointerEscape).join('/')}`;

function collectArray(value, path, output) {
  if (!Array.isArray(value)) return;
  value.forEach((item, index) => output.push({ id: item?.id, value: item, path: [...path, index], pointer: pointer([...path, index]) }));
}

function collectGeometry(features, path, output) {
  if (!Array.isArray(features)) return;
  features.forEach((feature, index) => {
    const itemPath = [...path, index];
    output.push({ id: feature?.id, value: feature, path: itemPath, pointer: pointer(itemPath) });
    collectGeometry(feature?.features, [...itemPath, 'features'], output);
  });
}

function buildIndex(name, records, diagnostics) {
  const byId = new Map();
  const duplicates = new Set();
  for (const record of records) {
    if (typeof record.id !== 'string' || !record.id) continue;
    if (byId.has(record.id)) {
      duplicates.add(record.id);
      diagnostics.push({ code: 'duplicate-id', collection: name, id: record.id,
        path: record.path, pointer: record.pointer, message: `Duplicate ${name} ID “${record.id}”.` });
    } else byId.set(record.id, record);
  }
  for (const id of duplicates) byId.delete(id);
  return { records, byId, duplicates };
}

/** Build non-mutating ID indexes and explicit reference edges for a FEM Model 2 document. */
export function projectModel(model) {
  const projectedDocument = structuredClone(model);
  const modelRoot = projectedDocument?.model;
  const records = {
    geometry: [], selections: [], integrationOperators: [], meshSizeControls: [], periodicPairs: [],
    materials: [], physics: [], studies: [], results: [], resultDatasets: [],
  };
  collectGeometry(modelRoot?.geometry?.features, ['model', 'geometry', 'features'], records.geometry);
  collectArray(modelRoot?.selections, ['model', 'selections'], records.selections);
  collectArray(modelRoot?.integrationOperators, ['model', 'integrationOperators'], records.integrationOperators);
  collectArray(modelRoot?.mesh?.meshSizeControls, ['model', 'mesh', 'meshSizeControls'], records.meshSizeControls);
  collectArray(modelRoot?.mesh?.periodicPairs, ['model', 'mesh', 'periodicPairs'], records.periodicPairs);
  collectArray(modelRoot?.materials, ['model', 'materials'], records.materials);
  if (modelRoot?.physics && typeof modelRoot.physics === 'object' && !Array.isArray(modelRoot.physics)) {
    records.physics.push({ id: modelRoot.physics.id, value: modelRoot.physics,
      path: ['model', 'physics'], pointer: '/model/physics' });
  }
  collectArray(modelRoot?.study, ['model', 'study'], records.studies);
  collectArray(modelRoot?.results, ['model', 'results'], records.results);
  for (const result of records.results) {
    collectArray(result.value?.datasets, [...result.path, 'datasets'], records.resultDatasets);
  }

  const diagnostics = [];
  const indexes = Object.fromEntries(Object.entries(records)
    .filter(([name]) => name !== 'resultDatasets')
    .map(([name, items]) => [name, buildIndex(name, items, diagnostics)]));
  const relationships = [];
  const physicsSource = records.physics[0] || null;
  const relate = ({ kind, source, field, targetCollection, reference, isEligible = () => true }) => {
    if (typeof reference !== 'string' || !reference) return;
    const targetIndex = indexes[targetCollection];
    const target = targetIndex?.byId.get(reference) || null;
    const duplicate = targetIndex?.duplicates.has(reference) || false;
    const status = duplicate ? 'ambiguous' : !target ? 'missing' : isEligible(target.value) ? 'resolved' : 'ineligible';
    const edge = {
      kind, sourcePath: source.path, sourcePointer: source.pointer, field,
      targetCollection, reference, targetPath: target?.path || null, targetPointer: target?.pointer || null, status,
    };
    relationships.push(edge);
    if (status !== 'resolved') diagnostics.push({ code: `reference-${status}`, ...edge,
      message: duplicate ? `Reference “${reference}” resolves to duplicate ${targetCollection} IDs.`
        : status === 'ineligible' ? `Reference “${reference}” resolves to an ineligible ${targetCollection} item.`
          : `Reference “${reference}” does not resolve in ${targetCollection}.` });
  };

  for (const selection of records.selections) {
    (selection.value?.addresses || []).forEach((address, addressIndex) => {
      const memberships = address?.membership;
      if (!memberships || typeof memberships !== 'object' || Array.isArray(memberships)) return;
      const addressPath = [...selection.path, 'addresses', addressIndex];
      const source = { value: address, path: addressPath, pointer: pointer(addressPath) };
      for (const reference of Object.keys(memberships)) relate({ kind: 'selection-feature-membership', source,
        field: `membership/${pointerEscape(reference)}`, targetCollection: 'geometry', reference });
    });
  }
  for (const item of records.integrationOperators) relate({ kind: 'integration-selection', source: item,
    field: 'selectionId', targetCollection: 'selections', reference: item.value?.selectionId });
  for (const item of records.meshSizeControls) relate({ kind: 'mesh-size-selection', source: item,
    field: 'selectionId', targetCollection: 'selections', reference: item.value?.selectionId });
  for (const item of records.periodicPairs) {
    relate({ kind: 'periodic-lower-selection', source: item, field: 'lowerSelectionId',
      targetCollection: 'selections', reference: item.value?.lowerSelectionId,
      isEligible: selection => selection.entity === 'boundary' });
    relate({ kind: 'periodic-upper-selection', source: item, field: 'upperSelectionId',
      targetCollection: 'selections', reference: item.value?.upperSelectionId,
      isEligible: selection => selection.entity === 'boundary' });
  }
  for (const item of records.materials) relate({ kind: 'material-selection', source: item,
    field: 'selectionId', targetCollection: 'selections', reference: item.value?.selectionId,
    isEligible: selection => selection.entity === 'domain' });

  const physics = physicsSource?.value;
  const enabledPeriodicPairIds = new Set(Array.isArray(modelRoot?.mesh?.periodicPairIds)
    ? modelRoot.mesh.periodicPairIds : []);
  if (physicsSource) {
    (Array.isArray(physics.domainSelectionIds) ? physics.domainSelectionIds : []).forEach((reference, index) =>
      relate({ kind: 'physics-domain-selection', source: physicsSource, field: `domainSelectionIds/${index}`,
        targetCollection: 'selections', reference, isEligible: selection => selection.entity === 'domain' }));
    for (const [key, condition] of Object.entries(physics)) {
      if (!condition || typeof condition !== 'object' || Array.isArray(condition) || typeof condition.type !== 'string') continue;
      const sourcePath = ['model', 'physics', key];
      const source = { value: condition, path: sourcePath, pointer: pointer(sourcePath) };
      if (condition.type === 'periodicCondition') {
        relate({ kind: 'periodic-condition-pair', source, field: 'periodicPairId',
          targetCollection: 'periodicPairs', reference: condition.periodicPairId,
          isEligible: () => enabledPeriodicPairIds.has(condition.periodicPairId) });
      } else if (['dirichlet', 'neumann', 'impedance'].includes(condition.type)) {
        relate({ kind: 'physics-condition-selection', source, field: 'selectionId',
          targetCollection: 'selections', reference: condition.selectionId,
          isEligible: selection => selection.entity === 'boundary' });
      }
    }
  }
  for (const item of records.studies) relate({ kind: 'study-physics', source: item,
    field: 'physicsId', targetCollection: 'physics', reference: item.value?.physicsId });
  for (const item of records.results) {
    (item.value?.datasets || []).forEach((dataset, datasetIndex) => {
      if (dataset?.source?.type === 'study') {
        const datasetPath = [...item.path, 'datasets', datasetIndex];
        const source = { value: dataset, path: datasetPath, pointer: pointer(datasetPath) };
        relate({ kind: 'dataset-study', source, field: 'source/studyId', targetCollection: 'studies',
          reference: dataset.source.studyId });
      }
    });
  }
  return { profileId: projectedDocument?.scenario?.profileId ?? null, records, indexes, relationships, diagnostics };
}

export const modelProjection = Object.freeze({ projectModel });
