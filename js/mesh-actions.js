/** FEM mesh orchestration through the editor's fixture-aware checked services. */
import {attachMeshMap} from './mesh-map.js';
const canonical=value=>typeof value==='string'&&value.length>0&&!/[\\:\0?#]/.test(value)&&value.split('/').every(part=>part&&part!=='.'&&part!=='..');
const at=(value,path)=>path.reduce((node,key)=>node?.[key],value);

export async function runMeshBuild(services,readInputs,execution){
  const {document,draft,operation,network,validation,preview,ui}=services;
  if(!network.compatible())throw new Error('Confirm server compatibility before building a mesh.');
  if(operation.busyWithDocument())throw new Error('Finish the active document operation first.');
  draft.commit();
  if(draft.settingsDirty()||draft.jsonDirty())throw new Error('Apply or repair the current draft before building a mesh.');
  const initial=readInputs(),identity=JSON.stringify([initial.folder,initial.file]),snapshot=JSON.stringify(initial.model);
  if(!canonical(initial.folder)||!canonical(initial.file)||!canonical(initial.runs))throw new Error('Open a saved model with canonical run storage.');
  const errors=validation.model(initial.model);validation.record(errors);if(errors.length)throw new Error(errors[0]);
  if(document.hasUnsavedChanges())await document.save();
  const current=readInputs();
  if(identity!==JSON.stringify([current.folder,current.file])||snapshot!==JSON.stringify(current.model)||document.hasUnsavedChanges())throw new Error('The document changed while saving. Save the latest inputs and build again.');
  const id=operation.createId(),runId=`mesh-${id}`,cwd=`${initial.folder}/${initial.runs}/${runId}`,kind=execution.workflow.operationKind;
  const values={...initial,runId,operationId:id,modelFile:`${initial.folder}/${initial.file}`};
  const args=execution.command.args.map(arg=>Object.hasOwn(arg,'value')?arg.value:values[arg.input]);
  if(args.some(arg=>typeof arg!=='string'))throw new Error('A declared mesh argument could not be resolved.');
  operation.start(kind,id);operation.beginProgress({id,kind,label:'Build mesh',message:'Building geometry and mesh…',cancellable:true});
  preview.updateStatus('running','Building geometry and mesh…',cwd);ui.setStatus('Building geometry and mesh…');
  let terminal;
  try{
    const result=await network.requestJson(network.endpoint(execution.workflow.api.command),{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({registryId:execution.command.registryId,entry:execution.command.entry,args,cwd,createCwd:true,stdin:snapshot,expectedOutputs:execution.command.expectedOutputs,operationId:id})});
    if(operation.stopRequested(kind))throw new Error('Mesh build stopped.');
    if(!result.success)throw new Error(result.error||result.stderr?.trim()||'The mesh command failed.');
    const metadata=JSON.parse(result.stdout),artifact=execution.output.previewArtifact;
    if(metadata.runId!==runId||!metadata.geometry?.geometryHash||!metadata.mesh||!(metadata.mesh.nodes>0))throw new Error('The mesher returned incomplete run metadata.');
    for(const expected of execution.command.expectedOutputs){
      if(!result.outputs?.some(output=>output.path===expected.path))throw new Error(`The Kernel did not verify ${expected.path}.`);
    }
    const url=new URL(network.endpoint(execution.workflow.api.artifact),'http://fullmeta.local');url.searchParams.set('path',`${cwd}/${artifact}`);
    const text=await network.requestJson(url.pathname+url.search,{cache:'no-store',responseType:'text'});
    if(operation.stopRequested(kind))throw new Error('Mesh build stopped.');
    let mesh=await preview.parse(text);
    if(mesh.nodes.length!==metadata.mesh.nodes)throw new Error('The checked mesh and report disagree on node count.');
    const mapName=metadata.geometry.mapName;
    if(typeof mapName!=='string'||!result.outputs.some(output=>output.path===mapName))throw new Error('The Kernel did not verify the mesh selection map.');
    url.searchParams.set('path',`${cwd}/${mapName}`);
    const map=await network.requestJson(url.pathname+url.search,{cache:'no-store'});
    mesh=attachMeshMap(mesh,map,{model:initial.model,geometryHash:metadata.geometry.geometryHash});
    if(operation.stopRequested(kind))throw new Error('Mesh build stopped.');
    preview.setGeometry(at(metadata,execution.workflow.response.geometryName));
    preview.show(mesh,initial.previewTarget,initial.model);
    const detail=`${mesh.nodes.length} nodes · ${mesh.cells?.length||mesh.triangles?.length||mesh.tetrahedra?.length||0} elements`;
    terminal={state:'success',message:'Mesh built',detail,progress:100};
    preview.updateStatus('success','Mesh built',detail);ui.setStatus(`Mesh built · ${detail} · ${runId}`,'good');return {runId,cwd,metadata};
  }catch(error){terminal={state:operation.stopRequested(kind)?'cancelled':'error',message:error.message};preview.updateStatus(terminal.state,'Mesh build failed',error.message);throw error;}
  finally{operation.finish(kind,id);operation.finishProgress(id,terminal);await operation.finishCancelable(id);ui.updateWorkflowPanels();}
}
