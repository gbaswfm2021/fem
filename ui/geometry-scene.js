import { evaluateQuantity,getCoreUnit,resolveParameterTable } from '../../../platform/core-rust/js/api.js';
import { boxSolid,circlePoints,cylinderSolid,combineSolids,polygon,solidOutlines } from './solid-geometry.js';
import { meshPreviewSurfaces,meshSurfaceStyle } from '../js/mesh-artifact.js';
import { namedSelectionPrimitives } from './selection-cad.js';
const selectionPaintCache=new WeakMap();

const domainTypes=new Set(['rectangle','circle','box','cylinder','union','intersection']);
const add=(a,b)=>a.map((value,index)=>value+b[index]);
const fail=(path,message)=>{throw new Error(`${path}: ${message}`);};
const magnitude=points=>points.reduce((maximum,point)=>point.reduce((value,coordinate)=>Math.max(value,Math.abs(coordinate)),maximum),Number.MIN_VALUE);

/** Reuse CAD during camera-only repaint, keyed by the actual geometry inputs. */
export function createGeometryProjector(){
  let signature,projection;
  return (model,options={})=>{
    const key=JSON.stringify([model?.scenario?.dimension,model?.model?.parameters,model?.model?.geometry,options.expressionPolicy]);
    if(key===signature)return projection;
    const candidate=projectGeometry(model,options);signature=key;projection=candidate;return candidate;
  };
}

export function projectGeometry(model,{expressionPolicy}={}) {
  const dimension=model?.scenario?.dimension;
  if(![2,3].includes(dimension))fail('/scenario/dimension','unsupported geometry dimension.');
  const parameters=resolveParameterTable(model.model.parameters||[],expressionPolicy?.builtinSymbols||[],expressionPolicy?.allowedFunctions||[],'parameters',expressionPolicy),byId=new Map(),diagnostics=[];
  const quantity=(value,path)=>{
    const unit=getCoreUnit(value?.unit);
    if(unit.dimension.some((value,index)=>value!==(index===0?1:0)))fail(path,'geometry requires length units.');
    const number=evaluateQuantity(value,parameters,path,expressionPolicy?.allowedFunctions||[],expressionPolicy);if(!Number.isFinite(number))fail(path,'nonfinite geometry coordinate.');return number;
  };
  const vector=(value,path,width=dimension)=>{
    if(!Array.isArray(value)||value.length!==width)fail(path,`expected ${width} length components.`);
    return value.map((quantityValue,index)=>quantity(quantityValue,`${path}/${index}`));
  };
  const positive=(value,path)=>{const number=quantity(value,path);if(!(number>0))fail(path,'length must be positive.');return number;};
  const build=(feature,path,depth=0)=>{
    if(depth>128)fail(path,'geometry nesting exceeds 128 levels.');
    if(typeof feature?.id!=='string'||!feature.id||byId.has(feature.id))fail(`${path}/id`,'feature ID is missing or ambiguous.');
    const entry={id:feature.id,path,type:feature.type,faces:[],curves:[],children:[]};byId.set(feature.id,entry);
    const three=point=>dimension===2?[...point,0]:point;
    if(['union','intersection'].includes(feature.type)){
      if(!Array.isArray(feature.features)||feature.features.length<2)fail(path,'a domain group needs at least two children.');
      entry.children=feature.features.map((child,index)=>{if(!domainTypes.has(child.type))fail(`${path}/features/${index}`,'a group child must be domain geometry.');return build(child,`${path}/features/${index}`,depth+1);});
      const points=entry.children.flatMap(child=>child.faces.flatMap(face=>face.vertices));
      const epsilon=magnitude(points)*1e-12;
      entry.faces=entry.children.slice(1).reduce((faces,child)=>combineSolids(faces,child.faces,feature.type,epsilon),entry.children[0].faces);
    }else if(feature.type==='rectangle'||feature.type==='box'){
      const width=feature.type==='rectangle'?2:3;if(dimension!==width)fail(path,'primitive dimension differs from the model.');
      let position=vector(feature.position,`${path}/position`),size=vector(feature.size,`${path}/size`);
      if(size.some(value=>value<=0))fail(`${path}/size`,'all sizes must be positive.');
      if(feature.positionMode==='center')position=position.map((value,index)=>value-size[index]/2);
      entry.faces=boxSolid(three(position),dimension===2?[...size,Math.max(...size)]:size,feature.id);
    }else if(feature.type==='circle'||feature.type==='cylinder'){
      if(feature.type==='circle'&&dimension!==2||feature.type==='cylinder'&&dimension!==3)fail(path,'primitive dimension differs from the model.');
      const base=three(vector(feature.type==='circle'?feature.center:feature.base,`${path}/${feature.type==='circle'?'center':'base'}`));
      const radius=positive(feature.radius,`${path}/radius`);
      if(feature.type==='cylinder'&&!(Math.hypot(...(feature.axis||[]))>1e-14))fail(`${path}/axis`,'axis norm must exceed 1e-14.');
      entry.faces=cylinderSolid(base,feature.type==='circle'?[0,0,1]:feature.axis,radius,feature.type==='circle'?2*radius:positive(feature.height,`${path}/height`),feature.id);
    }else if(feature.type==='lineSegment'){
      const start=three(vector(feature.start,`${path}/start`)),end=three(vector(feature.end,`${path}/end`));
      if(start.every((value,index)=>Math.abs(value-end[index])<=1e-14))fail(path,'line endpoints are coincident.');
      entry.curves=[{points:[start,end],closed:false}];
    }else if(feature.type==='circleCurve'||feature.type==='diskSurface'){
      if(feature.type==='diskSurface'&&dimension!==3)fail(path,'a disk surface requires 3D.');
      const points=circlePoints(three(vector(feature.center,`${path}/center`)),dimension===2?[0,0,1]:feature.normal||[0,0,1],positive(feature.radius,`${path}/radius`));
      entry.curves=[{points,closed:true}];
      if(feature.type==='diskSurface')entry.sheet=polygon(points,`${feature.id}:sheet`);
    }else if(feature.type==='planeRectangle'){
      if(dimension!==3)fail(path,'a planar rectangle requires 3D.');
      const origin=vector(feature.origin,`${path}/origin`),u=vector(feature.uVector,`${path}/uVector`),v=vector(feature.vVector,`${path}/vVector`);
      const points=[origin,add(origin,u),add(add(origin,u),v),add(origin,v)];entry.sheet=polygon(points,`${feature.id}:sheet`);
      if(!entry.sheet)fail(path,'the rectangle spanning vectors are dependent.');entry.curves=[{points,closed:true}];
    }else fail(`${path}/type`,`unsupported geometry type ${feature.type}.`);
    if(domainTypes.has(feature.type)&&!entry.faces.length)diagnostics.push({path,message:'The composed geometry has no nonempty domain.'});
    entry.outlines=solidOutlines(entry.faces,magnitude(entry.faces.flatMap(face=>face.vertices))*1e-12).filter(line=>dimension===3||line.every(point=>point[2]===0));
    return entry;
  };
  const features=(model.model.geometry?.features||[]).map((feature,index)=>build(feature,`/model/geometry/features/${index}`));
  if(!features.length)diagnostics.push({path:'/model/geometry/features',message:'The model has no geometry features.'});
  const points=features.flatMap(feature=>[...feature.faces.flatMap(face=>face.vertices),...feature.curves.flatMap(curve=>curve.points)]).filter(point=>dimension===3||point[2]===0);
  const bounds=Array.from({length:dimension},(_,axis)=>points.reduce((range,point)=>({minimum:Math.min(range.minimum,point[axis]),maximum:Math.max(range.maximum,point[axis])}),{minimum:Infinity,maximum:-Infinity}));
  return {dimension,features,byId,bounds,parameters,expressionPolicy,diagnostics};
}

export function geometryPrimitives(projection,features=projection.features,{highlight=false}={}) {
  const accent='#48d7b2',result=[],dimension=projection.dimension;
  for(const feature of features){
    const lines=feature.outlines;
    for(const points of lines)result.push({type:'polyline',points:points.map(point=>point.slice(0,dimension)),stroke:highlight?accent:'#8ea4b7',opacity:highlight?1:.72,lineWidth:highlight?2.8:1.35,featureId:feature.id});
    for(const curve of feature.curves)result.push({type:'polyline',points:curve.points.map(point=>point.slice(0,dimension)),closed:curve.closed,stroke:highlight?accent:'#8ea4b7',opacity:highlight?1:.72,lineWidth:highlight?2.8:1.35,featureId:feature.id});
    if(highlight)for(const face of [...feature.faces,...(feature.sheet?[feature.sheet]:[])]){
      if(dimension===2&&!face.vertices.every(point=>point[2]===0))continue;
      for(let index=1;index<face.vertices.length-1;index++)result.push({type:'triangle',points:[face.vertices[0],face.vertices[index],face.vertices[index+1]].map(point=>point.slice(0,dimension)),fill:accent,stroke:'none',opacity:dimension===2?.24:.30,featureId:feature.id});
    }
  }
  return result;
}

export function meshPrimitives(mesh,{field=false,edges=false}={}) {
  if(!mesh)return [];
  return meshPreviewSurfaces(mesh).surfaces.map(surface=>({type:'triangle',points:surface.nodes.map(node=>mesh.nodes[node].slice(0,mesh.dimension)),...meshSurfaceStyle(surface,{field,edges})}));
}

export function buildApplicationScene({model,selectedPath,selectedValue,mesh,fieldState,expressionPolicy,visibility={geometry:true,mesh:false,edges:true},projector=projectGeometry}={}) {
  if(!model)return {dimension:2,primitives:[],bounds:[],selectionTargets:{},diagnostics:[]};
  const source=projector(model,{expressionPolicy}),projection={...source,bounds:source.bounds.map(range=>({...range}))},primitives=[],diagnostics=[...source.diagnostics];
  if(visibility.mesh&&mesh)primitives.push(...meshPrimitives(mesh,{field:Boolean(fieldState?.isCurrent),edges:visibility.edges}));
  if(visibility.geometry)primitives.push(...geometryPrimitives(projection));
  const target=selectedPath?.[0]==='model'&&selectedPath?.[1]==='geometry'?projection.byId.get(selectedValue?.id):null;
  if(target)primitives.push(...geometryPrimitives(projection,[target],{highlight:true}));
  let selectionPaint;
  if(selectedValue&&!target){
    try{
      if(selectionPaintCache.get(source)?.mesh!==mesh||!selectionPaintCache.has(source))selectionPaintCache.set(source,{mesh,values:new Map()});
      const cache=selectionPaintCache.get(source).values,key=JSON.stringify([selectedValue,model.model.selections,model.model.mesh?.periodicPairs]);
      selectionPaint=cache.get(key);
      if(!selectionPaint){selectionPaint=namedSelectionPrimitives({model,projection,selected:selectedValue,mesh});cache.set(key,selectionPaint);if(cache.size>5)cache.delete(cache.keys().next().value);}
      primitives.push(...selectionPaint.primitives);diagnostics.push(...selectionPaint.diagnostics);
    }catch(error){diagnostics.push({message:error.message});}
  }
  return {...projection,primitives,selectionPaint,selectedGeometryId:target?.id,selectionTargets:Object.fromEntries([...projection.byId].map(([id])=>[id,{id}])),diagnostics};
}
