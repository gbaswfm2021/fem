// Generate elastic time-domain quantity checks from the shared boundary
// validation vocabulary and each formulation's declared component units.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {composeScenarioContracts} from '../../js/scenario-discovery.js';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const read=path=>JSON.parse(readFileSync(path,'utf8'));
export function syncTransientElasticBoundaryValidation({check=true}={}){
  const base=read(resolve(root,'../fem.schema.json')),registry=read(resolve(root,'src/scenarios/registry.json'));
  const acoustic=read(resolve(root,'src/scenarios/acoustic2d.json')).schemaContributions['x-fullmeta-rules'].declarative.obligations;
  const em=read(resolve(root,'src/scenarios/em3d.json')).schemaContributions['x-fullmeta-rules'].declarative.obligations;
  const source=(rows,id)=>{const result=rows.find(row=>row.id===id);if(!result)throw Error(`Missing source rule ${id}`);return structuredClone(result);};
  for(const entry of registry.packages.filter(row=>/^elastic-[23]d$/.test(row.id))){
    const path=resolve(root,'src/scenarios',entry.file),pkg=read(path),before=JSON.stringify(pkg);
    const program=pkg.schemaContributions['x-fullmeta-rules'],ob=program.declarative.obligations;
    for(const id of ['transient_wave_boundary_runtime_symbols','mixed_wave_boundary_runtime_symbols']){
      const binding=source(acoustic,id),bi=ob.findIndex(row=>row.id===binding.id);if(bi<0)ob.push(binding);else ob[bi]=binding;
    }
    const rules=[];
    for(const id of ['boundary_quantity_wave_value_transient','boundary_quantity_wave_impedance_transient','boundary_quantity_wave_value_mixed','boundary_quantity_wave_impedance_mixed']){
      const row=source(acoustic,id);row.id=`${entry.id.replace('-','')}_${id}`;row.name=`${entry.id} ${row.name}`;
      row.diagnostic.message=`${entry.id}: ${row.diagnostic.message}`;
      const conditions=row.scope.when.all;
      const family=conditions.find(value=>value.subject?.path==='/scenario/physics');
      family.type='equals';family.other.literal='elasticity';
      conditions.push({type:'equals',subject:{path:'/scenario/profileId',source:'root'},other:{literal:entry.id}});
      rules.push(row);
    }
    const defaults=pkg.schemaDocument['x-fem-physics'].boundaryDefaultsByFormulation;
    for(const type of ['dirichlet','neumann'])for(const kind of ['transient','mixed']){
      const row=source(em,`em3d_${type}_vector_value_${kind}`);
      row.id=`${entry.id.replace('-','')}_${type}_vector_value_${kind}`;row.name=`${entry.id} ${kind} ${type} vector values`;
      row.diagnostic.message=`${entry.id}: ${kind} ${type} vector component must be a valid quantity.`;
      const conditions=row.scope.when.all;
      const family=conditions.find(value=>value.subject?.path==='/scenario/physics');family.type='equals';family.other.literal='elasticity';
      conditions.find(value=>value.subject?.path==='/scenario/profileId').other.literal=entry.id;
      const units=[...new Set(Object.values(defaults).flatMap(form=>form[type]?.fieldValues?.map(value=>value.unit)||[]))];
      if(units.length!==1)throw Error(`${entry.id}/${type}: vector units must be declared consistently`);
      row.constraint.inputs.find(value=>value.name==='expectedUnit').operand.literal=units[0];
      rules.push(row);
    }
    for(const row of rules){const i=ob.findIndex(value=>value.id===row.id);if(i<0)ob.push(row);else ob[i]=row;}
    const generated=composeScenarioContracts(base,[{registry:entry,contract:pkg}]).schema['x-fullmeta-rules'].rules;
    for(const name of ['transientWaveBoundarySymbols','mixedWaveBoundarySymbols']){
      const derived=generated.filter(value=>value.bind===`__derived_${name}`);
      if(derived.length!==1)throw Error(`${entry.id}: missing ${name} symbol binding`);
      const di=program.rules.findIndex(value=>value.bind===`__derived_${name}`);
      if(di<0)program.rules.push(derived[0]);else program.rules[di]=derived[0];
    }
    for(const row of rules){const match=value=>JSON.stringify(value).includes(JSON.stringify(row.diagnostic.message));
      const items=generated.filter(match),indices=program.rules.flatMap((value,index)=>match(value)?[index]:[]);
      if(items.length!==1||indices.length>1)throw Error(`${entry.id}/${row.id}: ambiguous generated rule`);
      if(indices.length)program.rules[indices[0]]=items[0];else program.rules.push(items[0]);}
    if(JSON.stringify(pkg)!==before){if(check)throw Error(`${entry.id}: transient elastic boundary validation is stale`);writeFileSync(path,`${JSON.stringify(pkg,null,2)}\n`);}
  }
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))syncTransientElasticBoundaryValidation({check:process.argv[2]!=='--write'});
