// Keep harmonic boundary expressions and transient real-time expressions in
// their own scopes while compiling both from each scenario's declarations.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
import {composeScenarioContracts} from '../../js/scenario-discovery.js';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const read=path=>JSON.parse(readFileSync(path,'utf8'));
const condition=type=>({type,subject:{literal:'transient'},other:{select:{from:{path:'/model/study',source:'root'},kind:'items',value:{field:'/type'}}}});
const harmonic=type=>({type,subject:{literal:'frequencyDomain'},other:{select:{from:{path:'/model/study',source:'root'},kind:'items',value:{field:'/type'}}}});
const descriptors=['x','y','z','t'].map(symbol=>({requiresRuntimeSample:true,symbol,unit:symbol==='t'?'s':'m'}));
export function syncTransientWaveBoundaryValidation({check=true}={}){
  const base=read(resolve(root,'../fem.schema.json')),registry=read(resolve(root,'src/scenarios/registry.json'));
  for(const entry of registry.packages.filter(x=>/^(acoustic|em)-[23]d$/.test(x.id))){
    const path=resolve(root,'src/scenarios',entry.file),pkg=read(path),before=JSON.stringify(pkg);
    const program=pkg.schemaContributions['x-fullmeta-rules'],ob=program.declarative.obligations;
    const originals=ob.filter(row=>!row.id.endsWith('_transient')&&!row.id.endsWith('_mixed')&&row.scope?.path?.startsWith('/model/physics')&&row.constraint?.check==='quantity'
      &&row.constraint.inputs?.some(input=>input.name==='symbols'&&input.operand?.binding==='boundaryWaveSymbols'));
    if(!originals.length)throw Error(`${entry.id}: no wave boundary quantity rules found`);
    const binding={id:'transient_wave_boundary_runtime_symbols',name:'Transient wave boundary runtime symbol dimensions',stage:'model',
      scope:{source:'root',path:'',nested:[]},constraint:{type:'semanticCheck',check:'expression',expose:'transientWaveBoundarySymbols',
        subject:{binding:'parameters',path:'/symbols'},subjectInput:'symbols',inputs:[{name:'kind',operand:{literal:'runtimeSymbols'}},{name:'descriptors',operand:{literal:descriptors}}]},
      diagnostic:{message:'Transient wave boundary runtime symbols must have declared dimensions.',path:'/model/physics'}};
    const bi=ob.findIndex(row=>row.id===binding.id);if(bi<0)ob.push(binding);else ob[bi]=binding;
    const mixedBinding=structuredClone(binding);mixedBinding.id='mixed_wave_boundary_runtime_symbols';mixedBinding.name='Mixed-study wave boundary runtime symbols';
    mixedBinding.constraint.expose='mixedWaveBoundarySymbols';mixedBinding.diagnostic.message='Mixed-study wave boundary runtime symbols must have declared dimensions.';
    mixedBinding.constraint.inputs.find(input=>input.name==='descriptors').operand.literal=[...descriptors,{requiresRuntimeSample:true,symbol:'freq',unit:'Hz'},{requiresRuntimeSample:true,symbol:'frequency',unit:'Hz'},{requiresRuntimeSample:true,symbol:'omega',unit:'Hz'}];
    const mb=ob.findIndex(row=>row.id===mixedBinding.id);if(mb<0)ob.push(mixedBinding);else ob[mb]=mixedBinding;
    const generatedRows=[];
    for(const original of originals){
      const old=original.when?.all?.filter(row=>!isDeepStrictEqual(row,condition('notMemberOf')))??(original.when?[original.when]:[]);
      original.when={all:[...old,condition('notMemberOf')]};
      const transient=structuredClone(original);transient.id+='_transient';transient.name+=' (transient)';
      transient.when={all:[condition('memberOf'),harmonic('notMemberOf')]};
      transient.diagnostic.message=`Transient ${original.diagnostic.message}`;
      transient.constraint.inputs=transient.constraint.inputs.filter(input=>!['complex','imaginarySymbol','complexFunctions','runtimeSymbols'].includes(input.name));
      transient.constraint.inputs.find(input=>input.name==='symbols').operand.binding='transientWaveBoundarySymbols';
      transient.constraint.inputs.push({name:'runtimeSymbols',operand:{literal:descriptors}});
      const i=ob.findIndex(row=>row.id===transient.id);if(i<0)ob.push(transient);else ob[i]=transient;
      const mixed=structuredClone(original);mixed.id+='_mixed';mixed.name+=' (mixed studies)';
      mixed.when={all:[...old,condition('memberOf'),harmonic('memberOf')]};
      mixed.diagnostic.message=`Mixed-study ${original.diagnostic.message}`;
      mixed.constraint.inputs.find(input=>input.name==='symbols').operand.binding='mixedWaveBoundarySymbols';
      mixed.constraint.inputs.find(input=>input.name==='runtimeSymbols').operand.literal=mixedBinding.constraint.inputs.find(input=>input.name==='descriptors').operand.literal;
      const m=ob.findIndex(row=>row.id===mixed.id);if(m<0)ob.push(mixed);else ob[m]=mixed;
      generatedRows.push(original,transient,mixed);
    }
    const generated=composeScenarioContracts(base,[{registry:entry,contract:pkg}]).schema['x-fullmeta-rules'].rules;
    for(const row of generatedRows){
      const match=value=>JSON.stringify(value).includes(JSON.stringify(row.diagnostic.message));
      const source=generated.filter(match),indices=program.rules.flatMap((value,index)=>match(value)?[index]:[]);
      if(source.length!==1||indices.length>1)throw Error(`${entry.id}/${row.id}: ambiguous generated rule`);
      if(indices.length)program.rules[indices[0]]=source[0];else program.rules.push(source[0]);
    }
    for(const bindingName of ['transientWaveBoundarySymbols','mixedWaveBoundarySymbols']){
      const derived=generated.filter(value=>value.bind===`__derived_${bindingName}`);
      if(derived.length!==1)throw Error(`${entry.id}: missing ${bindingName} binding`);
      const index=program.rules.findIndex(value=>value.bind===`__derived_${bindingName}`);
      if(index<0){const messages=generatedRows.filter(row=>row.id.endsWith(bindingName.startsWith('mixed')?'_mixed':'_transient')).map(row=>JSON.stringify(row.diagnostic.message));
        const first=program.rules.findIndex(value=>messages.some(message=>JSON.stringify(value).includes(message)));program.rules.splice(first<0?program.rules.length:first,0,derived[0]);}
      else program.rules[index]=derived[0];
    }
    if(JSON.stringify(pkg)!==before){if(check)throw Error(`${entry.id}: transient boundary validation is stale`);writeFileSync(path,`${JSON.stringify(pkg,null,2)}\n`);}
  }
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))syncTransientWaveBoundaryValidation({check:process.argv[2]!=='--write'});
