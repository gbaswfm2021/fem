import {getCoreUnit} from '../../../platform/core-rust/js/api.js';
import {resultQuantityDescriptors} from './result-adapters.js';
import {fieldComponentHasSamples} from './field-view-state.js';

/** Project choices from declarations and checked samples without changing a recipe. */
export function fieldChoiceRecords({source,model,settingsDraft,modelSchema,state}){
  const current=state?.isCurrent&&state.actionId===settingsDraft?.id?state:null;
  if(source?.path?.[0]==='fieldQuantities')return resultQuantityDescriptors(model,modelSchema).flatMap(descriptor=>['real','imaginary','magnitude'].map(part=>{
    const disabled=Boolean(current&&!fieldComponentHasSamples(current.data,descriptor.symbol));
    return {value:{type:'fieldComponent',component:descriptor.symbol,part},label:part==='real'?`Re(${descriptor.symbol})`:part==='imaginary'?`Im(${descriptor.symbol})`:`|${descriptor.symbol}|`,
      disabled,...(disabled?{reason:'The selected checked sample has no values for this component.'}:{})};
  }));
  if(source?.path?.[0]==='fieldCutPlanes'&&model?.scenario?.dimension===3){
    const unit=model.model.units||'m',scale=getCoreUnit(unit).scale;
    return ['x','y','z'].map(axis=>({axis,unit,midpoint:current?(current.cutPlanes[axis].minimum+current.cutPlanes[axis].maximum)/2/scale:0,available:Boolean(current)}));
  }
  return [];
}
