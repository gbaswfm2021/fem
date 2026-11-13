// View-only projections of the declared profile/solver catalogs. They never
// fill optional members in a model or change its structural validation schema.
import {resolveDeclaredSettings} from '../../../platform/ui/widgets/model-editor/widget.js';
import {materialPropertyDescriptors} from './material-properties.js';
const at=(value,path)=>path.reduce((node,key)=>node?.[key],value);

/** Project the declared presentation sequence after schema resolution sorts maps. */
export function orderSettingsProperties(schemaNode){
  const order=schemaNode?.['x-fullmeta-ui-behavior']?.payload?.presentation?.propertyOrder;
  if(order===undefined)return schemaNode;
  if(!Array.isArray(order)||order.some(key=>typeof key!=='string'||!key)||new Set(order).size!==order.length)
    throw new Error('Settings propertyOrder must contain distinct nonempty property names.');
  const properties=schemaNode.properties;
  if(!properties||typeof properties!=='object'||Array.isArray(properties)||order.some(key=>!Object.hasOwn(properties,key)))
    throw new Error('Settings propertyOrder must name declared object properties.');
  const keys=[...order,...Object.keys(properties).filter(key=>!order.includes(key))];
  return {...schemaNode,properties:Object.fromEntries(keys.map(key=>[key,properties[key]]))};
}

function presentation(field){
  return field['x-fullmeta-ui-behavior']||={format:'fullmeta-ui-behavior/1',context:'projectEditing',target:{kind:'field'},payload:{}};
}
function showDefault(field,value){
  if(value===undefined)return;
  field.default=structuredClone(value);
  const ui=presentation(field);
  (ui.payload.presentation||={}).showDefault=true;
}

function solverDisplayValues(model,item,capabilities,context){
  const source=(scope,path)=>({kind:'path',scope,path});
  // The shared resolver owns precedence and conditional option evaluation;
  // these paths only adapt FEM's declared catalog and Study envelope to it.
  return resolveDeclaredSettings({document:model,item:{...item,displayContext:context},capabilities,
    declaration:{profile:{idPath:['scenario','profileId'],settingsPath:['study','solverSettingsByProfile']},itemCollectionPath:['model','study'],itemIdentityKey:'id',
      choiceFields:Object.keys(context).map(key=>({key,choices:{kind:'singleton',of:source('item',['displayContext',key])},candidates:[source('item',['displayContext',key])]})),
      optionGroup:{resolution:source('capabilities',['study','solverResolution']),options:source('capabilities',['study','solverOptions']),rules:source('capabilities',['study','solverOptionRules']),
        sources:{explicit:source('item',['solver']),defaults:source('profileSettings',['defaults']),shared:source('capabilities',['study','solverDefaults'])}},
      outputGroups:[{key:'values',fields:capabilities.study.solverResolution.fields.map(field=>field.key)}]},
    schemaAtPath:()=>null,resolveSchema:value=>value});
}

function studyContext(model,study,catalog){
  const profile=catalog.study.solverSettingsByProfile[model.scenario.profileId];
  const physics=model.model.physics?.id===study.physicsId?model.model.physics:null;
  return {profile,formulation:study.discretization?.fieldFormulation??physics?.formulation??profile.defaults.fieldFormulation,
    order:study.discretization?.elementOrder??profile.defaults.elementOrder};
}

/** Called by the host for both a visible field edit and its WebMCP command. */
export function changeStudySettings({draft,path,model,selectedPath,schema}){
  if(selectedPath?.length!==3||selectedPath[0]!=='model'||selectedPath[1]!=='study'||path?.length!==2||!['solver','discretization'].includes(path[0]))return {};
  const catalog=schema['x-fem-capabilities'],{profile,formulation,order}=studyContext(model,draft,catalog);
  if(path[0]==='discretization'){
    draft.discretization={fieldFormulation:formulation,elementOrder:profile.defaults.elementOrder,...draft.discretization};
  }else{
    const resolved=solverDisplayValues(model,draft,catalog,{fieldFormulation:formulation,elementOrder:order});
    const key=path[1];
    if(key&&!resolved.options[key]?.includes(draft.solver[key]))throw new Error(`The selected ${key} is not supported by the current solver settings.`);
    // Only this deliberate edit makes the displayed solver defaults explicit.
    // A backend change also replaces incompatible dependent option values.
    draft.solver={...draft.solver,...resolved.values};
  }
  return {};
}

export function studySettingsChoiceRecords({source,model,settingsDraft,schema}){
  if(!['studySolverChoices','studyDiscretizationChoices'].includes(source?.path?.[0]))return null;
  const study=settingsDraft;
  if(!study?.physicsId)return [];
  const catalog=schema['x-fem-capabilities'],{profile,formulation,order}=studyContext(model,study,catalog);
  if(source.path[0]==='studySolverChoices'){
    const options=solverDisplayValues(model,study,catalog,{fieldFormulation:formulation,elementOrder:order}).options;
    return (options[source.path[1]]||[]).map(value=>({value,label:String(value)}));
  }
  const choices=catalog.study.discretizationChoicesByProfile[model.scenario.profileId];
  return source.path[1]==='fieldFormulation'
    ?choices.filter(choice=>!profile.fieldFormulationsFromPhysics||choice.value===model.model.physics?.formulation).map(choice=>({value:choice.value,label:choice.label}))
    :(catalog.mesh.fieldFormulations?.[formulation]?.elementOrders||[]).filter(order=>catalog.mesh.elementOrdersByProfile[model.scenario.profileId]?.includes(order)).map(value=>({value,label:String(value)}));
}

function bindChoices(field,source,key){
  const ui=presentation(field);ui.payload.widget||={type:'valueControl',settings:{}};
  ui.payload.widget.settings.choiceSource={scope:'engine',path:[source,key]};
}

export function projectSettingsSchema({model,path,schema,getSchema,scenario}){
  const profileId=model?.scenario?.profileId,catalog=schema['x-fem-capabilities'];
  if(path?.[0]!=='model')return getSchema(path);
  if(path[1]==='materials'&&Number.isInteger(path[2])){
    const root=structuredClone(getSchema(path.slice(0,3)));
    for(const field of materialPropertyDescriptors(scenario,root))root.properties[field.key].default=structuredClone(field.default);
    return path.length===3?root:at(root,path.slice(3).flatMap(key=>['properties',key]))||getSchema(path);
  }
  if(path[1]==='results'&&Number.isInteger(path[2])){
    const rootPath=path.slice(0,3),result=at(model,rootPath);
    if(result?.type==='plot.sweep'&&Array.isArray(result.datasets)){
      const root=structuredClone(getSchema(rootPath));
      const dataset=result.datasets[0];
      const sourceUnit=dataset=>dataset?.source?.type==='study'
        ?model.model.parameters?.find(item=>item.symbol===dataset.xColumn)?.unit??'1'
        :dataset?.source?.columnUnits?.[dataset?.xColumn]??'1';
      const axisUnit=dataset?.xUnit??sourceUnit(dataset);
      for(const key of ['xMin','xMax']){
        const bound=root.properties.axes?.properties?.[key];
        if(bound?.default)bound.default.unit=axisUnit;
      }
      // Resolve each existing dataset's source with the public Model API.
      // The shared recursive form then sees the actual Study/file branch.
      root.properties.datasets.prefixItems=result.datasets.map((dataset,index)=>{
        const itemPath=[...rootPath,'datasets',index],item=structuredClone(getSchema(itemPath));
        if(dataset.source&&item.properties?.source)item.properties.source=structuredClone(getSchema([...itemPath,'source']));
        if(item.properties?.xUnit)showDefault(item.properties.xUnit,sourceUnit(dataset));
        return item;
      });
      root.properties.series.prefixItems=(result.series||[]).map((series,index)=>{
        const item=structuredClone(getSchema([...rootPath,'series',index]));
        const variable=model.model.variables?.find(entry=>entry.symbol===series.column||entry.symbol+'_re'===series.column||entry.symbol+'_im'===series.column);
        const unit=dataset?.source?.type==='study'?variable?.unit:dataset?.source?.columnUnits?.[series.column];
        if(item.properties?.unit)showDefault(item.properties.unit,unit??'1');
        return item;
      });
      if(path.length===3)return root;
      if(path[3]==='datasets'&&path.length===4)return root.properties.datasets;
      if(path[3]==='datasets'&&Number.isInteger(path[4])){
        const item=root.properties.datasets.prefixItems[path[4]];
        return path.length===5?item:at(item,path.slice(5).flatMap(key=>['properties',key]))||getSchema(path);
      }
      if(path[3]==='series'&&Number.isInteger(path[4])){
        const item=root.properties.series.prefixItems[path[4]];
        return path.length===5?item:at(item,path.slice(5).flatMap(key=>['properties',key]))||getSchema(path);
      }
      return at(root,path.slice(3).flatMap(key=>['properties',key]))||getSchema(path);
    }
  }
  if(path[1]==='study'&&Number.isInteger(path[2])){
    const rootPath=path.slice(0,3),study=at(model,rootPath),profile=catalog.study.solverSettingsByProfile[profileId];
    const choices=catalog.study.discretizationChoicesByProfile?.[profileId];
    if(!study||!profile||!choices)return getSchema(path);
    const root=structuredClone(getSchema(rootPath)),properties=root.properties;
    if(!properties?.discretization?.properties)return getSchema(path);
    const physics=model.model.physics?.id===study.physicsId?model.model.physics:null;
    const formulation=study.discretization?.fieldFormulation??physics?.formulation??profile.defaults.fieldFormulation;
    const order=study.discretization?.elementOrder??profile.defaults.elementOrder;
    const eligible=profile.fieldFormulationsFromPhysics?choices.filter(choice=>choice.value===physics?.formulation):choices;
    const discretization=properties.discretization,fields=discretization.properties;
    fields.fieldFormulation.enum=eligible.map(choice=>choice.value);
    presentation(fields.fieldFormulation).payload.widget={type:'valueControl',settings:{choiceLabels:Object.fromEntries(eligible.map(choice=>[choice.value,choice.label]))}};
    fields.elementOrder.enum=(catalog.mesh.fieldFormulations?.[formulation]?.elementOrders||[]).filter(order=>catalog.mesh.elementOrdersByProfile[profileId]?.includes(order));
    for(const [key,field]of Object.entries(fields))bindChoices(field,'studyDiscretizationChoices',key);
    discretization.default={fieldFormulation:physics?.formulation??profile.defaults.fieldFormulation,elementOrder:profile.defaults.elementOrder};
    const stationary=catalog.study.preparationByProfile?.[profileId]?.kind==='scenario';
    properties.type.enum=[stationary?'stationary':'frequencyDomain'];
    const solver=properties.solver?.properties;
    if(solver){
      const resolved=solverDisplayValues(model,study,catalog,{fieldFormulation:formulation,elementOrder:order});
      for(const [key,field]of Object.entries(solver)){
        field.enum=structuredClone(resolved.options[key]);showDefault(field,resolved.values[key]);bindChoices(field,'studySolverChoices',key);
      }
    }
    return path.length===3?root:at(root,path.slice(3).flatMap(key=>['properties',key]))||getSchema(path);
  }
  if(path[1]==='mesh'&&(path.length===2||['elementFamily','gmsh'].includes(path[2]))){
    const root=structuredClone(getSchema(['model','mesh']));
    const family=catalog.mesh.familyByDimension[model.scenario.dimension];
    if(family&&root.properties?.elementFamily)root.properties.elementFamily.enum=[family];
    const order=root.properties?.gmsh?.properties?.elementOrder;
    if(order)order.enum=[1];
    return path.length===2?root:at(root,path.slice(2).flatMap(key=>['properties',key]))||getSchema(path);
  }
  return getSchema(path);
}
