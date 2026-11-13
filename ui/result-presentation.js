import { evaluateQuantity, getCoreUnit, resolveParameterTable } from '../../../platform/core-rust/js/api.js';
import { bindRuntimePolicy } from '../../../platform/model-rust/js/api.js';
import { drawPlot1d } from '../../../platform/ui/widgets/plot-1d/widget.js';
import { plot1dColor } from '../../../platform/ui/widgets/graphics-console/widget.js';
import { createTableData } from '../../../platform/ui/widgets/shared/tables/data.js';
import { checkedFieldArtifactUrl, normalizeFieldArtifact } from '../js/result-adapters.js';
import { prepareResultDataset } from '../js/result-tables.js';
import { fieldParameterSymbols, prepareFieldSurfaces, resolveCutPlanes } from './field-surfaces.js';

const clone=value=>structuredClone(value);
const fail=message=>{throw new Error(message);};

function checkedSelection(stored={}) {
  const selection={};
  if(stored.caseIndex!==undefined){if(!Number.isInteger(stored.caseIndex)||stored.caseIndex<0)fail('Invalid saved field case index.');selection.caseIndex=stored.caseIndex;}
  if(stored.frequencyGHz!==undefined){if(!Number.isFinite(stored.frequencyGHz)||stored.frequencyGHz<=0)fail('Invalid saved field frequency.');selection.frequencyGHz=stored.frequencyGHz;}
  if(stored.studyId!==undefined)selection.studyId=stored.studyId;
  return selection;
}

function fieldTables(resolved,action) {
  const metadata=resolved.tabularMetadata;
  if(!metadata)return [];
  return [createTableData({id:action.id,name:resolved.studyName||action.name,columns:metadata.columns||[],rows:(metadata.coordinates||[]).map(coordinate=>({...coordinate.values})),defaultXColumn:metadata.defaultXColumn})];
}

function recipeForDataset(action){
  return action.type==='sweep'?{...action,series:[{id:action.id,name:action.name,column:action.quantity}]}:action;
}

/** Host ABI; service functions are injected, never serialized in editor state. */
export function createResultPresentation({modelSchema,readArtifact}={}) {
  const runtime=modelSchema?.['x-fullmeta-expressionRuntime'];
  const evaluationPolicy=runtime?.real?bindRuntimePolicy(null,{},runtime.real,modelSchema):undefined;
  const parameterPolicy=runtime?.parameters?bindRuntimePolicy(null,{realPolicy:evaluationPolicy,reservedSymbols:[...(evaluationPolicy?.builtinSymbols||[]),'t']},runtime.parameters,modelSchema):undefined;
  return {
    selectionForAction({stored}){return checkedSelection(stored);},
    resolveBody({folder,file,model,action,selection}){return {folder,file,model,actionId:action.id,...checkedSelection(selection)};},
    fieldResolution({action,resolved,missingMessage}){
      const tables=fieldTables(resolved,action),selectionError=resolved.selectionError||null;
      if(selectionError)return {tables,selectionError,error:null,artifactUrl:null};
      if(resolved.action?.id!==undefined&&resolved.action.id!==action.id)fail('The result response belongs to another recipe.');
      if(resolved.modelFreshness!==undefined&&resolved.modelFreshness!=='current')fail('The result response is stale or unverified.');
      if(resolved.error||!resolved.url)return {tables,selectionError:null,error:resolved.error||resolved.message||missingMessage||`No completed run is available for ${resolved.studyName||action.name}.`,artifactUrl:null};
      return {tables,selectionError:null,error:null,artifactUrl:checkedFieldArtifactUrl(resolved.url)};
    },
    prepareField({raw,resolved,action,resolvedAction=action,model,dimension}){
      const data=normalizeFieldArtifact(raw,{model,modelSchema,action:resolvedAction,resolved,artifactPath:resolved.path||resolved.url});
      if(dimension!==undefined&&dimension!==data.dimension)fail('The field dimension differs from the active viewport.');
      const symbols=fieldParameterSymbols(model,resolved,{evaluationPolicy,parameterPolicy}),slices=prepareFieldSurfaces(data,resolvedAction,{symbols,evaluationPolicy});
      return {data,action:clone(resolvedAction),actionId:action.id,isCurrent:true,resolved:clone(resolved),symbols,stationary:data.stationary,frequencyGHz:data.frequencyGHz,...slices};
    },
    selectionAfterLoad({stored={},resolved}){return {...clone(stored),...checkedSelection(resolved),filters:clone(resolved.selectedFilters||{})};},
    selectionAfterFilterChange(stored={}){const result=clone(stored);delete result.caseIndex;delete result.frequencyGHz;return result;},
    async prepareDataset(dataset,action,model){
      if(dataset.modelFreshness!==undefined&&dataset.modelFreshness!=='current')fail(`${dataset.path||dataset.id}: result is stale or unverified.`);
      if(typeof readArtifact!=='function')fail('Checked CSV artifact access is unavailable.');
      const text=await readArtifact(checkedFieldArtifactUrl(dataset.url),{responseType:'text'});
      return prepareResultDataset(text,dataset,recipeForDataset(action),model);
    },
    selectEntries(action,entries){const ids=new Set((action.datasets||[]).map(dataset=>dataset.id));return action.type==='sweep'?entries:entries.filter(entry=>ids.has(entry.dataset?.id));},
    tableColumns(action,entry){
      const columns=entry.view.columns;
      const keys=[entry.view.xColumn,...Object.keys(entry.view.filters||{}),...(action.tableColumns||entry.series.map(series=>series.column))];
      return [...new Set(keys)].map(key=>columns.find(column=>column.key===key)||fail(`Declared table column ${key} is unavailable.`));
    },
    tableSectionTitle(entry){return `${entry.name} · ${entry.view.rows.length} of ${entry.table.rows.length} samples`;},
    renderPlot({canvas,ready,action,parameters=[],renderer}){
      if(!ready.length)fail('No checked result samples are available.');
      const first=ready[0].view.columns.find(column=>column.key===ready[0].view.xColumn),xUnit=getCoreUnit(first.unit);
      const entries=ready.map(entry=>{
        const column=entry.view.columns.find(column=>column.key===entry.view.xColumn),other=getCoreUnit(column.unit);
        if(other.dimension.some((value,index)=>value!==xUnit.dimension[index]))fail(`${entry.name}: X units are incompatible across datasets.`);
        return {...entry,xDisplayScale:first.displayScale};
      });
      const axes={...action.axes},symbols=resolveParameterTable(parameters,parameterPolicy?.reservedSymbols||[],parameterPolicy?.allowedFunctions||[],parameterPolicy?.path||'parameters',evaluationPolicy);
      for(const key of ['xMin','xMax'])if(axes[key]!==undefined){const declared=getCoreUnit(axes[key].unit);if(declared.dimension.some((value,index)=>value!==xUnit.dimension[index]))fail(`Plot ${key} unit differs from the X axis.`);axes[key]=evaluateQuantity(axes[key],symbols,`Plot ${key}`,evaluationPolicy?.allowedFunctions||[],evaluationPolicy)/first.displayScale;}
      const series=entries.flatMap(entry=>entry.series.map((item,index)=>{
        const column=entry.view.columns.find(column=>column.key===item.column),unit=item.unit||column.unit||'1',scale=getCoreUnit(unit).scale;
        return {label:`${entry.name} · ${item.label}${unit==='1'?'':` (${unit})`}`,axis:item.axis==='right'?'right':'left',color:plot1dColor(index),style:entry.style,
          points:entry.view.rows.map(row=>[row[entry.view.xColumn]/first.displayScale,row[item.column]/scale])};
      }));
      const data={series,xAxis:{label:`${first.label||first.key}${first.unit==='1'?'':` (${first.unit})`}`,minimum:axes.xMin,maximum:axes.xMax},
        yAxes:{left:{label:'',minimum:axes.yLeftMin,maximum:axes.yLeftMax},right:{label:'',minimum:axes.yRightMin,maximum:axes.yRightMax}}};
      return (renderer||drawPlot1d)({canvas,data});
    },
    cutPlaneChange({action,state,axis,position}){
      if(!['x','y','z'].includes(axis)||state.data.dimension!==3)fail('The selected cut plane is unavailable.');
      const cutPlanePositions={...clone(action.cutPlanePositions||{}),[axis]:clone(position)};
      const cutPlanes=resolveCutPlanes(state.data,cutPlanePositions,state.symbols,evaluationPolicy);
      return {cutPlanePositions,cutPlanes,slices:prepareFieldSurfaces(state.data,{...action,cutPlanePositions},{symbols:state.symbols,cutPlanes,evaluationPolicy})};
    },
    fieldLoadingMessage(stage){return ({fetch:'Fetching checked field data…',artifact:'Loading field artifact…',parse:'Checking field samples…',plot:'Preparing field surfaces…'})[stage]||'Loading field…';},
    fieldPlotSummary({state,action,pointLabel}){return `${action.name} · ${typeof pointLabel==='function'?pointLabel(state):state.data.stationary?'Stationary':`${state.data.frequencyGHz} GHz`}`;},
    sourceSummary(ready){return ready.map(entry=>`${entry.name}: ${entry.view.rows.length}/${entry.table.rows.length} samples${entry.runId?` · run ${entry.runId}`:' · checked file'}`).join('; ');},
    freshnessNotice(ready){return ready.some(entry=>entry.modelFreshness&&entry.modelFreshness!=='current')?'Stale result':'';},
    flowSummary(action){return `${action.name}: Plot and Table use the same selected samples.`;},
    missingSummary(missing){return missing.map(entry=>`${entry.dataset?.name||entry.dataset?.id||'Dataset'}: ${entry.missing||entry.view?.error}`).join('\n');},
    figureSaveInfo({action,entries,ready,isPlot,loading,activeOperation}){
      const supported=action.type==='plot.sweep'&&/\.(svg|png|pdf)$/i.test(action.output?.figureFilename||'');
      const complete=entries.length===(action.datasets||[]).length&&ready.length===entries.length&&ready.every(entry=>entry.series.length&&entry.view.xColumn);
      const enabled=Boolean(supported&&complete&&isPlot&&!loading&&!activeOperation);
      const reason=!supported?'This result does not declare a supported figure output.'
        :!complete?'Figure saving needs ready plot data for every selected dataset.'
          :!isPlot?'Select the Plot tab to save a figure.'
            :loading||activeOperation?'Figure saving is unavailable while a result or operation is active.':'';
      return {enabled,label:'Save figure',reason:enabled?'':reason};
    },
    figureImageDataUrl({action,canvas}){return /\.pdf$/i.test(action.output?.figureFilename||'')?canvas.toDataURL('image/jpeg',.94):canvas.toDataURL('image/png');},
    figureSaveBody({folder,file,model,action,imageDataUrl,canvas}){if(!(canvas.width>0&&canvas.height>0))fail('The figure canvas is empty.');return {folder,file,model,actionId:action.id,imageDataUrl,imageWidth:canvas.width,imageHeight:canvas.height};},
    savedFigureLabel(saved){if(saved?.state!=='completed'||typeof saved.figure!=='string'||!saved.figure||!(saved.bytes>0))fail('The server did not confirm a completed figure write.');return `Saved ${saved.figure} (${saved.bytes} bytes)`;},
  };
}
