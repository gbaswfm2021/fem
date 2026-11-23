import { compileDeclarativeValidation } from '../../../platform/ui/widgets/validation-program/declarative-rules.js';

const PROFILE_CAPABILITY_PATHS = [
  ['mesh', 'elementOrdersByProfile'],
  ['physics', 'featureTypesByProfile'],
  ['physics', 'periodicPolicyByProfile'],
  ['physics', 'solverBoundaryTypesByProfile'],
  ['study', 'discretizationChoicesByProfile'],
  ['study', 'preparationByProfile'],
  ['study', 'solverRuntimesByProfile'],
  ['study', 'solverSettingsByProfile'],
];
const SCHEMA_CONTRIBUTION_SECTIONS = [
  'x-fullmeta-metadataValidation',
  'x-fullmeta-preparationValidation',
  'x-fullmeta-rules',
];

const isRecord = value => value !== null && typeof value === 'object' && !Array.isArray(value);

function profileDocument(contract, registryEntry, root) {
  const { id, file } = registryEntry || {};
  if (typeof id !== 'string' || !/^[a-z][a-z0-9-]{0,63}$/.test(id)
    || file !== `${id.replaceAll('-', '')}.json` || !isRecord(contract)) {
    throw new Error('FEM scenario registry entries must use a safe profile ID and its matching JSON filename.');
  }
  if (contract.contractVersion !== 'fullmeta-fem-scenario/1' || contract.id !== id
    || typeof contract.version !== 'string' || !contract.version.trim()) {
    throw new Error(`FEM scenario package ${id} has an unsupported contract or mismatched identity.`);
  }
  const document = contract.schemaDocument;
  const expectedId = `https://fullmeta.local/applications/fem/scenarios/${id}.schema.json`;
  if (!isRecord(document) || document.$id !== expectedId
    || document['x-fem-scenario']?.profile?.id !== id) {
    throw new Error(`FEM scenario package ${id} has an invalid schema document or schema identity.`);
  }
  const profile = document['x-fem-scenario'].profile;
  if (!Number.isSafeInteger(profile.dimension) || profile.dimension < 1
    || typeof profile.physics !== 'string' || !profile.physics.trim()
    || !Array.isArray(profile.formulations) || !profile.formulations.length
    || profile.formulations.some(value => typeof value !== 'string' || !value.trim())) {
    throw new Error(`FEM scenario package ${id} has incomplete profile metadata.`);
  }
  const fieldFormulations = contract.capabilities?.mesh?.fieldFormulations;
  if (!isRecord(fieldFormulations)
    || profile.formulations.some(formulation => !Object.hasOwn(fieldFormulations, formulation))) {
    throw new Error(`FEM scenario package ${id} must declare a mesh field formulation catalog for every profile formulation.`);
  }
  const cloned = structuredClone(document);
  // The same fragment is embedded under the root's $defs and exposed in the
  // annotation-document list. A nested $id would register it twice.
  delete cloned.$id;
  if (!cloned.$schema && typeof root.$schema === 'string') cloned.$schema = root.$schema;
  return cloned;
}

function mergeContribution(target, path, key, value, profileId) {
  let parent = target;
  for (const part of path) parent = parent[part] ||= {};
  const table = parent[key] ||= {};
  if (Object.hasOwn(table, profileId) && JSON.stringify(table[profileId]) !== JSON.stringify(value)) {
    throw new Error(`FEM scenario ${profileId} conflicts with x-fem-capabilities.${[...path, key, profileId].join('.')}.`);
  }
  table[profileId] = structuredClone(value);
}

function rewriteScenarioBindings(value, bindings) {
  if (Array.isArray(value)) return value.map(child => rewriteScenarioBindings(child, bindings));
  if (!isRecord(value)) return value;
  return Object.fromEntries(Object.entries(value).map(([key, child]) => [key,
    (key === 'expose' || key === 'binding') && typeof child === 'string' && bindings.has(child)
      ? bindings.get(child) : rewriteScenarioBindings(child, bindings),
  ]));
}

function sameExposure(left, right) {
  return JSON.stringify({ constraint: left?.constraint, scope: left?.scope, when: left?.when })
    === JSON.stringify({ constraint: right?.constraint, scope: right?.scope, when: right?.when });
}

function mergeScenarioObligations(current, additions, profileId) {
  const safeId = profileId.replace(/[^A-Za-z0-9_]/g, '_');
  const bindings = new Map();
  for (const addition of additions) {
    const name = addition?.constraint?.expose;
    if (typeof name !== 'string') continue;
    const existing = current.find(value => value?.constraint?.expose === name);
    if (existing && !sameExposure(existing, addition)) {
      bindings.set(name, `scenario_${safeId}__${name}`);
    }
  }
  const fingerprints = new Set(current.map(value => JSON.stringify(value)));
  const identifiers = new Set(current.map(value => value?.id).filter(value => typeof value === 'string'));
  for (const addition of additions) {
    const originalFingerprint = JSON.stringify(addition);
    if (fingerprints.has(originalFingerprint)) continue;
    if (typeof addition?.constraint?.expose === 'string') {
      const existing = current.find(value => value?.constraint?.expose === addition.constraint.expose);
      if (existing && sameExposure(existing, addition)) continue;
    }
    const contribution = rewriteScenarioBindings(addition, bindings);
    if (typeof contribution?.id === 'string' && identifiers.has(contribution.id)) {
      const stem = `${contribution.id}__scenario_${safeId}`;
      contribution.id = stem;
      let suffix = 2;
      while (identifiers.has(contribution.id)) contribution.id = `${stem}_${suffix++}`;
    }
    const fingerprint = JSON.stringify(contribution);
    if (!fingerprints.has(fingerprint)) {
      current.push(structuredClone(contribution));
      fingerprints.add(fingerprint);
      if (typeof contribution?.id === 'string') identifiers.add(contribution.id);
    }
  }
}

function mergeScenarioDeclarations(root, contract, id) {
  const contributions = contract.schemaContributions;
  if (contributions === undefined) return;
  if (!isRecord(contributions)) throw new Error(`FEM scenario ${id} schemaContributions must be an object.`);
  for (const section of SCHEMA_CONTRIBUTION_SECTIONS) {
    const source = contributions[section];
    if (source === undefined) continue;
    if (!isRecord(source)) throw new Error(`FEM scenario ${id} contribution ${section} must be an object.`);
    const target = root[section] ||= {};
    if (!isRecord(target)) throw new Error(`FEM base schema ${section} must be an object.`);
    const targetRules = target.declarative ||= {};
    const sourceRules = source.declarative;
    if (sourceRules !== undefined && !isRecord(sourceRules)) {
      throw new Error(`FEM scenario ${id} contribution ${section}.declarative must be an object.`);
    }
    for (const key of ['obligations', 'rules']) {
      const additions = sourceRules?.[key] ?? source[key];
      if (additions === undefined) continue;
      if (!Array.isArray(additions)) throw new Error(`FEM scenario ${id} contribution ${section}.${key} must be an array.`);
      const current = key === 'obligations' ? (targetRules.obligations ||= []) : (target.rules ||= []);
      if (!Array.isArray(current)) throw new Error(`FEM base schema ${section}.${key} must be an array.`);
      if (key === 'obligations') mergeScenarioObligations(current, additions, id);
      else {
        const fingerprints = new Set(current.map(value => JSON.stringify(value)));
        for (const value of additions) {
          const fingerprint = JSON.stringify(value);
          if (!fingerprints.has(fingerprint)) current.push(structuredClone(value));
          fingerprints.add(fingerprint);
        }
      }
    }
  }
  for (const [key, pointer] of [
    ['modelAllOf', '/properties/model/allOf'],
    ['physicsAllOf', '/properties/model/properties/physics/allOf'],
    ['studyItemAllOf', '/properties/model/properties/study/items/allOf'],
  ]) {
    const additions = contributions[key];
    if (additions === undefined) continue;
    if (!Array.isArray(additions)) throw new Error(`FEM scenario ${id} contribution ${key} must be an array.`);
    const parts = pointer.split('/').slice(1);
    let parent = root;
    for (const part of parts.slice(0, -1)) parent = parent[part] ||= {};
    const current = parent[parts.at(-1)] ||= [];
    if (!Array.isArray(current)) throw new Error(`FEM base schema ${pointer} must be an array.`);
    const fingerprints = new Set(current.map(value => JSON.stringify(value)));
    for (const value of additions) {
      const fingerprint = JSON.stringify(value);
      if (!fingerprints.has(fingerprint)) current.push(structuredClone(value));
      fingerprints.add(fingerprint);
    }
  }
  if (contributions['x-fullmeta-ui-profiles'] !== undefined) {
    const ui = contributions['x-fullmeta-ui-profiles'];
    if (!isRecord(ui) || !isRecord(ui[id])) throw new Error(`FEM scenario ${id} must declare its UI profile contribution.`);
    root['x-fullmeta-ui-profiles'] ||= {};
    if (!isRecord(root['x-fullmeta-ui-profiles'])) throw new Error('FEM base x-fullmeta-ui-profiles must be an object.');
    root['x-fullmeta-ui-profiles'][id] = structuredClone(ui[id]);
  }
  const runtimePrograms = contributions['x-fullmeta-runtime']?.externalPrograms;
  if (runtimePrograms !== undefined) {
    if (!isRecord(runtimePrograms)) throw new Error(`FEM scenario ${id} externalPrograms must be an object.`);
    const rootPrograms = root['x-fullmeta-runtime']?.externalPrograms;
    if (!isRecord(rootPrograms)) throw new Error('FEM base schema must declare x-fullmeta-runtime.externalPrograms.');
    for (const [programId, program] of Object.entries(runtimePrograms)) {
      if (!/^[A-Za-z][A-Za-z0-9]{0,63}$/.test(programId) || !isRecord(program)
        || program.pathType !== 'relative' || typeof program.executable !== 'string'
        || !program.executable.startsWith('.runtime/') || program.executable.includes('\\')
        || program.executable.split('/').some(part => !part || part === '.' || part === '..')
        || !Array.isArray(program.allowedEntries) || !program.allowedEntries.length
        || program.allowedEntries.some(value => typeof value !== 'string' || !value.trim())) {
        throw new Error(`FEM scenario ${id} declares an unsafe runtime program ${programId}.`);
      }
      if (Object.hasOwn(rootPrograms, programId) && JSON.stringify(rootPrograms[programId]) !== JSON.stringify(program)) {
        throw new Error(`FEM scenario ${id} conflicts with runtime program ${programId}.`);
      }
      rootPrograms[programId] = structuredClone(program);
    }
  }
}

function mergeScenarioCapabilities(root, contract, id) {
  const capabilities = contract.capabilities;
  for (const [key, value] of Object.entries(capabilities.mesh?.fieldFormulations || {})) {
    const current = root['x-fem-capabilities'].mesh.fieldFormulations[key];
    if (current !== undefined && JSON.stringify(current) !== JSON.stringify(value)) {
      throw new Error(`FEM scenario ${id} conflicts with mesh field formulation ${key}.`);
    }
    root['x-fem-capabilities'].mesh.fieldFormulations[key] = structuredClone(value);
  }
  const optionRules = capabilities.study?.solverOptionRules || [];
  if (!Array.isArray(optionRules)) throw new Error(`FEM scenario ${id} study.solverOptionRules must be an array.`);
  const target = root['x-fem-capabilities'].study.solverOptionRules;
  const fingerprints = new Set(target.map(value => JSON.stringify(value)));
  for (const value of optionRules) {
    const fingerprint = JSON.stringify(value);
    if (!fingerprints.has(fingerprint)) target.push(structuredClone(value));
    fingerprints.add(fingerprint);
  }
}

function validateScenarioPrograms(contract, id) {
  const scenario = contract.schemaDocument?.['x-fem-scenario'];
  const studyType = scenario?.studyType;
  if (!['stationary', 'frequencyDomain'].includes(studyType)) {
    throw new Error(`FEM scenario ${id} must declare x-fem-scenario.studyType as stationary or frequencyDomain.`);
  }
  if (typeof scenario?.supportsStationary !== 'boolean') {
    throw new Error(`FEM scenario ${id} must declare x-fem-scenario.supportsStationary as a boolean.`);
  }
  if (studyType === 'stationary' && scenario.supportsStationary !== true) {
    throw new Error(`FEM scenario ${id} must enable stationary Studies when its default Study type is stationary.`);
  }
  const starterStudyType = contract.schemaDocument?.['x-fem-scenario']?.starterDefaults?.study?.type;
  if (starterStudyType !== undefined && starterStudyType !== studyType) {
    throw new Error(`FEM scenario ${id} starter Study type must match x-fem-scenario.studyType.`);
  }
  const declared = contract.schemaContributions?.['x-fullmeta-runtime']?.externalPrograms;
  if (!isRecord(declared)) throw new Error(`FEM scenario ${id} must declare runtime programs in its contract.`);
  const capabilities = contract.capabilities?.study;
  const preparation = capabilities?.preparationByProfile?.[id];
  const runtimes = capabilities?.solverRuntimesByProfile?.[id];
  const used = new Set();
  if (typeof preparation?.programId === 'string') used.add(preparation.programId);
  if (isRecord(runtimes)) for (const runtime of Object.values(runtimes)) {
    if (typeof runtime?.programId === 'string') used.add(runtime.programId);
  }
  for (const programId of used) {
    if (!Object.hasOwn(declared, programId)) throw new Error(`FEM scenario ${id} references runtime ${programId} without registering it.`);
  }
}

function materialSchemaForProfile(document, id) {
  const declarations = document['x-fem-scenario']?.materials?.properties;
  if (!Array.isArray(declarations)) throw new Error(`FEM scenario ${id} must declare material properties.`);
  const properties = {};
  const required = ['id', 'name', 'selectionId'];
  for (const declaration of declarations) {
    const key = declaration?.key;
    if (typeof key !== 'string' || !/^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(key)
      || !isRecord(declaration.schema)) {
      throw new Error(`FEM scenario ${id} contains a material property without a safe key and JSON Schema.`);
    }
    if (required.includes(key)) throw new Error(`FEM scenario ${id} cannot redefine the generic material field ${key}.`);
    if (Object.hasOwn(properties, key) && JSON.stringify(properties[key]) !== JSON.stringify(declaration.schema)) {
      throw new Error(`FEM scenario ${id} declares conflicting schemas for material ${key}.`);
    }
    properties[key] = structuredClone(declaration.schema);
    if (declaration.required === true) required.push(key);
  }
  return { properties, required: [...new Set(required)] };
}

function configureScenarioSchema(root, document, id) {
  const material = materialSchemaForProfile(document, id);
  const modelProperties = document.then?.properties?.model?.properties;
  if (!isRecord(modelProperties)) throw new Error(`FEM scenario ${id} schema must declare a model-specific then branch.`);
  const baseMaterialProperties = root.properties?.model?.properties?.materials?.items?.properties;
  const genericMaterialProperties = Object.fromEntries(['id', 'name', 'selectionId']
    .map(key => [key, baseMaterialProperties?.[key]]).filter(([, value]) => isRecord(value)));
  if (Object.keys(genericMaterialProperties).length !== 3) {
    throw new Error('FEM base schema must declare generic material fields id, name, and selectionId.');
  }
  modelProperties.materials ||= {};
  modelProperties.materials.items = {
    type: 'object',
    additionalProperties: false,
    properties: { ...structuredClone(genericMaterialProperties), ...material.properties },
    required: material.required,
  };

  const physics = document['x-fem-physics'];
  const supportedBoundaries = physics?.supportedBoundaryTypes;
  const supportedFeatures = physics?.supportedFeatureTypes;
  if (!Array.isArray(supportedBoundaries) || !Array.isArray(supportedFeatures)
    || !supportedBoundaries.every(type => typeof type === 'string' && type.trim())
    || !supportedFeatures.every(type => typeof type === 'string' && type.trim())) {
    throw new Error(`FEM scenario ${id} must declare supported boundary and feature types.`);
  }
  const supported = [...supportedBoundaries, ...supportedFeatures];
  if (new Set(supported).size !== supported.length) {
    throw new Error(`FEM scenario ${id} boundary and feature type declarations must be unique nonempty strings.`);
  }
  const fixedSchemas = root.properties?.model?.properties?.physics?.additionalProperties?.oneOf;
  if (!Array.isArray(fixedSchemas)) throw new Error('FEM base schema must declare its fixed Physics condition variants.');
  const branches = supported.map(type => {
    const branch = fixedSchemas.find(candidate => candidate?.properties?.type?.const === type);
    if (!branch) throw new Error(`FEM base schema has no fixed Physics condition variant for ${type}.`);
    const effective = structuredClone(branch);
    if (physics?.featureHandlingByType?.[type] === 'domainSource') {
      const sourceLayout = physics?.domainSourceLayoutsByType?.[type] || physics?.domainSourceLayout;
      if (!isRecord(sourceLayout) || !Array.isArray(sourceLayout.componentLabels)
        || sourceLayout.componentLabels.length < 1 || sourceLayout.componentLabels.length > 9
        || sourceLayout.componentLabels.some(label => typeof label !== 'string' || !label.trim())) {
        throw new Error(`FEM scenario ${id} must label each domain source component.`);
      }
      const values = effective.properties?.values;
      if (!values || !values['x-fullmeta-ui-behavior']?.payload?.widget?.settings) {
        throw new Error('FEM base schema must define the domain source quantity widget.');
      }
      values.minItems = values.maxItems = sourceLayout.componentLabels.length;
      values['x-fullmeta-ui-behavior'].payload.widget.settings.componentLabels = [...sourceLayout.componentLabels];
      if (typeof sourceLayout.title === 'string' && sourceLayout.title.trim()) values.title = sourceLayout.title;
      const unitChoices = new Set();
      for (const [formulation, defaults] of Object.entries(physics.boundaryDefaultsByFormulation || {})) {
        const formulationLabels = sourceLayout.labelsByFormulation?.[formulation];
        if (formulationLabels !== undefined && (!Array.isArray(formulationLabels)
          || formulationLabels.length !== sourceLayout.componentLabels.length
          || formulationLabels.some(label => typeof label !== 'string' || !label.trim()))) {
          throw new Error(`FEM scenario ${id} has invalid domain source labels for ${formulation}.`);
        }
        const components = defaults?.[type]?.values;
        if (!Array.isArray(components) || components.length !== sourceLayout.componentLabels.length
          || components.some(component => typeof component?.unit !== 'string' || !component.unit.trim())) {
          throw new Error(`FEM scenario ${id} must declare domain source defaults and units for each formulation.`);
        }
        for (const component of components) unitChoices.add(component.unit);
      }
      if (!unitChoices.size) throw new Error(`FEM scenario ${id} has no domain source unit choices.`);
      values.items.properties.unit.enum = [...unitChoices];
      const parameterUnits = root.properties?.model?.properties?.parameters?.items?.properties?.unit?.enum;
      if (!Array.isArray(parameterUnits)) throw new Error('FEM base schema must declare parameter units.');
      for (const unit of unitChoices) if (!parameterUnits.includes(unit)) parameterUnits.push(unit);
    }
    const layout = physics?.boundaryFieldLayouts?.[type];
    if (!layout) delete effective.properties?.fieldValues;
    else {
      if (!['primary', 'derived'].includes(layout.role) || !Array.isArray(layout.shape) || !layout.shape.length
        || layout.shape.some(size => !Number.isSafeInteger(size) || size < 1))
        throw new Error(`FEM scenario ${id} must declare a field role and positive shape for ${type}.`);
      const count = layout.shape.reduce((total, size) => total * size, 1);
      if (!Number.isSafeInteger(count) || count > 1024 || !Array.isArray(layout.componentLabels)
        || layout.componentLabels.length !== count || layout.componentLabels.some(label => typeof label !== 'string' || !label.trim()))
        throw new Error(`FEM scenario ${id} must label every field component for ${type}.`);
      effective.properties.fieldValues.minItems = count;
      effective.properties.fieldValues.maxItems = count;
      effective.properties.fieldValues['x-fullmeta-ui-behavior'].payload.widget.settings.componentLabels = [...layout.componentLabels];
      if (layout.zeroScalarOnly && effective.properties?.value) effective.properties.value.title = 'scalar value (zero only)';
    }
    return effective;
  });
  const basePhysicsProperties = root.properties?.model?.properties?.physics?.properties;
  if (!isRecord(basePhysicsProperties)) throw new Error('FEM base schema must declare its fixed Physics properties.');
  const physicsSchema = modelProperties.physics ||= {};
  physicsSchema.properties = {
    ...structuredClone(basePhysicsProperties),
    ...(isRecord(physicsSchema.properties) ? physicsSchema.properties : {}),
  };
  physicsSchema.additionalProperties ||= {};
  physicsSchema.additionalProperties.oneOf = branches;
}

function composePhysicsConditionActions(root, documents) {
  const menu = root['x-fullmeta-ui-tree']?.actionMenus?.find(rule =>
    rule.when?.scope === 'item' && JSON.stringify(rule.when.path) === JSON.stringify(['model', 'physics']));
  if (!menu || !Array.isArray(menu.entries)) throw new Error('FEM base schema must declare the Physics condition action menu.');
  const actions = new Map();
  for (const document of documents) {
    const profile = document['x-fem-scenario']?.profile;
    const physics = document['x-fem-physics'];
    const declarations = physics?.conditionActions;
    if (!Array.isArray(declarations)) throw new Error(`FEM scenario ${profile?.id} must declare conditionActions.`);
    const declaredTypes = new Set();
    for (const action of declarations) {
      if (!isRecord(action) || typeof action.action !== 'string' || !action.action.trim()
        || typeof action.conditionType !== 'string' || !action.conditionType.trim()
        || typeof action.itemName !== 'string' || !action.itemName.trim()
        || typeof action.label !== 'string' || !action.label.trim()) {
        throw new Error(`FEM scenario ${profile?.id} contains a malformed physics condition action.`);
      }
      if (declaredTypes.has(action.conditionType)) throw new Error(`FEM scenario ${profile?.id} declares condition action ${action.conditionType} more than once.`);
      declaredTypes.add(action.conditionType);
      const current = actions.get(action.conditionType);
      if (current && JSON.stringify(current) !== JSON.stringify(action)) {
        throw new Error(`FEM condition type ${action.conditionType} has conflicting action declarations.`);
      }
      actions.set(action.conditionType, structuredClone(action));
    }
    for (const type of [...(physics.supportedBoundaryTypes || []), ...(physics.supportedFeatureTypes || [])]) {
      if (!declaredTypes.has(type)) throw new Error(`FEM scenario ${profile?.id} has no UI action for supported condition ${type}.`);
    }
  }
  menu.entries = menu.entries.filter(entry => !entry.conditionType);
  menu.entries.push(...actions.values());
}

function composeMaterialSchemas(root, documents) {
  const itemSchema = root.properties?.model?.properties?.materials?.items;
  if (!isRecord(itemSchema) || !isRecord(itemSchema.properties) || !Array.isArray(itemSchema.required)) {
    throw new Error('FEM base schema must declare model.materials.items with properties and required fields.');
  }
  const genericProperties = new Set(['id', 'name', 'selectionId']);
  for (const key of Object.keys(itemSchema.properties)) if (!genericProperties.has(key)) delete itemSchema.properties[key];
  itemSchema.required = itemSchema.required.filter(key => genericProperties.has(key));
  for (const document of documents) {
    const scenario = document['x-fem-scenario'];
    const id = scenario.profile.id;
    const material = materialSchemaForProfile(document, id);
    for (const [key, schema] of Object.entries(material.properties)) {
      const existing = itemSchema.properties[key];
      if (existing && JSON.stringify(existing) !== JSON.stringify(schema)) {
        throw new Error(`FEM material ${key} has conflicting schema declarations across scenarios.`);
      }
      itemSchema.properties[key] = structuredClone(schema);
    }
  }
  itemSchema.required = [...new Set(itemSchema.required)];
}

/** Compose installed scenario contracts into the effective, in-memory FEM schema graph. */
export function composeScenarioContracts(rootSchema, packages) {
  if (!isRecord(rootSchema) || !Array.isArray(packages) || !packages.length) {
    throw new Error('FEM requires a root schema and at least one installed scenario package.');
  }
  const root = structuredClone(rootSchema);
  root.$defs ||= {};
  root.$defs.femProfiles = {};
  root.allOf = (Array.isArray(root.allOf) ? root.allOf : [])
    .filter(item => !/^#\/\$defs\/femProfiles\//.test(item?.$ref || ''));
  root['x-fem-capabilities'] ||= {};
  const documents = [];
  const seen = new Set();
  const profiles = [];
  for (const entry of packages) {
    if (!entry || !isRecord(entry.contract)) throw new Error('FEM scenario package entry is malformed.');
    const { id } = entry.registry;
    if (seen.has(id)) throw new Error(`FEM scenario ${id} is registered more than once.`);
    seen.add(id);
    validateScenarioPrograms(entry.contract, id);
    const document = profileDocument(entry.contract, entry.registry, root);
    configureScenarioSchema(root, document, id);
    root.$defs.femProfiles[id] = document;
    documents.push(document);
    profiles.push(document['x-fem-scenario'].profile);
    mergeScenarioDeclarations(root, entry.contract, id);
    mergeScenarioCapabilities(root, entry.contract, id);
    const reference = { $ref: `#/$defs/femProfiles/${id}` };
    if (!root.allOf.some(item => item?.$ref === reference.$ref)) root.allOf.push(reference);
    const contributions = entry.contract.capabilities;
    if (!isRecord(contributions)) throw new Error(`FEM scenario ${id} must declare capability contributions.`);
    for (const fullPath of PROFILE_CAPABILITY_PATHS) {
      const value = fullPath.reduce((current, part) => current?.[part], contributions);
      if (value !== undefined) {
        if (!isRecord(value) || !Object.hasOwn(value, id)) throw new Error(`FEM scenario ${id} has malformed ${fullPath.join('.')} capabilities.`);
        mergeContribution(root['x-fem-capabilities'], fullPath.slice(0, -1), fullPath.at(-1), value[id], id);
      }
    }
  }
  // Scenario packages carry authored declarative obligations. Regenerate the
  // executable rule AST after composition so runtime validation sees the same
  // complete profile set as the effective schema metadata.
  for (const section of SCHEMA_CONTRIBUTION_SECTIONS) {
    const program = root[section];
    if (Array.isArray(program?.declarative?.obligations)) {
      program.rules = compileDeclarativeValidation(program.declarative.obligations);
    }
  }
  const profileIds = profiles.map(profile => profile.id);
  composePhysicsConditionActions(root, documents);
  composeMaterialSchemas(root, documents);
  const scenario = root.properties?.scenario;
  if (!isRecord(scenario) || !isRecord(scenario.properties)) {
    throw new Error('FEM base schema must declare the scenario object and its properties.');
  }
  scenario.oneOf = profiles.map(profile => ({
    properties: {
      profileId: { const: profile.id },
      dimension: { const: profile.dimension },
      physics: { const: profile.physics },
    },
  }));
  scenario.properties.profileId = {
    ...(scenario.properties.profileId || {}),
    enum: profileIds,
  };
  scenario.properties.dimension = {
    ...(scenario.properties.dimension || {}),
    enum: [...new Set(profiles.map(profile => profile.dimension))].sort((a, b) => a - b),
  };
  scenario.properties.physics = {
    ...(scenario.properties.physics || {}),
    enum: [...new Set(profiles.map(profile => profile.physics))].sort(),
  };
  const modelPhysics = root.properties?.model?.properties?.physics;
  if (!isRecord(modelPhysics?.properties)) throw new Error('FEM base model physics schema must declare properties.');
  const formulations = [...new Set(profiles.flatMap(profile => profile.formulations))].sort();
  const physicsTypes = [...new Set(profiles.map(profile => profile.modelPhysicsType || profile.physics))].sort();
  modelPhysics.properties.type = { ...(modelPhysics.properties.type || {}), enum: physicsTypes };
  modelPhysics.properties.formulation = { ...(modelPhysics.properties.formulation || {}), enum: formulations };
  const fieldFormulation = root.properties?.model?.properties?.study?.items?.properties?.discretization?.properties?.fieldFormulation;
  if (fieldFormulation) fieldFormulation.enum = Object.keys(root['x-fem-capabilities'].mesh.fieldFormulations).sort();
  const editorScenario = root['x-fullmeta-ui-modelEditor']?.scenario;
  if (editorScenario && !profileIds.includes(editorScenario.defaultProfileId)) {
    editorScenario.defaultProfileId = packages.find(item => item.registry.default === true)?.registry.id || profileIds[0];
  }
  return { schema: root, scenarioDocuments: documents };
}

/** Load packages using only the application-owned registry and safe sibling filenames. */
export async function loadScenarioContracts({ registryUrl, loadJson }) {
  const registry = await loadJson(registryUrl);
  if (!isRecord(registry) || registry.format !== 'fullmeta-fem-scenario-registry/1'
    || !Array.isArray(registry.packages)) {
    throw new Error('The FEM scenario registry is malformed or unsupported.');
  }
  const seen = new Set();
  let defaults = 0;
  return Promise.all(registry.packages.map(async item => {
    if (!isRecord(item) || typeof item.id !== 'string' || seen.has(item.id)) {
      throw new Error('The FEM scenario registry contains a malformed or duplicate ID.');
    }
    seen.add(item.id);
    if (!/^[a-z][a-z0-9-]{0,63}$/.test(item.id) || item.file !== `${item.id.replaceAll('-', '')}.json`) {
      throw new Error(`FEM scenario registry entry ${String(item.id)} has an unsafe package path.`);
    }
    if (item.default !== undefined && typeof item.default !== 'boolean') {
      throw new Error(`FEM scenario registry entry ${item.id} default must be a boolean when declared.`);
    }
    if (item.default === true && ++defaults > 1) {
      throw new Error('The FEM scenario registry may declare at most one default profile.');
    }
    const contract = await loadJson(new URL(item.file, registryUrl));
    return { registry: item, contract };
  }));
}
