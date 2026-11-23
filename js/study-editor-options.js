/** Add scenario-declared controls for Study kinds present in the model. */
export function studyEditorOptions(scenario,model){
  const base=scenario?.editorOptions;
  const result=base&&typeof base==='object'&&!Array.isArray(base)?structuredClone(base):{};
  const declared=scenario?.editorOptionsByStudyType;
  const supported=scenario?.supportedStudyTypes||[scenario?.studyType];
  for(const study of model?.model?.study||[]){
    if(!supported.includes(study?.type))continue;
    for(const [key,value] of Object.entries(declared?.[study.type]||{}))if(value===true)result[key]=true;
  }
  return result;
}
