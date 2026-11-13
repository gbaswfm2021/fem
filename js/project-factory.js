// Model 2 starter documents for the launcher and schema-project workflow.
const profiles = Object.freeze({
  'acoustic-2d': { dimension: 2, physics: 'acoustics', formulation: 'pressure', elementFamily: 'triangle', wave: true },
  'acoustic-3d': { dimension: 3, physics: 'acoustics', formulation: 'pressure', elementFamily: 'tetrahedron', wave: true },
  'em-2d': { dimension: 2, physics: 'electromagnetics', formulation: 'Ez', elementFamily: 'triangle', wave: true },
  'em-3d': { dimension: 3, physics: 'electromagnetics', formulation: 'electric', elementFamily: 'tetrahedron', wave: true },
  'thermal-2d': { dimension: 2, physics: 'thermal', formulation: 'T', elementFamily: 'triangle', wave: false, studyType: 'stationary' },
});

export const launcherProfiles = Object.freeze([
  { id: 'acoustic-2d', name: '2D acoustics', description: '2D pressure acoustics', status: 'available' },
  { id: 'acoustic-3d', name: '3D acoustics', description: '3D pressure acoustics', status: 'available' },
  { id: 'em-2d', name: '2D electromagnetics', description: '2D Ez formulation', status: 'available' },
  { id: 'em-3d', name: '3D electromagnetics', description: '3D electric formulation', status: 'available' },
  { id: 'thermal-2d', name: '2D thermal', description: 'Stationary temperature', status: 'available' },
]);

function workspaceParts(path, label, allowRoot = false) {
  if (allowRoot && (path === '.' || path === '')) return [];
  if (typeof path !== 'string' || !path || path.startsWith('/') || path.includes('\\')
    || path.split('/').some(part => !part || part === '.' || part === '..')
    || /[:?#]/.test(path)) throw new Error(`${label} must be a canonical workspace-relative path.`);
  return path.split('/');
}

export function relativeSchemaReference(folder, schemaPath) {
  const from = workspaceParts(folder, 'Project folder', true);
  const to = workspaceParts(schemaPath, 'Schema path');
  let common = 0;
  while (common < from.length && common < to.length && from[common] === to[common]) common++;
  return [...Array(from.length - common).fill('..'), ...to.slice(common)].join('/');
}

export function createModelDocument({ modelId, profileId, folder }, { applicationId, schemaPath, profiles: declaredProfiles }) {
  if (typeof modelId !== 'string' || !/^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(modelId)) {
    throw new Error('Use a project ID starting with a letter, followed by letters, digits, or underscores.');
  }
  const availableProfiles = declaredProfiles || profiles;
  if (!Object.hasOwn(availableProfiles, profileId)) throw new Error(`Unsupported target application profile: ${profileId}`);
  if (applicationId !== 'fem') throw new Error('The registered application ID must be fem.');
  const profile = availableProfiles[profileId];
  return {
    application: applicationId,
    metadata: { id: modelId, name: modelId },
    modelFormat: 'fem-model/2',
    scenario: { dimension: profile.dimension, physics: profile.physics, profileId },
    'x-fullmeta-schema': relativeSchemaReference(folder, schemaPath),
    model: {
      units: 'm', parameters: [], variables: [], pointwiseVariables: [], integrationOperators: [],
      geometry: { features: [], tolerance: { expression: '1e-9', unit: 'm' } },
      materials: [],
      mesh: {
        elementFamily: profile.elementFamily,
        gmsh: { algorithm3D: 1, elementOrder: 1, optimize: true, optimizeNetgen: true,
          optimizeThreshold: 0.25, smoothing: 10, threads: 1 },
        maxSize: { expression: '0.01', unit: 'm' }, minSize: { expression: '0.001', unit: 'm' },
        meshSizeControls: [], periodicPairs: [],
      },
      physics: {
        id: 'physics', name: 'Physics', type: profile.physics, formulation: profile.formulation,
        domainSelectionIds: [],
        ...(profile.wave ? { frequency: { expression: '1', unit: 'Hz' } } : {}),
        ...(profile.physics === 'electromagnetics' ? { phasorConvention: 'exp(+i omega t)' } : {}),
      },
      study: profile.studyType === 'stationary'
        ? [{ id: 'study', name: 'Stationary study', type: 'stationary', physicsId: 'physics' }]
        : [],
      results: [], selections: [],
    },
  };
}
