const terminal=new Set(['completed','failed','cancelled','interrupted']);
function utcMilliseconds(value){
  if(typeof value!=='string'||!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d{1,3})?Z$/.test(value))return null;
  const milliseconds=Date.parse(value);if(!Number.isFinite(milliseconds))return null;
  const normalized=value.includes('.')?value.replace(/\.(\d+)Z$/,(_,fraction)=>'.'+fraction.padEnd(3,'0')+'Z'):value.replace('Z','.000Z');
  return new Date(milliseconds).toISOString()===normalized?milliseconds:null;
}
export function finishedRunView(record){
  if(record?.available===false)return {empty:true,message:'No finished run for this Study.'};
  if(!record?.available||!terminal.has(record.state)||typeof record.runId!=='string'||!record.runId)throw new Error('The native history response has no valid terminal run identity.');
  const start=utcMilliseconds(record.startedAt),end=utcMilliseconds(record.completedAt),durationSeconds=start===null||end===null||end<start?null:(end-start)/1000;
  return {runId:record.runId,studyId:record.studyId,state:record.state,startedAt:record.startedAt||'Unavailable',completedAt:record.completedAt||'Unavailable',durationSeconds,durationLabel:record.state==='interrupted'?'Time to interruption detection':'Duration',error:typeof record.error==='string'?record.error:record.error?JSON.stringify(record.error):''};
}

/** Observe/reconcile native records using one exact selected document and Study. */
export function createStudyHistory({services,render}){
  let current=null,sequence=0,disposed=false;
  const refresh=async()=>{
    if(!current||disposed)return;
    const selected={...current},version=++sequence,service=services();if(!service)return;
    const same=()=>!disposed&&version===sequence&&current?.key===selected.key&&JSON.stringify(service.resolveInputs({folder:'document.folder',file:'document.file'}))===JSON.stringify({folder:selected.folder,file:selected.file});
    render({loading:true});
    const request=body=>service.network.requestJson(service.network.endpoint('runStatus'),{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({folder:selected.folder,file:selected.file,studyId:selected.studyId,...body})});
    try{
      if(!service.network.compatible())throw new Error('Confirm server compatibility to read Study history.');
      let observed=await request({reconcile:false});if(!same())return;
      if(observed.available&&observed.state==='running'&&observed.liveness==='gone'){
        if(typeof observed.runId!=='string'||typeof observed.operationId!=='string'||observed.studyId!==selected.studyId)throw new Error('The observed worker has incomplete reconciliation identity.');
        observed=await request({runId:observed.runId,operationId:observed.operationId,reconcile:true});if(!same())return;
      }
      const finished=await request({terminalOnly:true,reconcile:false});if(!same())return;
      if(finished.available&&finished.studyId!==selected.studyId)throw new Error('The finished run belongs to another Study.');
      render({...finishedRunView(finished),active:observed.available&&observed.state==='running'?{runId:observed.runId,liveness:observed.liveness}:null});
    }catch(error){if(same())render({loadError:error.message});}
  };
  return {
    select({studyId,folder,file}){const key=JSON.stringify([folder,file,studyId]);if(current?.key===key)return;current={key,studyId,folder,file};void refresh();},
    clear(){current=null;sequence++;},refresh,
    dispose(){disposed=true;current=null;sequence++;},
  };
}
