import React from '../../../platform/ui/widgets/shared/react/react-runtime.js';
import {mountReactComponent} from '../../../platform/ui/widgets/shared/react/mount.js';

const summaries=new WeakMap();
const histories=new WeakMap();
function StudyHistory({view}){
  if(view.loading)return <p role="status">Reading the selected Study's saved run…</p>;
  if(view.loadError)return <p role="alert">{view.loadError}</p>;
  return <>{view.active?<p>Observed run {view.active.runId} · worker {view.active.liveness}</p>:null}{view.empty?<p>{view.message}</p>:<dl>
    <dt>Run</dt><dd>{view.runId}</dd><dt>State</dt><dd>{view.state}</dd><dt>Started</dt><dd>{view.startedAt}</dd><dt>Finished</dt><dd>{view.completedAt}</dd><dt>{view.durationLabel}</dt><dd>{view.durationSeconds===null?'Unavailable':`${view.durationSeconds} s`}</dd>{view.error?<><dt>Diagnostic</dt><dd>{view.error}</dd></>:null}
  </dl>}</>;
}
export function renderStudyHistory(host,view){
  if(!host)return;const mount=histories.get(host);if(mount?.inspect().mounted)mount.update({view});else histories.set(host,mountReactComponent(host,StudyHistory,{view}));
}
function FieldStatus({context}){
  const state=context.fieldPlotState,selected=context.selectedPath&&context.valueAt(context.selectedPath),ready=state?.actionId===selected?.id&&state.isCurrent;
  return <section aria-label="Field source"><p role="status">{ready?`${state.resolved.studyName||state.data.studyId} · run ${state.data.runId} · ${state.data.stationary?'Stationary':context.fieldPointLabel(state)} · ${state.data.nodes.length} nodes · ${state.data.cells.length} elements`:context.fieldPlotLoadStatus?.message||'Select a saved field result to inspect its source.'}</p></section>;
}

export function renderResultSettings(context){
  const host=context.$('[data-model-editor-extension="resultSettings"]');if(!host)return;
  const action=Array.isArray(context.selectedPath)&&context.isResultPath(context.selectedPath)?context.valueAt(context.selectedPath):null;
  host.hidden=!context.isFieldResultAction(action);
  if(host.hidden)return;
  const existing=summaries.get(host);
  if(existing?.inspect().mounted)existing.update({context});else summaries.set(host,mountReactComponent(host,FieldStatus,{context}));
}

function ClearResultsDialog(){return <dialog data-model-editor-extension="clearResults" aria-labelledby="clear-results-title">
  <h2 id="clear-results-title">Clear results</h2><p id="clear-results-description"/><p id="clear-results-path"/>
  <button type="button" id="cancel-clear-results" onClick={event=>event.currentTarget.closest('dialog').close('cancel')}>Cancel</button>
  <button type="button" id="confirm-clear-results" onClick={event=>event.currentTarget.closest('dialog').close('clear')}>Clear results</button>
</dialog>;}

export function mountEditorPanels({settingsView}){
  const host=settingsView.ownerDocument.createElement('div');host.dataset.applicationPanel='fem';settingsView.append(host);
  const dialogHost=host.ownerDocument.createElement('div'),fieldHost=host.ownerDocument.createElement('div');fieldHost.dataset.modelEditorExtension='resultSettings';fieldHost.hidden=true;host.append(dialogHost,fieldHost);
  const history=host.ownerDocument.createElement('section');history.id='fem-study-history';history.dataset.modelEditorPanel='studyHistory';history.hidden=true;
  const heading=host.ownerDocument.createElement('h3');heading.dataset.modelEditorPanelHeading='';heading.textContent='Last finished run';history.append(heading);
  const historyBody=host.ownerDocument.createElement('div');historyBody.dataset.femStudyHistory='';history.append(historyBody);host.append(history);
  const mount=mountReactComponent(dialogHost,ClearResultsDialog,{});
  return ()=>{summaries.get(fieldHost)?.dispose();summaries.delete(fieldHost);histories.get(historyBody)?.dispose();histories.delete(historyBody);mount.dispose();host.remove();};
}
