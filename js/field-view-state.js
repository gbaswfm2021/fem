import {prepareFieldSurfaces,resolveCutPlanes} from '../ui/field-surfaces.js';

const settings=action=>JSON.stringify([action?.quantity,action?.cutPlanePositions]);
export function fieldComponentHasSamples(data,symbol){
  return Boolean(data?.quantities?.some(quantity=>quantity.symbol===symbol)
    ||Object.hasOwn(data?.fields||{},symbol)
    ||Object.hasOwn(data?.cellFields||{},symbol));
}
export function checkedFieldEditErrors(model,state){
  if(!state?.isCurrent)return [];
  const action=model?.model?.results?.find(action=>action.id===state.actionId);
  if(!action||settings(action)===settings(state.action)||JSON.stringify(action.filters)!==JSON.stringify(state.action.filters))return [];
  try{
    if(!fieldComponentHasSamples(state.data,action.quantity?.component))throw new Error('The selected quantity has no values in this checked sample.');
    resolveCutPlanes(state.data,action.cutPlanePositions,state.symbols);
    return [];
  }catch(error){return [`/model/results/${model.model.results.indexOf(action)}: ${error.message}`];}
}

/** Reuse checked samples for presentation-only edits; case changes use the resolver. */
export function updateCheckedFieldView(model,state){
  if(!state?.isCurrent)return false;
  const action=model?.model?.results?.find(action=>action.id===state.actionId);
  if(!action||settings(action)===settings(state.action)||JSON.stringify(action.filters)!==JSON.stringify(state.action.filters))return false;
  const errors=checkedFieldEditErrors(model,state);if(errors.length)throw new Error(errors[0]);
  const surfaces=prepareFieldSurfaces(state.data,action,{symbols:state.symbols});
  Object.assign(state,surfaces,{action:structuredClone(action)});return true;
}
