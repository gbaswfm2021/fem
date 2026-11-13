const nodeCounts={F:3,I:3,T:4,L:2,P:1,E:2,C:2,J:2,Q:3};
const integer=(value,path)=>{if(!/^\d+$/.test(value)||!Number.isSafeInteger(Number(value)))throw new Error(`${path}: expected a nonnegative integer.`);return Number(value);};
const tag=(value,path)=>{if(!/^-?\d+$/.test(value)||!Number.isSafeInteger(Number(value)))throw new Error(`${path}: expected an integer physical tag.`);return Number(value);};
const key=nodes=>[...nodes].sort((a,b)=>a-b).join(',');

/** Read the public native mesh handoff; preserve source order and full topology. */
export function parseMeshArtifact(text,path='mesh artifact') {
  if(typeof text!=='string')throw new Error(`${path}: expected mesh text.`);
  const lines=text.replace(/^\uFEFF/,'').trim().split(/\r?\n/).map(line=>line.trim()).filter(Boolean),header=lines.shift();
  const dimension=header==='FULLMETA_MESH/1'?3:header==='FULLMETA_MESH/2'?2:null;
  if(!dimension||lines.pop()!=='FULLMETA_MESH_DATA_END')throw new Error(`${path}: invalid mesh header or end marker.`);
  const nodes=[],records=[],ids=new Map(),allowed=dimension===3?new Set(['F','I','T','L','P']):new Set(['E','C','J','Q','P']);
  for(const [lineIndex,line] of lines.entries()){
    const [type,...cells]=line.split(',').map(value=>value.trim()),at=`${path}:${lineIndex+2}`;
    if(type==='V'){
      if(cells.length!==4)throw new Error(`${at}: invalid node record.`);
      const id=integer(cells[0],at),position=cells.slice(1).map(value=>value===''?NaN:Number(value));
      if(nodes[id]||!position.every(Number.isFinite)||dimension===2&&position[2]!==0)throw new Error(`${at}: duplicate or invalid node coordinates.`);
      nodes[id]=position;continue;
    }
    if(!allowed.has(type))throw new Error(`${at}: unsupported ${type} record.`);
    const count=nodeCounts[type],tagCount=['I','J'].includes(type)?2:1;
    if(cells.length!==1+count+tagCount)throw new Error(`${at}: invalid ${type} record width.`);
    const values=cells.map((value,index)=>(index>count?tag:integer)(value,at)),id=values.shift(),recordNodes=values.splice(0,count),identity=`${type}:${id}`;
    if(ids.has(identity)||new Set(recordNodes).size!==count)throw new Error(`${at}: duplicate record ID or repeated node reference.`);
    const record={type,id,nodes:recordNodes,tags:values,line:lineIndex+2};ids.set(identity,true);records.push(record);
  }
  if(nodes.length<dimension+1||Object.keys(nodes).length!==nodes.length)throw new Error(`${path}: node IDs must be compact and zero based.`);
  for(const record of records)if(record.nodes.some(id=>!nodes[id]))throw new Error(`${path}:${record.line}: node reference does not exist.`);
  const cells=records.filter(record=>record.type===(dimension===2?'Q':'T'));
  if(!cells.length)throw new Error(`${path}: the mesh has no cells.`);
  const surfaces=dimension===2?cells.map(record=>({...record,kind:'cell',tag:record.tags[0]})):
    records.filter(record=>record.type==='F'||record.type==='I').map(record=>({...record,kind:record.type==='I'?'interface':'boundary',tag:record.tags[0]}));
  if(dimension===3){
    const explicit=new Set(surfaces.filter(surface=>surface.kind==='boundary').map(surface=>key(surface.nodes))),faces=new Map();
    for(const cell of cells){const [a,b,c,d]=cell.nodes;for(const face of [[a,b,c],[a,b,d],[a,c,d],[b,c,d]]){
      const identity=key(face),entry=faces.get(identity);
      if(entry)entry.count++;else faces.set(identity,{nodes:face,count:1,kind:'exterior',tag:cell.tags[0],cellId:cell.id});
    }}
    for(const [identity,face] of faces)if(face.count===1&&!explicit.has(identity))surfaces.push(face);
  }
  return {dimension,nodes,cells,records,surfaces,path};
}

export function meshPreviewSurfaces(mesh) {
  const stride=Math.max(1,Math.ceil(mesh.surfaces.length/18000));
  return {surfaces:mesh.surfaces.filter((_,index)=>index%stride===0),stride,total:mesh.surfaces.length};
}

const fieldMeshes=new WeakMap();
/** The field's own checked topology supplies its overlay in the same saved frame. */
export function meshForField(data){
  if(fieldMeshes.has(data))return fieldMeshes.get(data);
  let surfaces;
  if(data.dimension===2)surfaces=data.cells.map((nodes,index)=>({nodes,kind:'cell',tag:data.cellMaterialTags?.[index]??0}));
  else{
    const faces=new Map();
    for(const [index,cell]of data.cells.entries()){
      const [a,b,c,d]=cell,tag=data.cellMaterialTags?.[index]??null;
      for(const nodes of [[a,b,c],[a,b,d],[a,c,d],[b,c,d]]){const identity=key(nodes),entry=faces.get(identity);if(entry){entry.count++;entry.tags.push(tag);}else faces.set(identity,{nodes,count:1,tags:[tag]});}
    }
    surfaces=[...faces.values()].filter(face=>face.count===1||face.tags.every(tag=>tag!==null)&&new Set(face.tags).size>1).map(face=>({...face,kind:face.count===1?'exterior':'interface',tag:face.tags[0]??0}));
  }
  const mesh={dimension:data.dimension,nodes:data.nodes,cells:data.cells,surfaces};fieldMeshes.set(data,mesh);return mesh;
}

export function meshSurfaceStyle(surface,{field=false,edges=false}={}) {
  if(field)return {fill:'rgba(129,153,175,0.12)',stroke:edges?'rgba(163,185,204,0.36)':'none',lineWidth:.7};
  if(surface.kind==='interface')return {fill:'rgba(72,215,178,0.12)',stroke:edges?'rgba(72,215,178,0.72)':'none',lineWidth:.7};
  const hue=(Math.abs(surface.tag)*47)%360;
  return {fill:`hsla(${hue},70%,55%,0.10)`,stroke:edges?`hsla(${hue},75%,65%,0.55)`:'none',lineWidth:.7};
}
