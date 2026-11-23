// Add a shared real space-time Domain Source scope without changing harmonic
// frequency semantics. Both declarative and native executable rules live in
// the application schema and are regenerated together.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
import {compileDeclarativeValidation} from '../../../../platform/ui/widgets/validation-program/declarative-rules.js';
const root=resolve(fileURLToPath(new URL('../..',import.meta.url)));
const schemaPath=resolve(root,'fem.schema.json');
const condition=type=>({type,subject:{literal:'transient'},other:{select:{from:{path:'/model/study',source:'root'},kind:'items',value:{field:'/type'}}}});
const harmonic=type=>({type,subject:{literal:'frequencyDomain'},other:{select:{from:{path:'/model/study',source:'root'},kind:'items',value:{field:'/type'}}}});
const descriptors=['x','y','z','t'].map(symbol=>({requiresRuntimeSample:true,symbol,unit:symbol==='t'?'s':'m'}));
export function syncTransientDomainSourceValidation({check=true}={}){
  const schema=JSON.parse(readFileSync(schemaPath,'utf8')),before=JSON.stringify(schema);
  const program=schema['x-fullmeta-rules'],ob=program.declarative.obligations;
  const binding={id:'domain_source_symbols_transient',name:'Transient Domain Source coordinate and time symbols',stage:'model',
    scope:{source:'root',path:'',nested:[]},constraint:{type:'semanticCheck',check:'expression',expose:'domainSourceSymbolsTransient',
      subject:{binding:'parameters',path:'/symbols'},subjectInput:'symbols',inputs:[{name:'kind',operand:{literal:'runtimeSymbols'}},{name:'descriptors',operand:{literal:descriptors}}]},
    diagnostic:{message:'Transient Domain Source expression symbols must be valid.',path:'/model/physics'}};
  const bi=ob.findIndex(row=>row.id===binding.id);if(bi<0)ob.push(binding);else ob[bi]=binding;
  const mixedBinding=structuredClone(binding);mixedBinding.id='domain_source_symbols_mixed';mixedBinding.name='Mixed-study Domain Source runtime symbols';
  mixedBinding.constraint.expose='domainSourceSymbolsMixed';mixedBinding.diagnostic.message='Mixed-study Domain Source expression symbols must be valid.';
  mixedBinding.constraint.inputs.find(value=>value.name==='descriptors').operand.literal=[...descriptors,{requiresRuntimeSample:true,symbol:'freq',unit:'Hz'},{requiresRuntimeSample:true,symbol:'omega',unit:'Hz'},{requiresRuntimeSample:true,symbol:'frequency',unit:'Hz'}];
  const mi=ob.findIndex(row=>row.id===mixedBinding.id);if(mi<0)ob.push(mixedBinding);else ob[mi]=mixedBinding;
  for(const id of ['domain_source_quantity_thermal','domain_source_quantity_wave']){
    const original=ob.find(row=>row.id===id);if(!original)throw Error(`Missing ${id}`);
    const old=original.when?.all?.filter(value=>!isDeepStrictEqual(value,condition('notMemberOf')))??(original.when?[original.when]:[]);
    original.when={all:[...old,condition('notMemberOf')]};
    const transient=structuredClone(original);transient.id+='_transient';transient.name+=' (transient)';
    transient.when=id.endsWith('_wave')?{all:[condition('memberOf'),harmonic('notMemberOf')]}:condition('memberOf');
    transient.diagnostic.message='Transient Domain Source must use the declared unit and a valid real space-time expression.';
    transient.constraint.inputs=transient.constraint.inputs.filter(value=>!['complex','imaginarySymbol','complexFunctions','runtimeSymbols'].includes(value.name));
    transient.constraint.inputs.find(value=>value.name==='symbols').operand.binding='domainSourceSymbolsTransient';
    transient.constraint.inputs.push({name:'runtimeSymbols',operand:{literal:descriptors}});
    const i=ob.findIndex(row=>row.id===transient.id);if(i<0)ob.push(transient);else ob[i]=transient;
    if(id.endsWith('_wave')){
      const mixed=structuredClone(original);mixed.id+='_mixed';mixed.name+=' (mixed studies)';mixed.when={all:[condition('memberOf'),harmonic('memberOf')]};
      mixed.diagnostic.message='Mixed-study Domain Source must use the declared unit and a valid expression.';
      mixed.constraint.inputs.find(value=>value.name==='symbols').operand.binding='domainSourceSymbolsMixed';
      mixed.constraint.inputs.find(value=>value.name==='runtimeSymbols').operand.literal=mixedBinding.constraint.inputs.find(value=>value.name==='descriptors').operand.literal;
      const index=ob.findIndex(row=>row.id===mixed.id);if(index<0)ob.push(mixed);else ob[index]=mixed;
    }
  }
  program.rules=compileDeclarativeValidation(ob);
  if(JSON.stringify(schema)!==before){if(check)throw Error('Transient Domain Source validation is stale');writeFileSync(schemaPath,`${JSON.stringify(schema,null,2)}\n`);}
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))syncTransientDomainSourceValidation({check:process.argv[2]!=='--write'});
