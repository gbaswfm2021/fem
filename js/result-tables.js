import { getCoreUnit } from '../../../platform/core-rust/js/api.js';
import { createTableData, resolveTableSelection } from '../../../platform/ui/widgets/shared/tables/data.js';

const fail=(path,message)=>{throw new Error(`${path}: ${message}`);};
const owns=(value,key)=>Object.hasOwn(value||{},key);
const finite=(value,path)=>{if(typeof value!=='number'||!Number.isFinite(value))fail(path,'expected a finite numeric sample.');return value;};

export function parseNumericCsv(text,path='CSV artifact') {
  if(typeof text!=='string')fail(path,'expected CSV text.');
  const records=[];let row=[],cell='',quoted=false,closed=false;
  const finishCell=()=>{row.push(cell.trim());cell='';closed=false;};
  const finishRow=()=>{finishCell();if(row.some(value=>value!==''))records.push(row);row=[];};
  text=text.replace(/^\uFEFF/,'');
  for(let index=0;index<text.length;index++){
    const character=text[index];
    if(quoted){if(character==='"'){if(text[index+1]==='"'){cell+='"';index++;}else{quoted=false;closed=true;}}else cell+=character;continue;}
    if(character==='"'){if(cell.trim()||closed)fail(path,'malformed CSV quoting.');quoted=true;continue;}
    if(character===','){finishCell();continue;}
    if(character==='\n'){finishRow();continue;}
    if(character==='\r')continue;
    if(closed&&character.trim())fail(path,'text follows a quoted cell.');
    cell+=character;
  }
  if(quoted)fail(path,'unterminated CSV quote.');
  if(cell||row.length)finishRow();
  const headers=records.shift();
  if(!headers?.length||headers.some(header=>!header)||new Set(headers).size!==headers.length)fail(path,'CSV headers must be nonempty and distinct.');
  if(!records.length)fail(path,'CSV contains no numeric rows.');
  const rows=records.map((record,index)=>{
    if(record.length!==headers.length)fail(path,`row ${index+2} has the wrong column count.`);
    return Object.fromEntries(headers.map((header,column)=>{
      if(!record[column])fail(path,`row ${index+2}, column ${header} is blank.`);
      return [header,finite(Number(record[column]),`${path}: row ${index+2}, column ${header}`)];
    }));
  });
  return {headers,rows};
}

function unit(value,path){try{return getCoreUnit(value);}catch(error){fail(path,error.message);}}
function compatible(left,right,path){if(left.dimension.some((value,index)=>Math.abs(value-right.dimension[index])>1e-10))fail(path,'source and display units are incompatible.');}

/** Keep rows in SI and attach display scales shared by the plot and table. */
export function prepareResultDataset(text,dataset,action,model) {
  const path=dataset.path||dataset.url||dataset.id||'result dataset';
  const parsed=parseNumericCsv(text,path),rows=parsed.rows.map(row=>({...row}));
  const series=Array.isArray(action.series)?action.series:[];
  const file=dataset.source?.type==='file',sourceUnits=dataset.source?.columnUnits||{};
  for(const key of Object.keys(sourceUnits))if(!parsed.headers.includes(key))fail(path,`source unit column ${key} is not a literal CSV header.`);
  const declaredSeriesUnit=key=>series.find(item=>item.column===key)?.unit;
  const sourceUnit=key=>sourceUnits[key]??(key==='frequency_Hz'?'Hz':key==='frequency_GHz'?'GHz':
    file?(key===dataset.xColumn?dataset.xUnit||declaredSeriesUnit(key)||'1':declaredSeriesUnit(key)||'1'):
    dataset.tabularMetadata?.columns?.find(column=>column.key===key)?.unit??dataset.storedVariableUnits?.[key]??(key.endsWith('_re')||key.endsWith('_im')?dataset.storedVariableUnits?.[key.slice(0,-3)]:undefined)??'1');
  const columns=parsed.headers.map(key=>{
    const actual=sourceUnit(key),actualUnit=unit(actual,`${path}.${key}`);
    const display=key===dataset.xColumn?dataset.xUnit||actual:declaredSeriesUnit(key)||actual;
    const displayUnit=unit(display,`${path}.${key}`);compatible(actualUnit,displayUnit,`${path}.${key}`);
    const inputScale=file||key==='frequency_Hz'||key==='frequency_GHz'?actualUnit.scale:1;
    for(const row of rows)row[key]=finite(row[key]*inputScale,`${path}.${key}`);
    return {key,label:key,unit:display,displayScale:displayUnit.scale,role:key===dataset.xColumn||dataset.parameterColumns?.includes(key)?'parameter':'value'};
  });
  const metadata=dataset.tabularMetadata;
  if(metadata?.coordinates){
    if(metadata.coordinates.length!==rows.length)fail(path,'CSV row count differs from the checked Study case count.');
    rows.forEach((row,index)=>{
      const expected=metadata.coordinates[index].frequencyHz;
      if(expected!==undefined)for(const key of ['frequency_Hz','frequency_GHz'])if(owns(row,key)&&Math.abs(row[key]-finite(expected,path))>Math.max(1,Math.abs(expected))*1e-10)fail(path,`CSV frequency differs from saved case ${index}.`);
    });
    for(const descriptor of metadata.columns||[]){
      const actual=unit(descriptor.unit,`${path}.${descriptor.key}`),display=descriptor.key===dataset.xColumn?dataset.xUnit||descriptor.unit:descriptor.unit;
      const displayUnit=unit(display,`${path}.${descriptor.key}`);compatible(actual,displayUnit,`${path}.${descriptor.key}`);
      rows.forEach((row,index)=>{
        const value=finite(metadata.coordinates[index].values?.[descriptor.key],`${path}: saved case ${index}`);
        if(owns(row,descriptor.key)&&Math.abs(row[descriptor.key]-value)>Math.max(1,Math.abs(value))*1e-10)fail(path,`CSV parameter ${descriptor.key} differs from saved case ${index}.`);
        row[descriptor.key]=value;
      });
      const column={...structuredClone(descriptor),unit:display,displayScale:displayUnit.scale,role:'parameter'};
      const existing=columns.findIndex(value=>value.key===column.key);if(existing<0)columns.push(column);else columns[existing]=column;
    }
  }
  for(const item of series){
    let column=columns.find(column=>column.key===item.column);
    if(!column&&!file&&owns(dataset.storedVariableExpressions,item.column)&&parsed.headers.includes(item.column+'_re')){
      const source=columns.find(column=>column.key===item.column+'_re');
      column={...source,key:item.column,label:item.column,role:'value'};columns.push(column);
      for(const row of rows)row[item.column]=row[item.column+'_re'];
    }
    if(!column)fail(path,`declared series column ${item.column} is missing.`);
    if(item.unit){compatible(unit(column.unit,path),unit(item.unit,path),`${path}.${item.column}`);if(column.key!==dataset.xColumn){column.unit=item.unit;column.displayScale=unit(item.unit,path).scale;}}
  }
  if(!columns.some(column=>column.key===dataset.xColumn))fail(path,`declared X column ${dataset.xColumn} is missing; no checked mapping is available.`);
  for(const key of action.tableColumns||[])if(!columns.some(column=>column.key===key))fail(path,`declared table column ${key} is missing.`);
  const table=createTableData({id:dataset.id,name:dataset.name,rows,columns,defaultXColumn:dataset.xColumn});
  const selection=resolveTableSelection(table,{xColumn:dataset.xColumn,filters:dataset.filters||{}},{mode:'slice',unavailable:'error'});
  if(selection.error)fail(path,selection.error);
  const selected=rows.filter(row=>Object.entries(selection.filters).every(([key,value])=>row[key]===value));
  if(!selected.length)fail(path,'no rows match the selected parameters.');
  const study=(model?.model?.study||[]).find(study=>study.id===dataset.source?.studyId);
  const axis=study?.parameterSweep?.parameters?.find(axis=>axis.parameter===dataset.xColumn);
  if(axis?.values?.type==='linear'&&selected.some((row,index)=>index>0&&row[dataset.xColumn]<selected[index-1][dataset.xColumn]))fail(path,'ordered sweep coordinates are not monotonic.');
  const label=item=>item.name||(file?item.column:item.column.endsWith('_re')?`Re(${item.column.slice(0,-3)})`:item.column.endsWith('_im')?`Im(${item.column.slice(0,-3)})`:owns(dataset.storedVariableExpressions,item.column)?`Re(${item.column})`:item.column);
  return {dataset:structuredClone(dataset),name:dataset.name||dataset.id,style:dataset.style||'solid',series:series.map(item=>({...item,label:label(item)})),table,
    view:{...table,...selection,xColumn:dataset.xColumn,rows:selected},runId:dataset.runId,modelFreshness:dataset.modelFreshness};
}
