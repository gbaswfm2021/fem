import React from '../../../../../platform/ui/widgets/shared/react/react-runtime.js';
import {mountReactComponent} from '../../../../../platform/ui/widgets/shared/react/mount.js';

/** Literal CSV headers remain map keys; the normal field transaction validates units. */
export function Widget(props){
  const {label='CSV source units',value={},rawValue,path=[],schema={},context={},onCommand=context.onCommand,disabled=false}={...props,...props.view};
  const pending=rawValue??context.readDraft?.(path);
  const text=pending===undefined?JSON.stringify(value,null,2):String(pending);
  const columns=schema.type==='array'||Array.isArray(value);
  return <tr><td>{label}</td><td><textarea aria-label={label} data-setting-path={JSON.stringify(path)}
    data-setting-type="json-value" rows={4} value={text} disabled={disabled||schema.readOnly===true}
    onChange={event=>onCommand?.({type:'field.change',path,value:event.target.value,rawValue:event.target.value,valueType:'json-value'},event)}/>
    <div className="settings-empty">{columns?'Enter an ordered JSON array of literal CSV column headers.':<>Map literal CSV column headers to units. Use {'{}'} to leave columns unmapped. Display units are configured separately.</>}</div>
  </td></tr>;
}
export const mountWidget=(container,props)=>mountReactComponent(container,Widget,props);
