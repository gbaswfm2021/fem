import {parseMeshArtifact} from './mesh-artifact.js';
import {checkedFieldArtifactUrl} from './result-adapters.js';
import {attachMeshMap} from './mesh-map.js';

/** Read an integrity-checked native cache without starting a mesher or changing the model. */
export async function restorePreview(context){
  await context.compatibilityReady;
  if(!context.model?.model?.geometry?.features?.length)return;
  if(!context.isCurrent()||!context.serverCompatible()||context.operationBusy()||!context.hasEndpoint('meshPreview'))return;
  const contract=context.apiContract('meshPreview'),program=contract?.externalProgram;
  if(!program?.registryId||program.entry!=='preview')throw new Error('The FEM schema must declare the mesh preview entry.');
  const response=await context.requestJson(context.endpoint('meshPreview'),{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({registryId:program.registryId,entry:program.entry,cwd:context.folder,args:['-','--component',context.componentId||'model','--runs-directory',context.runsDirectory],stdin:JSON.stringify(context.model),expectedOutputs:[]})});
  if(!context.isCurrent())return;
  if(!response.success)throw new Error(response.error||response.stderr?.trim()||'The native mesh cache could not be read.');
  const result=JSON.parse(response.stdout);if(result.available===false)return;
  if(result.available!==true||typeof result.meshFile!=='string'||!result.meshFile.startsWith(`${context.folder}/${context.runsDirectory}/`)||typeof result.geometryFile!=='string')throw new Error('The native mesh cache returned an invalid artifact identity.');
  const url=new URL('/api/kernel/artifact','http://fullmeta.local');url.searchParams.set('path',result.meshFile);
  const text=await context.requestJson(checkedFieldArtifactUrl(url.pathname+url.search),{responseType:'text',cache:'no-store'});
  if(!context.isCurrentComponent())return;
  let mesh=parseMeshArtifact(text,result.meshFile);
  if(mesh.nodes.length!==result.report?.nodes)throw new Error('The cached mesh and native report disagree on node count.');
  if(typeof result.modelMapFile!=='string'||!result.modelMapFile.startsWith(`${context.folder}/${context.runsDirectory}/`))throw new Error('The cached selection map is outside the selected run storage.');
  url.searchParams.set('path',result.modelMapFile);
  const map=await context.requestJson(checkedFieldArtifactUrl(url.pathname+url.search),{cache:'no-store'});
  if(!context.isCurrentComponent())return;
  mesh=attachMeshMap(mesh,map,{model:context.model});
  context.showMesh(mesh,context.componentId,context.model,result.geometryFile);
  context.updateRunbar('success','Restored checked mesh',`${mesh.nodes.length} nodes · ${mesh.cells.length} elements`);
}

/** Refresh the in-memory preview after a Study creates or reuses the shared mesh. */
export async function restorePreviewAfterStudy(services, schema){
  const inputs=services.resolveInputs({
    model:'document.model',folder:'document.folder',file:'document.file',
    componentId:'graphics.componentId',runsDirectory:'application.runsDirectory',
  });
  if(!inputs.model||typeof inputs.folder!=='string'||typeof inputs.file!=='string'
    ||typeof inputs.runsDirectory!=='string')return;
  const snapshot=JSON.stringify(inputs.model),identity=[inputs.folder,inputs.file,inputs.runsDirectory];
  const current=()=>{
    const latest=services.resolveInputs({
      model:'document.model',folder:'document.folder',file:'document.file',
      componentId:'graphics.componentId',runsDirectory:'application.runsDirectory',
    });
    return latest.model===inputs.model&&JSON.stringify(latest.model)===snapshot
      &&latest.folder===identity[0]&&latest.file===identity[1]&&latest.runsDirectory===identity[2];
  };
  const editorApi=schema?.['x-fullmeta-runtime']?.editorApi||{};
  await restorePreview({
    model:inputs.model,componentId:inputs.componentId,folder:inputs.folder,file:inputs.file,
    runsDirectory:inputs.runsDirectory,compatibilityReady:Promise.resolve(),isCurrent:current,
    isCurrentComponent:()=>current()&&services.resolveInputs({componentId:'graphics.componentId'}).componentId===inputs.componentId,
    serverCompatible:services.network.compatible,operationBusy:services.operation.busy,
    hasEndpoint:name=>typeof editorApi[name]?.route==='string',apiContract:name=>editorApi[name]||null,
    endpoint:services.network.endpoint,requestJson:services.network.requestJson,
    showMesh:(mesh,componentId,model)=>services.preview.show(mesh,componentId,model),
    updateRunbar:services.preview.updateStatus,
  });
}
