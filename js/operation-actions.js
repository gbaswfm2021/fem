import { createActions } from '../../../platform/ui/widgets/model-editor-behavior/widget.js';
import { runMeshBuild } from './mesh-actions.js';

/** Resolve FEM-specific action inputs, then use the normal shared workflows. */
export function createApplicationActions({services,bindings}) {
  const resolveInputs=sources=>{
    const ordinary=Object.fromEntries(Object.entries(sources).filter(([,source])=>!source.startsWith('fem.')));
    const values=services.resolveInputs(ordinary);
    for(const [name,source] of Object.entries(sources)){
      if(source==='fem.documentIdentity'){
        const {folder,file}=services.resolveInputs({folder:'document.folder',file:'document.file'});values[name]=`${folder}/${file}`;
      }else if(source==='fem.selectedStudyId'){
        const selection=services.resolveInputs({path:'selection.path'}).path;
        if(!services.model.selectionHasRole('study',selection)||selection?.length!==3)throw new Error('Select the Study to prepare.');
        const study=services.model.valueAtRole('study')?.[selection[2]];
        if(typeof study?.id!=='string'||!study.id)throw new Error('The selected Study no longer exists.');
        values[name]=study.id;
      }else if(source==='fem.preparationMeshReference'){
        // Preparation validates the Study and returns its solver plan. The
        // solver's preparation entry requires a mesh path, but does not open
        // it; the actual Study run supplies its checked run-local mesh.
        values[name]='mesh/model-mesh.mesh';
      }else if(source==='fem.geometryOutputDirectory'){
        const {folder,runs}=services.resolveInputs({folder:'document.folder',runs:'application.runsDirectory'});
        if(typeof folder!=='string'||!folder||typeof runs!=='string'||!runs)throw new Error('Open a saved FEM document before compiling geometry.');
        const directory=`${folder}/${runs}/geometry-${services.operation.createId()}`;
        if(directory.split('/').some(part=>!part||part==='.'||part==='..')||/[\\:\0]/.test(directory))throw new Error('The geometry output directory is invalid.');
        values[name]=directory;
      }else if(source.startsWith('fem.'))throw new Error(`Unknown FEM action input ${source}.`);
    }
    return values;
  };
  const nativeOperation=async(execution,readInputs,invocation)=>{
    if(execution.operation==='mesh.compile'){
      const values=readInputs(),input=invocation?.input===undefined?values:{...invocation.input,outputDirectory:invocation.input.outputDirectory??values.outputDirectory};
      const {folder,runs}=services.resolveInputs({folder:'document.folder',runs:'application.runsDirectory'}),prefix=`${folder}/${runs}/`;
      if(typeof input.outputDirectory!=='string'||!input.outputDirectory.startsWith(prefix)||input.outputDirectory.split('/').some(part=>!part||part==='.'||part==='..')||/[\\:\0]/.test(input.outputDirectory))throw new Error('Compile geometry must write inside this project’s declared run storage.');
      return services.nativeOperation(execution,()=>values,{...invocation,input});
    }
    if(execution.operation==='fem.prepareStudy'&&invocation?.input){
      const values=readInputs();
      if(invocation.input.studyId!==values.studyId)throw new Error('The preparation input must use the selected Study.');
      return services.nativeOperation(execution,()=>values,{...invocation,input:{...invocation.input,meshFile:invocation.input.meshFile??values.meshFile}});
    }
    return services.nativeOperation(execution,readInputs,invocation);
  };
  const actions=createActions({...services,resolveInputs,nativeOperation},bindings);
  for(const binding of bindings)if(binding.execution.operation==='fem.buildMesh')actions[binding.id]=()=>runMeshBuild(services,()=>resolveInputs(binding.execution.inputs),binding.execution);
  return actions;
}
