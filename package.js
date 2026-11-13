// Development runtime package. Full visible, native and numerical acceptance
// remains tracked separately from catalog registration and declaration checks.
import { createSchemaProjectPackage } from '../../platform/ui/widgets/model-editor/widget.js';
import { bindRuntimePolicy, schemaAtPath, validateModelRules } from '../../platform/model-rust/js/api.js';
import { loadWorkspaceJson, modelWithArrayItemDraft } from '../../platform/js/services.js';
import { createSchemaTreeActionPlans, resolveTreeDescriptor } from '../../platform/ui/widgets/tree/widget.js';
import { activeModelSchema,activateModelSchema,loadModelSchema } from './schema-documents.js';
import { createModelDocument } from './js/project-factory.js';
import { geometryInsertionIssue, geometryTreeBehavior } from './js/geometry-tree.js';
import { preserveModelDocument } from './js/model-preservation.js';
import { projectModel } from './js/model-projection.js';
import { resultFields } from './js/result-fields.js';
import { createApplicationResultAdapters } from './js/result-adapters.js';
import { createResultPresentation } from './ui/result-presentation.js';
import { createApplicationActions } from './js/operation-actions.js';
import { restorePreview } from './js/preview-restoration.js';
import { fieldChoiceRecords } from './js/field-choices.js';
import { checkedFieldEditErrors,updateCheckedFieldView } from './js/field-view-state.js';
import { createStudyHistory } from './js/study-history.js';
import { changeStudySettings,orderSettingsProperties,projectSettingsSchema,studySettingsChoiceRecords } from './js/settings-schema.js';
import { femReferenceOptions } from './js/reference-choices.js';
import { modelEditorActionBindings } from '../../platform/ui/widgets/model-editor-behavior/widget.js';
import { parseMeshArtifact } from './js/mesh-artifact.js';
import { renderPreview } from './ui/preview.js';
import { mountEditorPanels,renderResultSettings,renderStudyHistory } from './ui/panels.js';
import { selectionAdapter } from './ui/selection-adapter.js';
import { integrationTreeItem, meshSizeTreeItem, periodicPairTreeItem, selectionTreeItem } from './js/selection-tree.js';
import { studyTreeItem, sweepTreeItem } from './js/study-tree.js';
import { fieldPlotTreeItem, globalPlotTreeItem } from './js/result-tree.js';
import { materialTreeItem } from './js/material-tree.js';
import { materialPropertyDescriptors } from './js/material-properties.js';
import { definitionTreeItem, uniqueDefinitionSymbol } from './js/definition-tree.js';
import { prepareArrayDeletion, deleteArrayItem } from './js/tree-deletion.js';
import { saveFemDocument } from './js/document-save.js';
import { createEditorDomain } from './js/editor-domain.js';
import { addPhysicsCondition, conditionToggleAvailability, deletePhysicsCondition, duplicateArrayAvailability, duplicateArrayItem, duplicatePhysicsCondition, guardArrayItem, guardPhysicsCondition, guardPhysicsRoot, prepareConditionDeletion, preparePhysicsCondition, togglePhysicsCondition } from './js/physics-tree.js';

const workspaceUrl = new URL('../../', import.meta.url);
const uiProfileSchemaUrl = new URL('platform/ui/widgets/model-editor/fullmeta-ui-profile-1.schema.json', workspaceUrl);

function expectedRuntimeCompatibility(schema) {
  const declarations = schema?.['x-fullmeta-runtime']?.compatibility;
  const capabilities = schema?.['x-fem-capabilities'];
  if (!declarations || !capabilities) throw new Error('FEM runtime compatibility declarations are missing.');
  for (const key of ['apiContract', 'capabilitySchema', 'geometryArtifactVersion', 'modelSchema', 'projectApiContract']) {
    if (!Object.hasOwn(declarations, key)) throw new Error(`FEM runtime compatibility expectation ${key} is missing.`);
  }
  return Object.fromEntries(Object.entries(declarations).map(([key, declaration]) => {
    const value = Array.isArray(declaration.capabilityPath)
      ? declaration.capabilityPath.reduce((current, part) => current?.[part], capabilities)
      : declaration.value;
    if (value === undefined || value === null || value === '') {
      throw new Error(`FEM runtime compatibility expectation ${key} is missing.`);
    }
    return [key, value];
  }));
}

function scenarioProfiles(documents) {
  const entries = [];
  const seen = new Set();
  for (const document of documents) {
    const profile = document?.['x-fem-scenario']?.profile;
    if (profile === undefined) continue;
    if (!profile || typeof profile !== 'object' || Array.isArray(profile)
      || typeof profile.id !== 'string' || !profile.id.trim()) {
      throw new Error('Each FEM scenario profile declaration must contain a nonblank ID.');
    }
    if (typeof profile.name !== 'string' || !profile.name.trim()) {
      throw new Error(`FEM scenario profile ${profile.id} must declare a nonblank display name.`);
    }
    if (seen.has(profile.id)) throw new Error(`FEM scenario profile ${profile.id} is declared more than once.`);
    seen.add(profile.id);
    entries.push({ document, profile });
  }
  if (!entries.length) throw new Error('The FEM schema graph has no scenario profile declarations.');
  return entries;
}

function starterProfilesFromSchema(schema, entries) {
  const familyByDimension = schema?.['x-fem-capabilities']?.mesh?.familyByDimension;
  return Object.fromEntries(entries.map(({ document, profile }) => {
    const id = profile.id;
    const declaredPhysics = profile.physics;
    const runtime = document?.['x-fem-capabilities']?.study?.preparationByProfile?.[id];
    const solverSettings = document?.['x-fem-capabilities']?.study?.solverSettingsByProfile?.[id];
    const formulation = solverSettings?.defaults?.fieldFormulation || profile.formulations?.[0];
    const elementFamily = familyByDimension?.[String(profile.dimension)];
    if (!Number.isInteger(profile.dimension) || !declaredPhysics || !formulation || !elementFamily) {
      throw new Error(`FEM profile ${id} is missing its dimension, Physics, formulation, or mesh-family declaration.`);
    }
    const wave = runtime?.kind === 'frequencyPlan' || runtime?.kind === 'configuredFrequencyRequest';
    const stationary = runtime?.kind === 'scenario';
    if (!wave && !stationary) throw new Error(`FEM profile ${id} has no supported Study preparation declaration.`);
    return [id, {
      dimension: profile.dimension,
      physics: declaredPhysics,
      formulation,
      elementFamily,
      wave,
      studyType: stationary ? 'stationary' : null,
    }];
  }));
}

function profileEntryForModel(entries, model) {
  const profileId = model?.scenario?.profileId;
  return entries.find(entry => entry.profile.id === profileId) || null;
}

function editorOptionsFor(entries, model) {
  const options = profileEntryForModel(entries, model)?.document?.['x-fem-scenario']?.editorOptions;
  return options && typeof options === 'object' && !Array.isArray(options)
    ? structuredClone(options) : {};
}

function uiProfilesFrom(entries, declarations) {
  if (!declarations || typeof declarations !== 'object' || Array.isArray(declarations)) {
    throw new Error('The FEM schema must declare x-fullmeta-ui-profiles as a profile-ID map.');
  }
  const profileIds = entries.map(({ profile }) => profile.id);
  const declaredIds = Object.keys(declarations);
  const missing = profileIds.filter(id => !Object.hasOwn(declarations, id));
  const unknown = declaredIds.filter(id => !profileIds.includes(id));
  if (missing.length || unknown.length) {
    throw new Error(`FEM UI profile IDs must match the scenario schemas (missing: ${missing.join(', ') || 'none'}; unknown: ${unknown.join(', ') || 'none'}).`);
  }
  return Object.fromEntries(entries.map(({ profile }) => {
    const declaration = declarations[profile.id];
    if (!declaration || typeof declaration !== 'object' || Array.isArray(declaration)
      || Object.keys(declaration).some(key => key !== 'layout')
      || !declaration.layout || typeof declaration.layout !== 'object' || Array.isArray(declaration.layout)) {
      throw new Error(`FEM UI profile ${profile.id} must contain only a layout object.`);
    }
    const layout = declaration.layout;
    if (Object.keys(layout).some(key => !['treeWidthPx', 'graphicsHeightPercent'].includes(key))
      || !Number.isInteger(layout.treeWidthPx) || layout.treeWidthPx < 180 || layout.treeWidthPx > 420
      || !Number.isInteger(layout.graphicsHeightPercent) || layout.graphicsHeightPercent < 30 || layout.graphicsHeightPercent > 80) {
      throw new Error(`FEM UI profile ${profile.id} layout must declare treeWidthPx (180–420) and graphicsHeightPercent (30–80) as integers.`);
    }
    return [profile.id, {
      schema: 'fullmeta-ui-profile/1',
      layout: structuredClone(layout),
      behavior: {},
      copy: {},
      ui: {
        documentTitle: `FEM — ${profile.name}`,
        text: {},
        attributes: {},
        dataLabels: {},
      },
    }];
  }));
}

function quantityVectorComponentsFor(entries, { source, model, component } = {}) {
  if (source !== 'primaryFieldVariables') return [];
  const entry = profileEntryForModel(entries, model);
  const physics = model?.model?.physics || component?.physics || component;
  const formulation = physics?.formulation;
  if (!entry || typeof formulation !== 'string' || !entry.profile.formulations?.includes(formulation)) return [];
  const variables = entry.document?.['x-fem-scenario']?.fields?.formulations?.[formulation]?.fieldVariables;
  if (!Array.isArray(variables)) return [];
  return variables.map(variable => {
    if (typeof variable?.symbol !== 'string' || !variable.symbol.trim()
      || typeof variable?.unit !== 'string' || !variable.unit.trim()) {
      throw new Error(`FEM profile ${entry.profile.id} formulation ${formulation} has an invalid primary field descriptor.`);
    }
    return { symbol: variable.symbol, label: variable.symbol, unit: variable.unit };
  });
}

export async function createApplicationPackage(options = {}) {
  // The Schema Editor calls this factory directly with a routing-only probe;
  // it has not necessarily invoked the runtime package loader first.
  if(options.model&&options.modelLocation)await activateModelSchema(options.model,options.modelLocation);
  else if(options.schemaUrl)await loadModelSchema(options.schemaUrl,'fem');
  const { schema, sources, documents } = activeModelSchema();
  const profileEntries = scenarioProfiles(documents);
  const uiProfileSchema = await loadWorkspaceJson(uiProfileSchemaUrl, workspaceUrl);
  if (uiProfileSchema?.properties?.schema?.const !== 'fullmeta-ui-profile/1') {
    throw new Error('The shared Model Editor UI profile schema is missing or has an unsupported version.');
  }
  const uiProfiles = uiProfilesFrom(profileEntries, schema['x-fullmeta-ui-profiles']);
  const profileIds = new Set(profileEntries.map(entry => entry.profile.id));
  const schemaUrl = sources[0].url;
  const schemaPath = decodeURIComponent(schemaUrl.pathname.slice(workspaceUrl.pathname.length));
  const starterProfiles = starterProfilesFromSchema(schema, profileEntries);
  const factory = args => createModelDocument(args, { applicationId: 'fem', schemaPath, profiles: starterProfiles });
  const defaultProfileId = schema['x-fullmeta-ui-modelEditor']?.scenario?.defaultProfileId;
  if (typeof defaultProfileId !== 'string' || !profileIds.has(defaultProfileId)) {
    throw new Error('The FEM Model Editor default profile must identify a declared scenario profile.');
  }
  const profileMetadata = profileEntries.map(({ profile }) => ({ ...profile, status: 'available' }));
  const launcherProfiles = profileMetadata.map(profile => ({
    id: profile.id,
    name: profile.name || profile.id,
    description: profile.description || profile.name || profile.id,
    status: profile.status,
  }));
  const modelLocation = options.modelLocation || new URL('projects/femDraft/fem_model.json', workspaceUrl);
  const model = options.model || factory({ modelId: 'femDraft', profileId: defaultProfileId, folder: 'projects/femDraft' });
  const base = await createSchemaProjectPackage({ model, modelLocation });
  if (base.id !== 'fem' || base.schemaPath !== schemaPath) {
    throw new Error('The opened model and selected FEM schema do not match.');
  }
  const modelEditorSchema = {
    ...base.modelEditorSchema,
    modelUnitChoices: structuredClone(schema.properties.model.properties.parameters.items.properties.unit.enum),
    resolveModelEditorSchema(...args) {
      return orderSettingsProperties(base.modelEditorSchema.resolveModelEditorSchema(...args));
    },
    validateFullMetaModelWithRules(value){
      const errors=base.modelEditorSchema.validateFullMetaModelWithRules(value);
      return errors.length?errors:checkedFieldEditErrors(value,checkedFieldState);
    },
    editorOptionsForModel(value) { return editorOptionsFor(profileEntries, value); },
    modelEditorSchemaForPath(value, path, schemaOptions) {
      // Named Physics conditions are additional properties, not declared
      // interface fields. Keep the real parent schema for tree projection.
      if (Array.isArray(path) && path.length === 2 && path[0] === 'model' && path[1] === 'physics') {
        return schemaAtPath(value, path, schemaOptions, base.modelSchema);
      }
      return projectSettingsSchema({model:value,path,schema:base.modelSchema,
        scenario:profileEntryForModel(profileEntries,value)?.document?.['x-fem-scenario'],
        getSchema:target=>base.modelEditorSchema.modelEditorSchemaForPath(value,target,schemaOptions)});
    },
  };
  const definitionSymbol = (document, prefix) => uniqueDefinitionSymbol(document, prefix,
    profileEntryForModel(profileEntries, document)?.document?.['x-fem-scenario'], schema['x-fem-capabilities']);
  const {validateDocument: _draftValidation, ...editingBase}=base;
  const actionBindings=modelEditorActionBindings(schema['x-fullmeta-ui-modelEditor'],schema['x-fullmeta-ui-graphicsInterfaces']);
  let actionServices;
  let checkedFieldState;
  let historyHost;
  const history=createStudyHistory({services:()=>actionServices,render:view=>renderStudyHistory(historyHost,view)});
  const readArtifact=options.resolveArtifact||((url,requestOptions)=>{
    if(!actionServices?.network?.requestJson)throw new Error('The editor artifact service is unavailable.');
    return actionServices.network.requestJson(url,requestOptions);
  });
  return {
    ...editingBase,
    saveDocument: saveFemDocument,
    kind: 'application',
    requiresServerCompatibility: true,
    engineCapabilities: structuredClone(schema['x-fem-capabilities']),
    graphicsConsole: structuredClone(schema['x-fullmeta-ui-graphicsConsole']),
    graphicsControlAvailability({model,inputs,scene}){
      const hasMesh=Boolean(scene?.onSelect?inputs?.fieldData?.isCurrent&&inputs.fieldData.data?.cells?.length:inputs?.previewMesh?.cells?.length);
      const reason=hasMesh?'':'Build a mesh or load a checked field result first.';
      return {geometry:{disabled:!model?.model?.geometry?.features?.length,reason:model?.model?.geometry?.features?.length?'':'Add geometry features first.'},mesh:{disabled:!hasMesh,reason},edges:{disabled:!hasMesh,reason}};
    },
    graphicsLayerHandlers: {selectionHighlight:({input})=>({focus:input||null,kind:'fem-selection'})},
    renderPreview(context) {
      return renderPreview({ ...context, expressionPolicy: bindRuntimePolicy(context?.model ?? null, {}, schema['x-fullmeta-expressionRuntime'].real, schema) });
    },
    mountEditorPanels(context){const dispose=mountEditorPanels(context);historyHost=context.settingsView.querySelector('[data-fem-study-history]');return ()=>{history.dispose();historyHost=null;dispose();};},
    renderResultSettings(context){
      checkedFieldState=context.fieldPlotState;if(updateCheckedFieldView(context.model,checkedFieldState))context.drawPreview();renderResultSettings(context);
      const path=context.selectedPath,study=Array.isArray(path)&&path.length===3&&path[0]==='model'&&path[1]==='study'?context.valueAt(path):null;
      if(study?.id&&actionServices)history.select({studyId:study.id,...actionServices.resolveInputs({folder:'document.folder',file:'document.file'})});else history.clear();
    },
    settingsFieldChange(context){return changeStudySettings({...context,schema});},
    engineChoiceRecords(context){return studySettingsChoiceRecords({...context,schema})??fieldChoiceRecords({...context,modelSchema:schema,state:checkedFieldState});},
    referenceOptions(context){return femReferenceOptions(context,base.referenceOptions);},
    modelEditorSchema,
    editorDomain: createEditorDomain({ schema, validate: value => modelEditorSchema.validateFullMetaModelWithRules(value) }),
    uiProfileSchema,
    uiProfiles,
    scenarioMetadata: { profiles: profileMetadata },
    modelEditorUi: structuredClone(base.modelSchema['x-fullmeta-ui-modelEditor'] || base.modelEditorUi),
    runtimeCompatibility: () => expectedRuntimeCompatibility(base.modelSchema),
    resultFields,
    parsePreview: parseMeshArtifact,
    restorePreview,
    resultAdapters: createApplicationResultAdapters({modelSchema:schema,resolveArtifact:readArtifact}),
    resultPresentation: createResultPresentation({modelSchema:schema,readArtifact}),
    preserveModelDocument,
    projectModel,
    quantityVectorComponents(request) { return quantityVectorComponentsFor(profileEntries, request); },
    selectionAdapter,
    selectionActions({ loadResultAction }) { return { loadResult: loadResultAction }; },
    createDynamicPropertySets() {
      return { femMaterialProperties: ({ model, schema: materialSchema }) =>
        materialPropertyDescriptors(profileEntryForModel(profileEntries, model)?.document?.['x-fem-scenario'], materialSchema) };
    },
    editorFactories: { ...base.editorFactories, uniqueDefinitionSymbol: definitionSymbol },
    // Complete acceptance matrices remain separate from these runtime bindings.
    actionBindings,
    actions(services,bindings){actionServices=services;const actions=createApplicationActions({services,bindings});for(const binding of bindings)if(['fem.studyRun','fem.clearStudyResults'].includes(binding.execution.operation)){const run=actions[binding.id];actions[binding.id]=async(...args)=>{try{return await run(...args);}finally{await history.refresh();}};}return actions;},
    actionAvailability({loaded,busy,saving,switching,serverReady,repairMode,jsonDraftInvalid,modelValid,pendingSettingsValid,hasModelFile}){
      const reason=!serverReady?'Confirm server compatibility before running FEM actions.':busy||saving||switching?'Finish the active operation first.':!loaded||!hasModelFile?'Open a saved FEM model.':repairMode||jsonDraftInvalid||!pendingSettingsValid?'Repair the active document draft.':!modelValid?'Validate and repair the FEM model.':'';
      return {actions:Object.fromEntries(actionBindings.filter(binding=>binding.id!=='document.validate').map(binding=>[binding.id,{disabled:Boolean(reason),title:reason||binding.label}]))};
    },
    resolveResourceUrl(path){
      const url=new URL(path,new URL('./',import.meta.url));
      if(url.origin!==workspaceUrl.origin||!url.pathname.startsWith(new URL('./',import.meta.url).pathname))throw new Error('FEM resources must stay inside the application folder.');
      return url;
    },
    treeBehavior: { ...base.treeBehavior, ...geometryTreeBehavior },
    treeDescriptorForModel(document, descriptor = base.modelSchema['x-fullmeta-ui-tree']) {
      return resolveTreeDescriptor(descriptor, {
        model: document,
        editorOptions: editorOptionsFor(profileEntries, document),
      });
    },
    treeActionPlans(context) {
      const physicsEntries = schema['x-fullmeta-ui-tree']?.actionMenus?.find(rule =>
        rule.when?.scope === 'item' && JSON.stringify(rule.when.path) === JSON.stringify(['model', 'physics']))?.entries || [];
      const conditionSpecs = new Map(physicsEntries.filter(entry => entry.conditionType && entry.itemName).map(entry => [entry.action, entry]));
      const physicsProfile = profileEntryForModel(profileEntries, context.model)?.document?.['x-fem-physics'];
      const conditionDeletion = target => ({ model: context.model, target,
        validate: value => validateModelRules(value, {}, schema), maximumErrors: schema['x-fullmeta-rules']?.maximumErrors });
      const profileId = context.model?.scenario?.profileId;
      const preparation = profileEntryForModel(profileEntries, context.model)?.document?.['x-fem-capabilities']?.study?.preparationByProfile?.[profileId]
        || schema['x-fem-capabilities']?.study?.preparationByProfile?.[profileId];
      const femStudyKind = preparation?.kind === 'scenario' ? 'stationary'
        : ['frequencyPlan', 'configuredFrequencyRequest'].includes(preparation?.kind) ? 'wave' : null;
      const plan = createSchemaTreeActionPlans({ ...context,
        capabilities: { ...context.capabilities, femStudyKind }, descriptor: schema['x-fullmeta-ui-tree'],
        itemFactories: { ...base.editorFactories,
          femIntegrationOperator: integrationTreeItem,
          femMeshSizeControl: meshSizeTreeItem,
          femPeriodicPair: periodicPairTreeItem,
          femStudy: studyTreeItem,
          femParameterSweep: sweepTreeItem,
          femFieldPlot: helpers => fieldPlotTreeItem({ ...helpers,
            profile: profileEntryForModel(profileEntries, helpers.model)?.document?.['x-fem-scenario'] }),
          femGlobalPlot: globalPlotTreeItem,
          femMaterial: helpers => materialTreeItem({ ...helpers,
            profile: profileEntryForModel(profileEntries, helpers.model)?.document?.['x-fem-scenario'] }),
          femDefinition: helpers => definitionTreeItem({ ...helpers,
            profile: profileEntryForModel(profileEntries, helpers.model)?.document?.['x-fem-scenario'], capabilities: schema['x-fem-capabilities'] }),
          femSelection: helpers => selectionTreeItem({ ...helpers,
            domainFeatureTypes: schema['x-fem-modelValidation'].geometry.domainFeatureTypes }) } });
      return { ...plan, selectItemOnContextMenu: true,
        itemTitle(item, path) {
          const selectionPath = schema['x-fullmeta-ui-modelEditor']?.roles?.selection;
          const isSelection = Array.isArray(selectionPath) && path?.length === selectionPath.length + 1
            && selectionPath.every((part,index) => part === path[index]) && Number.isInteger(path.at(-1));
          return isSelection ? schema['x-fullmeta-ui-modelEditor'].selectionNames?.[context.model?.scenario?.dimension]?.[item?.entity]
            || plan.itemTitle(item,path) : plan.itemTitle(item,path);
        },
        defaultArrayItem(arrayPath) {
          const insertion = (schema['x-fullmeta-ui-tree']?.insertions || []).find(entry =>
            entry.factory === 'femDefinition' && entry.action !== 'add-default'
            && JSON.stringify(entry.when?.path) === JSON.stringify(arrayPath));
          return insertion ? plan.insertionForAction(insertion.action, { arrayPath })?.item ?? null
            : plan.defaultArrayItem(arrayPath);
        },
        menuTarget(...args) {
          const target = plan.menuTarget(...args);
          const guard = target && guardPhysicsCondition(context.model, target);
          const rootGuard = target && guardPhysicsRoot(context.model, target);
          const arrayGuard = target && guardArrayItem(context.model, target);
          return guard ? { ...target, femConditionGuard: guard }
            : rootGuard ? { ...target, femPhysicsGuard: rootGuard }
              : arrayGuard ? { ...target, femArrayGuard: arrayGuard } : target;
        },
        actionAvailability(action, target) {
          if (action === 'fem.delete-array-item') {
            const prepared = prepareArrayDeletion(context.model, target);
            return { disabled: !prepared.document, reason: prepared.reason || '' };
          }
          if (action === 'fem.toggle-condition') return conditionToggleAvailability(context.model, target);
          if (action === 'fem.duplicate-array-item') return duplicateArrayAvailability(context.model, target, context.cloneItem);
          if (action === 'fem.delete-condition') {
            const prepared = prepareConditionDeletion(conditionDeletion(target));
            return { disabled: !prepared.document, reason: prepared.reason || '' };
          }
          if (action === 'fem.duplicate-condition') {
            const available = conditionToggleAvailability(context.model, target);
            if (available.disabled) return available;
            if (context.model.model.physics[target.itemPath[2]].type === 'periodicCondition')
              return { disabled: true, reason: 'Periodic conditions cannot be duplicated.' };
            if (typeof context.makeId !== 'function') return { disabled: true, reason: 'A unique ID generator is required.' };
            return available;
          }
          if (conditionSpecs.has(action)) {
            const prepared = preparePhysicsCondition({ model: context.model, target, spec: conditionSpecs.get(action), profile: physicsProfile, makeId: context.makeId });
            return { disabled: !prepared.condition, reason: prepared.reason || '' };
          }
          if (action.startsWith('add-')) {
            const insertion = plan.insertionForAction(action, target);
            if (action === 'add-integration-operator' && !insertion) {
              return { disabled: true, reason: 'Add an eligible named selection before creating an integration operator.' };
            }
            if (action === 'add-mesh-size-control' && !insertion) {
              return { disabled: true, reason: 'Add an unused eligible named selection and set the maximum mesh size first.' };
            }
            if (action === 'add-periodic-pair' && !insertion) {
              return { disabled: true, reason: 'Add two distinct boundary selections before creating a periodic pair.' };
            }
            if (['add-harmonic-study', 'add-stationary-study'].includes(action) && !insertion) {
              return { disabled: true, reason: 'The active profile and Physics interface must be valid before adding a Study.' };
            }
            if (action === 'add-parameter-sweep' && !insertion) {
              return { disabled: true, reason: 'Add a parameter and link this harmonic Study to the active Physics interface first.' };
            }
            if (action === 'add-domain-field-plot' && !insertion) {
              return { disabled: true, reason: 'Choose a supported Physics formulation with a declared primary field first.' };
            }
            if (action === 'add-global-variable-plot' && !insertion) {
              return { disabled: true, reason: 'Add a harmonic Study with a parameter sweep and a global variable first.' };
            }
            if (action === 'add-material-assignment' && !insertion) {
              return { disabled: true, reason: 'Select an unassigned Physics domain and provide the active profile material defaults first.' };
            }
            const reason = geometryInsertionIssue(context.model, insertion);
            if (reason) return { disabled: true, reason };
          }
          return plan.actionAvailability?.(action, target);
        },
        executeAction(action, target, input) {
          if (action === 'fem.delete-array-item') return deleteArrayItem(context.model, target);
          if (action === 'fem.toggle-condition') return togglePhysicsCondition(context.model, target);
          if (action === 'fem.duplicate-array-item') return duplicateArrayItem({ model: context.model, target,
            cloneItem: context.cloneItem, insertItem: modelWithArrayItemDraft });
          if (action === 'fem.delete-condition') return deletePhysicsCondition(conditionDeletion(target));
          if (action === 'fem.duplicate-condition') return duplicatePhysicsCondition(context.model, target, context.makeId);
          if (conditionSpecs.has(action)) return addPhysicsCondition({ model: context.model, target,
            spec: conditionSpecs.get(action), profile: physicsProfile, makeId: context.makeId });
          return plan.executeAction?.(action, target, input);
        },
      };
    },
    rootLabel(document) {
      const name = document?.metadata?.name;
      return typeof name === 'string' && name.trim() ? name : document?.metadata?.id || 'FEM';
    },
    prepareModelForEditor(source) {
      return {
        model: preserveModelDocument(source),
        profile: profileEntryForModel(profileEntries, source)?.profile || null,
        appliedDefault: null,
      };
    },
    launcher: { name: 'FEM', description: 'Finite-element simulation workspace.',
      profiles: launcherProfiles, defaultProfileId,
      profileIdProperty: 'id', profileLabelProperty: 'name', profileDescriptionProperty: 'description',
      profileStatusProperty: 'status', availableStatus: 'available' },
    createModelDocument: factory,
  };
}
