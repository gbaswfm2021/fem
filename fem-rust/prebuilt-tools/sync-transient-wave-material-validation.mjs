// Add a Study-aware expression scope without changing legacy-only models.
// The scenario's existing material rules remain the source of units and paths.
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
import {composeScenarioContracts} from '../../js/scenario-discovery.js';

const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const read=p=>JSON.parse(readFileSync(p,'utf8'));
const marker='transient_material_runtime_symbols';
const condition=type=>({type,subject:{literal:'transient'},other:{select:{
  from:{path:'/model/study',source:'root'},kind:'items',value:{field:'/type'}}}});
const unique=(rows,predicate,label)=>{
  const matches=rows.filter(predicate);
  if(matches.length!==1) throw Error(`${label}: expected one rule, found ${matches.length}`);
  return matches[0];
};
export function syncTransientWaveMaterialValidation({check=true}={}) {
  const base=read(resolve(root,'../fem.schema.json'));
  const registry=read(resolve(root,'src/scenarios/registry.json'));
  for(const entry of registry.packages) {
    const file=resolve(root,'src/scenarios',entry.file),pkg=read(file);
    const program=pkg.schemaContributions?.['x-fullmeta-rules'];
    const obligations=program?.declarative?.obligations;
    const wave=obligations?.find(row=>row.id==='wave_material_runtime_symbols');
    if(!wave) continue;
    const before=JSON.stringify(pkg);
    const binding=structuredClone(wave);binding.id=marker;
    binding.name='Transient material runtime symbol dimensions';
    binding.diagnostic.message='Transient material runtime symbols must have declared dimensions.';
    binding.constraint.expose='transientMaterialSymbols';
    binding.constraint.inputs.find(row=>row.name==='descriptors').operand.literal.push(
      {requiresRuntimeSample:true,symbol:'t',unit:'s'});
    const bindingIndex=obligations.findIndex(row=>row.id===marker);
    if(bindingIndex<0) obligations.push(binding); else obligations[bindingIndex]=binding;
    const materialRules=obligations.filter(row=>
      /^material_(quantity|scalar)_/.test(row.id) && !row.id.endsWith('_transient') &&
      row.constraint?.inputs?.some(input=>input.operand?.binding==='waveMaterialSymbols'));
    for(const original of materialRules) {
      if(!original.when?.all) throw Error(`${entry.id}/${original.id}: expected existing conditions`);
      if(!original.when.all.some(x=>isDeepStrictEqual(x,condition('notMemberOf'))))
        original.when.all.push(condition('notMemberOf'));
      const copy=structuredClone(original);copy.id+='_transient';
      copy.name+= ' (transient)';copy.diagnostic.message=`Transient ${copy.diagnostic.message}`;
      copy.when.all=copy.when.all.filter(x=>!isDeepStrictEqual(x,condition('notMemberOf')));
      copy.when.all.push(condition('memberOf'));
      copy.constraint.inputs.find(row=>row.name==='symbols').operand.binding='transientMaterialSymbols';
      copy.constraint.inputs.find(row=>row.name==='runtimeSymbols').operand.literal.push(
        {requiresRuntimeSample:true,symbol:'t',unit:'s'});
      const copyIndex=obligations.findIndex(row=>row.id===copy.id);
      if(copyIndex<0) obligations.push(copy); else obligations[copyIndex]=copy;
    }
    // Composition compiles with all required derived bindings in scope. Match
    // whole executable rules by unique diagnostic, never by obligation index.
    const generated=composeScenarioContracts(base,[{registry:entry,contract:pkg}]).schema['x-fullmeta-rules'].rules;
    for(const original of materialRules) {
      for(const row of [original,obligations.find(x=>x.id===`${original.id}_transient`)]) {
        const message=row.diagnostic.message;
        const match=x=>JSON.stringify(x).includes(JSON.stringify(message));
        const compiled=unique(generated,match,`${entry.id}/${row.id}`);
        const found=program.rules.map((x,i)=>match(x)?i:-1).filter(i=>i>=0);
        if(found.length>1) throw Error(`${entry.id}/${row.id}: duplicate executable rules`);
        if(found.length) program.rules[found[0]]=compiled; else program.rules.push(compiled);
      }
    }
    const derived=unique(generated,x=>x.bind==='__derived_transientMaterialSymbols',entry.id);
    const index=program.rules.findIndex(x=>x.bind==='__derived_transientMaterialSymbols');
    if(index<0) {
      // The binding must precede every generated transient material check.
      const first=program.rules.findIndex(x=>JSON.stringify(x).includes('Transient Material '));
      program.rules.splice(first<0?program.rules.length:first,0,derived);
    } else program.rules[index]=derived;
    if(JSON.stringify(pkg)!==before) {
      if(check) throw Error(`${entry.id}: transient material validation is stale; run with --write`);
      writeFileSync(file,`${JSON.stringify(pkg,null,2)}\n`);
    }
  }
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  if(process.argv.length>3 || (process.argv[2] && !['--check','--write'].includes(process.argv[2]))) throw Error('Use --check or --write.');
  syncTransientWaveMaterialValidation({check:process.argv[2]!=='--write'});
}
