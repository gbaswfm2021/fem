import { evaluatePointwiseFields, getCoreUnit } from '../../../platform/core-rust/js/api.js';

const object=value=>value!==null&&typeof value==='object'&&!Array.isArray(value);
const fail=(path,message)=>{throw new Error(`${path}: ${message}`);};
const finite=(value,path)=>{if(typeof value!=='number'||!Number.isFinite(value))fail(path,'expected a finite number.');return value;};

export function resultQuantityDescriptors(model,modelSchema) {
  const scenario=modelSchema?.$defs?.femProfiles?.[model?.scenario?.profileId]?.['x-fem-scenario'];
  const catalog=scenario?.fields?.formulations?.[model?.model?.physics?.formulation];
  if(!catalog)throw new Error('The active FEM formulation has no result field catalog.');
  const dimension=model.scenario.dimension, sources=scenario.fields.plotSources||[];
  const fields=['fieldVariables','derivedFieldVariables'].flatMap(key=>(catalog[key]||[])
    .filter(field=>!field.zeroInDimensions?.includes(dimension))
    .map(field=>({...structuredClone(field),source:'field',role:key==='fieldVariables'?'primary':'derived'})));
  if(sources.includes('pointwise'))fields.push(...(model.model.pointwiseVariables||[]).map(field=>({...structuredClone(field),source:'pointwise'})));
  if(sources.includes('material'))fields.push(...(scenario.materials?.properties||[]).filter(field=>field.plottable===true).map(field=>({...structuredClone(field),source:'material'})));
  return fields;
}

export function checkedFieldArtifactUrl(value) {
  if(typeof value!=='string'||!value.startsWith('/api/kernel/artifact?'))throw new Error('Field data must use the checked artifact route.');
  const url=new URL(value,'http://fullmeta.invalid');
  const path=url.searchParams.get('path');
  if(url.pathname!=='/api/kernel/artifact'||url.hash||url.searchParams.getAll('path').length!==1||[...url.searchParams.keys()].some(key=>key!=='path')||!path||/[\\:\0]/.test(path)||path.split('/').some(part=>!part||part==='.'||part==='..'))
    throw new Error('Field artifact URL has an invalid workspace path.');
  return url.pathname+url.search;
}

function unitScale(actual,expected,path) {
  let source,target;
  try{source=getCoreUnit(actual);target=getCoreUnit(expected);}catch(error){fail(path,error.message);}
  if(source.dimension.some((value,index)=>Math.abs(value-target.dimension[index])>1e-10))fail(path,`unit ${actual} is incompatible with ${expected}.`);
  return source.scale/target.scale;
}

function samples(field,count,unit,path,width) {
  if(!object(field)||!Array.isArray(field.real)||field.real.length!==count)fail(path,`expected ${count} real samples.`);
  const scale=unitScale(field.unit,unit,path), imaginary=Object.hasOwn(field,'imag')?field.imag:field.imaginary;
  if(imaginary!==undefined&&(!Array.isArray(imaginary)||imaginary.length!==count))fail(path,`expected ${count} imaginary samples.`);
  const read=(values,label)=>field.real.map((_,index)=>{
    if(width!==undefined){
      const row=values===undefined?Array(width).fill(0):values[index];
      if(!Array.isArray(row)||row.length!==width)fail(`${path}.${label}[${index}]`,`expected ${width} local-node samples.`);
      return row.map((value,local)=>finite(finite(value,`${path}.${label}[${index}][${local}]`)*scale,path));
    }
    return finite(finite(values===undefined?0:values[index],`${path}.${label}[${index}]`)*scale,path);
  });
  return {...structuredClone(field),unit,real:read(field.real,'real'),imag:read(imaginary,'imag')};
}

/** Normalize native data for display, without changing the artifact or project. */
export function normalizeFieldArtifact(raw,{model,modelSchema,action,resolved={},artifactPath='field artifact'}={}) {
  if(!object(raw))fail(artifactPath,'expected a field object.');
  const hasTets=Array.isArray(raw.tetrahedra)&&raw.tetrahedra.length>0;
  const hasTriangles=Array.isArray(raw.triangles)&&raw.triangles.length>0;
  const dimension=raw.dimension??(hasTets?3:hasTriangles?2:null);
  if(![2,3].includes(dimension)||dimension!==model?.scenario?.dimension||dimension===2&&hasTets||dimension===3&&!hasTets)
    fail(artifactPath,'field topology and model dimension disagree or are ambiguous.');
  if(!Array.isArray(raw.nodes)||raw.nodes.length<dimension+1)fail(artifactPath,'field has no usable mesh nodes.');
  // Native triangular fields also use the mesher's [x,y,0] representation.
  // Validate the full frame before projecting it to the two display axes.
  const coordinateWidth=raw.coordinateOrder?.length??raw.nodes[0]?.length;
  if(coordinateWidth!==dimension&&!(dimension===2&&coordinateWidth===3))fail(artifactPath,'invalid coordinate width.');
  const frameAxes=['x','y','z'].slice(0,coordinateWidth),axes=frameAxes.slice(0,dimension);
  const order=raw.coordinateOrder??frameAxes,origin=raw.origin??Array(coordinateWidth).fill(0);
  if(!Array.isArray(order)||order.length!==coordinateWidth||new Set(order).size!==coordinateWidth||frameAxes.some(axis=>!order.includes(axis)))fail(artifactPath,'invalid coordinate order.');
  if(!Array.isArray(origin)||origin.length!==coordinateWidth)fail(artifactPath,'invalid coordinate origin.');
  origin.forEach((value,index)=>finite(value,`${artifactPath}.origin[${index}]`));
  const nodes=raw.nodes.map((node,index)=>{
    if(!Array.isArray(node)||node.length!==coordinateWidth)fail(`${artifactPath}.nodes[${index}]`,`expected ${coordinateWidth} coordinates.`);
    node.forEach((value,slot)=>finite(value,`${artifactPath}.nodes[${index}][${slot}]`));
    if(dimension===2&&coordinateWidth===3&&node[order.indexOf('z')]+origin[order.indexOf('z')]!==0)fail(`${artifactPath}.nodes[${index}]`,'two-dimensional field nodes must lie at z=0.');
    return axes.map(axis=>{const slot=order.indexOf(axis);return finite(finite(node[slot],`${artifactPath}.nodes[${index}]`)+origin[slot],artifactPath);});
  });
  const topology=dimension===3?raw.tetrahedra:raw.triangles,width=dimension+1;
  if(!Array.isArray(topology)||!topology.length)fail(artifactPath,'field contains no cells.');
  const cells=topology.map((cell,index)=>{
    const indices=Array.isArray(cell)?cell:cell?.nodes;
    if(!Array.isArray(indices)||indices.length!==width||new Set(indices).size!==width||indices.some(node=>!Number.isInteger(node)||node<0||node>=nodes.length))fail(`${artifactPath}.cells[${index}]`,'invalid node references.');
    return [...indices];
  });
  const cellMaterialTags=topology.map((cell,index)=>{
    if(!object(cell)||cell.materialTag===undefined)return null;
    if(!Number.isSafeInteger(cell.materialTag))fail(`${artifactPath}.cells[${index}].materialTag`,'expected an integer material tag.');
    return cell.materialTag;
  });
  if(raw.studyId!==undefined&&resolved.studyId!==undefined&&raw.studyId!==resolved.studyId)fail(artifactPath,'field belongs to another Study.');
  if(raw.runId!==undefined&&resolved.runId!==undefined&&raw.runId!==resolved.runId)fail(artifactPath,'field belongs to another run.');
  if(resolved.modelFreshness!==undefined&&resolved.modelFreshness!=='current')fail(artifactPath,'field belongs to a stale model snapshot.');
  const stationary=model.scenario.physics==='thermal';
  const frequencyHz=raw.frequencyHz??(raw.frequencyGHz===undefined?undefined:raw.frequencyGHz*1e9);
  if(!stationary&&(!(finite(frequencyHz,`${artifactPath}.frequencyHz`)>0)))fail(artifactPath,'wave frequency must be positive.');
  if(!stationary&&resolved.frequencyGHz!==undefined&&Math.abs(frequencyHz-resolved.frequencyGHz*1e9)>Math.max(1,Math.abs(frequencyHz))*1e-10)fail(artifactPath,'field frequency differs from the selected case.');
  const quantities=resultQuantityDescriptors(model,modelSchema),fields={},cellFields={};
  const catalog=modelSchema.$defs.femProfiles[model.scenario.profileId]['x-fem-scenario'].fields.formulations[model.model.physics.formulation];
  const solverFields=['fieldVariables','derivedFieldVariables','zeroFieldVariables'].flatMap(key=>catalog[key]||[]);
  for(const descriptor of solverFields){
    if(raw.fields?.[descriptor.symbol])fields[descriptor.symbol]=samples(raw.fields[descriptor.symbol],nodes.length,descriptor.unit,`${artifactPath}.fields.${descriptor.symbol}`);
    else if((catalog.zeroFieldVariables||[]).some(field=>field.symbol===descriptor.symbol)||descriptor.zeroInDimensions?.includes(dimension))
      fields[descriptor.symbol]={unit:descriptor.unit,real:Array(nodes.length).fill(0),imag:Array(nodes.length).fill(0)};
  }
  for(const descriptor of quantities){
    const symbol=descriptor.symbol;
    // Native material samples are SI values keyed by the schema's material
    // symbols; unlike nodal fields they may omit the redundant unit member.
    // An explicitly supplied unit must still pass dimensional validation.
    if(descriptor.source==='material'&&raw.materialFieldsByCell?.[symbol])cellFields[symbol]=samples({unit:descriptor.unit,...raw.materialFieldsByCell[symbol]},cells.length,descriptor.unit,`${artifactPath}.materialFieldsByCell.${symbol}`,width);
  }
  const requested=quantities.find(quantity=>quantity.symbol===action?.quantity?.component);
  if(!requested)fail(artifactPath,'requested component is unavailable for this formulation.');
  if(requested.source==='pointwise'||quantities.some(quantity=>quantity.source==='pointwise')&&resolved.fieldEvaluation){
    const evaluation=resolved.fieldEvaluation;
    if(!Array.isArray(evaluation?.definitions)||!object(evaluation.parameters))fail(artifactPath,'saved pointwise definitions and parameter values are missing.');
    const result=evaluatePointwiseFields({definitions:evaluation.definitions,parameters:evaluation.parameters,frequencyHz:frequencyHz??0,nodes,fields,cells,materialFieldsByCell:cellFields});
    for(const descriptor of quantities.filter(quantity=>quantity.source==='pointwise')){
      if(result.fields[descriptor.symbol])fields[descriptor.symbol]=samples({...result.fields[descriptor.symbol],unit:descriptor.unit},nodes.length,descriptor.unit,`${artifactPath}.pointwise.${descriptor.symbol}`);
      if(result.cellFields[descriptor.symbol])cellFields[descriptor.symbol]=samples({...result.cellFields[descriptor.symbol],unit:descriptor.unit},cells.length,descriptor.unit,`${artifactPath}.pointwiseCells.${descriptor.symbol}`,width);
    }
  }
  if(!fields[requested.symbol]&&!cellFields[requested.symbol])fail(artifactPath,`requested component ${requested.symbol} has no samples.`);
  if(!['real','imaginary','magnitude'].includes(action.quantity.part))fail(artifactPath,'unsupported complex field part.');
  const availableQuantities=quantities.filter(quantity=>fields[quantity.symbol]||cellFields[quantity.symbol]);
  return {dimension,nodes,cells,cellMaterialTags,fields,cellFields,quantities:availableQuantities,quantity:structuredClone(action.quantity),unit:requested.unit,
    stationary,...(!stationary?{frequencyHz,frequencyGHz:frequencyHz/1e9}:{}),runId:resolved.runId,studyId:resolved.studyId,artifactPath};
}

export function createApplicationResultAdapters({resolveArtifact,modelSchema}) {
  return {forModel(model){
    const quantities=resultQuantityDescriptors(model,modelSchema);
    return {quantities,isFieldAction:action=>action?.type==='field',pointLabel(state,{formatFrequency}={}){
      return state?.stationary?'Stationary':typeof formatFrequency==='function'?formatFrequency(state?.frequencyGHz):`${state?.frequencyGHz} GHz`;
    },normalizeField:(raw,options)=>normalizeFieldArtifact(raw,{...options,model,modelSchema}),async loadField(resolved,action){
      if(typeof resolveArtifact!=='function')throw new Error('Checked artifact access is unavailable.');
      const url=checkedFieldArtifactUrl(resolved.url);const raw=await resolveArtifact(url);
      return normalizeFieldArtifact(raw,{model,modelSchema,action,resolved,artifactPath:resolved.path||url});
    }};
  }};
}
