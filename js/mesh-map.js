/** Attach checked native selection tags to transient mesh presentation data. */
const canonical=value=>Array.isArray(value)?value.map(canonical):value!==null&&typeof value==='object'
  ?Object.fromEntries(Object.keys(value).sort().map(key=>[key,canonical(value[key])])):value;
export function selectionGeometrySignature(selection){
  return JSON.stringify(canonical({id:selection.id,entity:selection.entity,addresses:selection.addresses,
    coordinateExpression:selection.coordinateExpression??'0'}));
}

export function attachMeshMap(mesh,map,{model,geometryHash}={}){
  const dimension=map?.dimension??(map?.schema==='fullmeta-mesh-map/5'?3:null);
  if(!map||!['fullmeta-mesh-map/5','fullmeta-mesh-map/6'].includes(map.schema)||map.modelId!==model?.metadata?.id||dimension!==mesh.dimension||dimension!==model.scenario.dimension||typeof map.geometryHash!=='string'||!/^[a-f0-9]{64}$/.test(map.geometryHash)||geometryHash&&map.geometryHash!==geometryHash)throw new Error('The checked mesh map has incompatible model, geometry or dimension identity.');
  const selectionTags={};
  for(const [entity,key]of [['boundary','boundarySelections'],['edge','edgeSelections'],['point','pointSelections']]){
    const rows=map[key]||[];if(!Array.isArray(rows))throw new Error(`The checked mesh map has invalid ${key}.`);
    for(const row of rows){if(typeof row.id!=='string'||!Number.isSafeInteger(row.physicalTag)||row.physicalTag<=0||selectionTags[row.id])throw new Error(`The checked mesh map has invalid ${entity} tag identity.`);selectionTags[row.id]={entity,tag:row.physicalTag};}
  }
  return {...mesh,selectionTags,selectionSignatures:Object.fromEntries((model.model.selections||[]).map(selection=>[selection.id,selectionGeometrySignature(selection)])),geometryHash:map.geometryHash};
}
