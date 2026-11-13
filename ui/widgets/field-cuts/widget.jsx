import React from '../../../../../platform/ui/widgets/shared/react/react-runtime.js';
import {mountReactComponent} from '../../../../../platform/ui/widgets/shared/react/mount.js';
import {Quantity} from '../../../../../platform/ui/widgets/quantity/widget.js';

/** Three saved cut descriptions; absent positions remain display-only mid-planes. */
export function Widget(props){
  const {label='Cut-plane positions',value={},path=[],context={},ui={},onCommand,disabled=false}={...props,...props.view};
  const records=context.choiceRecords?.(ui.choiceSource)||[];
  if(!records.length)return null;
  return <><tr className="settings-group"><th colSpan={2}>{label}</th></tr>{records.map(record=>{
    const axis=record.axis,position=value?.[axis],mode=position?.mode||'midPlane',name=axis.toUpperCase()+' position';
    const coordinate=position?.coordinate||{expression:String(record.midpoint),unit:record.unit};
    return <React.Fragment key={axis}><tr><td>{name}</td><td><select aria-label={name+' mode'} value={mode} disabled={disabled} onChange={event=>{
      const next=event.target.value==='midPlane'?{mode:'midPlane'}:{mode:'explicitValues',coordinate};
      const serialized=JSON.stringify({...value,[axis]:next});onCommand({type:'field.change',path,value:serialized,rawValue:serialized,valueType:'json-value'},event);
    }}><option value="midPlane">Mid-plane</option><option value="explicitValues" disabled={!record.available}>Explicit value</option></select></td></tr>
      {mode==='explicitValues'?<Quantity label={name} value={coordinate} path={[...path,axis,'coordinate']} context={{...context,defaultUnits:ui.units||['m','mm','um','nm'],unitChoices:()=>ui.units||['m','mm','um','nm']}} onCommand={onCommand} disabled={disabled}/>:null}
    </React.Fragment>;
  })}</>;
}
export const mountWidget=(container,props)=>mountReactComponent(container,Widget,props);
