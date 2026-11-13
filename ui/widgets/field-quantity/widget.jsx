import React from '../../../../../platform/ui/widgets/shared/react/react-runtime.js';
import {mountReactComponent} from '../../../../../platform/ui/widgets/shared/react/mount.js';

/** Combined field component/part selector; changes use the host's normal raw draft. */
export function Widget(props){
  const {label='Quantity',value,path=[],ui={},context={},onCommand,disabled=false}={...props,...props.view};
  const records=context.choiceRecords?.(ui.choiceSource)||[];
  // JSON object member order has no meaning, including after a schema default
  // or a saved document has been serialized with sorted keys.
  const identity=quantity=>JSON.stringify([quantity?.type,quantity?.component,quantity?.part]);
  const match=records.find(record=>identity(record.value)===identity(value));
  const selected=JSON.stringify(match?.value??value),unavailable=!match;
  return <tr><td>{label}</td><td><select aria-label={label} value={selected} disabled={disabled} onChange={event=>onCommand({type:'field.change',path,value:event.target.value,rawValue:event.target.value,valueType:'json-value'},event)}>
    {unavailable?<option value={selected} disabled>{value?.component||'Choose quantity'} · unavailable</option>:null}
    {records.map(record=><option key={JSON.stringify(record.value)} value={JSON.stringify(record.value)} disabled={record.disabled} title={record.reason||''}>{record.label}{record.disabled?' · no saved samples':''}</option>)}
  </select></td></tr>;
}
export const mountWidget=(container,props)=>mountReactComponent(container,Widget,props);
