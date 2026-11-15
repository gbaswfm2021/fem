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
  const availableProfiles = declaredProfiles;
  if (!availableProfiles || typeof availableProfiles !== 'object' || Array.isArray(availableProfiles)) {
    throw new Error('Create FEM starter models through the loaded scenario registry so profile defaults are available.');
  }
  if (!Object.hasOwn(availableProfiles, profileId)) throw new Error(`Unsupported target application profile: ${profileId}`);
  if (applicationId !== 'fem') throw new Error('The registered application ID must be fem.');
  const profile = availableProfiles[profileId];
  const starterDefaults = profile.starterDefaults;
  if (!starterDefaults || typeof starterDefaults !== 'object' || Array.isArray(starterDefaults)
    || !starterDefaults.physics || typeof starterDefaults.physics !== 'object' || Array.isArray(starterDefaults.physics)) {
    throw new Error(`FEM scenario ${profileId} must declare starterDefaults for the model and Physics component.`);
  }
  const starterStudy = starterDefaults.study;
  if (starterStudy !== undefined && (!starterStudy || typeof starterStudy !== 'object' || Array.isArray(starterStudy)
    || typeof starterStudy.type !== 'string' || !starterStudy.type.trim()
    || typeof starterStudy.name !== 'string' || !starterStudy.name.trim())) {
    throw new Error(`FEM scenario ${profileId} starterDefaults.study must declare a type and name.`);
  }
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
        ...structuredClone(starterDefaults.physics),
      },
      study: starterStudy
        ? [{ ...structuredClone(starterStudy), id: 'study', physicsId: 'physics' }]
        : [],
      results: [], selections: [],
    },
  };
}
