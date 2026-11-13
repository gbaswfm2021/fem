import { evaluateQuantity, getCoreUnit, resolveParameterTable } from '../../../platform/core-rust/js/api.js';

const axes=['x','y','z'];
const fail=message=>{throw new Error(message);};
const scalar=(real,imag,part)=>part==='magnitude'?Math.hypot(real,imag):part==='imaginary'?imag:real;

export function fieldQuantityLabel(quantity={}) {
  const component=quantity.component;
  if(quantity.part==='magnitude')return `|${component}|`;
  return `${quantity.part==='imaginary'?'Im':'Re'}(${component})`;
}

/** The resolver's saved SI samples override the current parameter defaults. */
export function fieldParameterSymbols(model,resolved={}, {evaluationPolicy,parameterPolicy}={}) {
  const saved=resolved.fieldEvaluation?.parameters;
  const selected={...(resolved.selectedFilters||{}),...(saved||{})};
  const parameters=(model?.model?.parameters||[]).map(parameter=>{
    if(!Object.hasOwn(selected,parameter.symbol))return parameter;
    const sample=selected[parameter.symbol];
    if(typeof sample!=='number'||!Number.isFinite(sample))fail(`Saved parameter ${parameter.symbol} is not finite.`);
    return {...parameter,expression:String(sample/getCoreUnit(parameter.unit).scale)};
  });
  return resolveParameterTable(parameters,parameterPolicy?.reservedSymbols||[],parameterPolicy?.allowedFunctions||[],parameterPolicy?.path||'parameters',evaluationPolicy);
}

export function fieldBounds(data) {
  return Array.from({length:data.dimension},(_,axis)=>data.nodes.reduce((range,point)=>({minimum:Math.min(range.minimum,point[axis]),maximum:Math.max(range.maximum,point[axis])}),{minimum:Infinity,maximum:-Infinity}));
}

export function resolveCutPlanes(data,positions={},symbols={},evaluationPolicy) {
  if(data.dimension!==3)return {};
  const bounds=fieldBounds(data);
  return Object.fromEntries(axes.map((axis,index)=>{
    const {minimum,maximum}=bounds[index],declaration=positions[axis];
    let coordinate=(minimum+maximum)/2;
    if(declaration?.mode==='explicitValues'){
      const unit=getCoreUnit(declaration.coordinate?.unit);
      if(unit.dimension.some((value,index)=>value!==(index===0?1:0)))fail(`${axis.toUpperCase()} cut plane: coordinate must have length units.`);
      coordinate=evaluateQuantity(declaration.coordinate,symbols,`${axis.toUpperCase()} cut plane coordinate`,evaluationPolicy?.allowedFunctions||[],evaluationPolicy);
    }else if(declaration!==undefined&&declaration.mode!=='midPlane')fail(`${axis.toUpperCase()} cut plane: unknown position mode.`);
    const tolerance=Math.max(1e-10,(maximum-minimum)*1e-9);
    if(!Number.isFinite(coordinate)||coordinate<minimum-tolerance||coordinate>maximum+tolerance)fail(`${axis.toUpperCase()} cut plane: coordinate ${coordinate} m is outside [${minimum}, ${maximum}] m.`);
    return [axis,{axis,index,coordinate:Math.min(maximum,Math.max(minimum,coordinate)),minimum,maximum,mode:declaration?.mode||'midPlane'}];
  }));
}

/** Percentiles count rendered triangle corners, including repeated vertices. */
export function fieldColorRange(surfaces,part) {
  const values=surfaces.flatMap(surface=>surface.faces.flatMap(face=>face.map(index=>surface.vertices[index].value))).filter(Number.isFinite).sort((a,b)=>a-b);
  if(!values.length)fail('The field has no finite drawable values.');
  const percentile=p=>values[Math.floor(p*(values.length-1))];
  let minimum=part==='magnitude'?0:percentile(.02),maximum=percentile(part==='magnitude'?.985:.98);
  if(!Number.isFinite(minimum)||!Number.isFinite(maximum)||maximum<minimum){minimum=values[0];maximum=values.at(-1);}
  if(maximum===minimum)maximum=minimum+1;
  return [minimum,maximum];
}

function cutCell(points,values,plane) {
  const span=Math.max(...axes.map((_,axis)=>Math.max(...points.map(p=>p[axis]))-Math.min(...points.map(p=>p[axis]))));
  const epsilon=Math.max(span*1e-12,Number.EPSILON*Math.max(...points.flat().map(Math.abs))*16,Number.MIN_VALUE);
  const vertices=[];
  const add=(position,value)=>{if(!vertices.some(vertex=>Math.hypot(...position.map((v,i)=>v-vertex.position[i]))<=epsilon))vertices.push({position,value});};
  const distances=points.map(point=>point[plane.index]-plane.coordinate);
  for(let a=0;a<4;a++){
    if(Math.abs(distances[a])<=epsilon)add([...points[a]],values[a]);
    for(let b=a+1;b<4;b++)if(distances[a]<-epsilon&&distances[b]>epsilon||distances[a]>epsilon&&distances[b]<-epsilon){
      const fraction=distances[a]/(distances[a]-distances[b]);
      add(points[a].map((value,index)=>value+(points[b][index]-value)*fraction),values[a]+(values[b]-values[a])*fraction);
    }
  }
  if(vertices.length<3)return null;
  const remaining=[0,1,2].filter(index=>index!==plane.index),center=remaining.map(index=>vertices.reduce((sum,v)=>sum+v.position[index],0)/vertices.length);
  vertices.sort((a,b)=>Math.atan2(a.position[remaining[1]]-center[1],a.position[remaining[0]]-center[0])-Math.atan2(b.position[remaining[1]]-center[1],b.position[remaining[0]]-center[0]));
  const faces=[];
  for(let index=1;index<vertices.length-1;index++){
    const a=vertices[0].position,b=vertices[index].position,c=vertices[index+1].position,u=remaining[0],v=remaining[1];
    if(Math.abs((b[u]-a[u])*(c[v]-a[v])-(b[v]-a[v])*(c[u]-a[u]))>epsilon*epsilon)faces.push([0,index,index+1]);
  }
  return faces.length?{vertices,faces}:null;
}

/** Convert checked nodal or cell-local samples to the shared scalar-surface ABI. */
export function prepareFieldSurfaces(data,action,{symbols={},evaluationPolicy,cutPlanes=resolveCutPlanes(data,action.cutPlanePositions,symbols,evaluationPolicy)}={}) {
  const component=action?.quantity?.component,part=action?.quantity?.part;
  if(!['real','imaginary','magnitude'].includes(part))fail('Unsupported field part.');
  const field=data.cellFields[component]||data.fields[component],local=Boolean(data.cellFields[component]);
  if(!field)fail(`Field component ${component} has no checked samples.`);
  const unit=data.quantities.find(value=>value.symbol===component)?.unit||field.unit;
  if(!unit)fail(`Field component ${component} is unavailable.`);
  const surfaces=[],byAxis={x:[],y:[],z:[]};
  data.cells.forEach((cell,cellIndex)=>{
    const points=cell.map(index=>data.nodes[index]);
    const values=cell.map((node,index)=>scalar(local?field.real[cellIndex][index]:field.real[node],local?field.imag[cellIndex][index]:field.imag[node],part));
    if(values.some(value=>!Number.isFinite(value)))fail(`Field cell ${cellIndex} has nonfinite samples.`);
    if(data.dimension===2){
      const [a,b,c]=points;
      if((b[0]-a[0])*(c[1]-a[1])!==(b[1]-a[1])*(c[0]-a[0]))surfaces.push({vertices:points.map((position,index)=>({position,value:values[index]})),faces:[[0,1,2]],cellIndex});
    }
    else for(const axis of axes){const surface=cutCell(points,values,cutPlanes[axis]);if(surface){const value={...surface,cellIndex,axis};surfaces.push(value);byAxis[axis].push(value);}}
  });
  return {surfaces,byAxis,range:fieldColorRange(surfaces,part),unit,quantity:structuredClone(action.quantity),cutPlanes,bounds:fieldBounds(data)};
}
