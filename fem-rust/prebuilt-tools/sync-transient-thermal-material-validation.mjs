// Keep the stationary material scope intact while declaring space/time scope
// for models containing a transient Study. JSON owns the quantity units.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
import {composeScenarioContracts} from '../../js/scenario-discovery.js';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const read=p=>JSON.parse(readFileSync(p,'utf8'));
const condition=type=>({type,subject:{literal:'transient'},other:{select:{from:{path:'/model/study',source:'root'},kind:'items',value:{field:'/type'}}}});
const descriptors=['x','y','z','t'].map(symbol=>({requiresRuntimeSample:true,symbol,unit:symbol==='t'?'s':'m'}));
const unique=(rows,fn,label)=>{const found=rows.filter(fn);if(found.length!==1)throw Error(`${label}: expected one result, found ${found.length}`);return found[0];};
export function syncTransientThermalMaterialValidation({check=true}={}) {
  const base=read(resolve(root,'../fem.schema.json')),registry=read(resolve(root,'src/scenarios/registry.json'));
  for(const entry of registry.packages.filter(x=>/^thermal-[23]d$/.test(x.id))) {
    const path=resolve(root,'src/scenarios',entry.file),pkg=read(path),before=JSON.stringify(pkg);
    const program=pkg.schemaContributions['x-fullmeta-rules'],ob=program.declarative.obligations;
    const original=unique(ob,x=>x.id.startsWith('material_quantity_thermalConductivity')&&!x.id.endsWith('_transient'),entry.id);
    if(!original.when.all.some(x=>isDeepStrictEqual(x,condition('notMemberOf')))) original.when.all.push(condition('notMemberOf'));
    const binding={id:'transient_thermal_material_runtime_symbols',name:'Transient thermal material runtime symbol dimensions',stage:'model',
        scope:{source:'root',path:'',nested:[]},
        constraint:{type:'semanticCheck',check:'expression',expose:'transientThermalMaterialSymbols',subject:{binding:'parameters',path:'/symbols'},subjectInput:'symbols',
          inputs:[{name:'kind',operand:{literal:'runtimeSymbols'}},{name:'descriptors',operand:{literal:descriptors}}]},
        diagnostic:{message:'Transient thermal material runtime symbols must have declared dimensions.',path:'/model/materials'}};
    const bindingIndex=ob.findIndex(x=>x.id===binding.id);
    if(bindingIndex<0)ob.push(binding);else ob[bindingIndex]=binding;
    const copy=structuredClone(original);copy.id+='_transient';copy.name+=' (transient)';
    copy.diagnostic.message='Transient material conductivity must be a valid quantity.';
    copy.when.all=copy.when.all.filter(x=>!isDeepStrictEqual(x,condition('notMemberOf')));copy.when.all.push(condition('memberOf'));
    copy.constraint.inputs.find(x=>x.name==='symbols').operand.binding='transientThermalMaterialSymbols';
    copy.constraint.inputs.push({name:'runtimeSymbols',operand:{literal:descriptors}});
    const copyIndex=ob.findIndex(x=>x.id===copy.id);
    if(copyIndex<0)ob.push(copy);else ob[copyIndex]=copy;
    const properties=pkg.schemaDocument['x-fem-scenario'].materials.properties;
    const capacityIndex=properties.findIndex(x=>x.key==='volumetricHeatCapacity');
    if(capacityIndex<0)throw Error(`${entry.id}: transient capacity declaration is missing`);
    const capacity=structuredClone(copy);
    capacity.id='material_quantity_volumetricHeatCapacity_transient';
    capacity.name='Transient volumetric heat capacity evaluates in space and time';
    capacity.constraint.subject.field='/volumetricHeatCapacity';
    capacity.constraint.inputs.find(x=>x.name==='expectedUnit').operand.literal=properties[capacityIndex].unit;
    capacity.constraint.inputs.find(x=>x.name==='bounds').operand.path=`/$defs/femProfiles/${entry.id}/x-fem-scenario/materials/properties/${capacityIndex}/bounds`;
    capacity.when.all.find(x=>x.subject?.field==='/thermalConductivity').subject.field='/volumetricHeatCapacity';
    capacity.diagnostic={message:'Transient volumetric heat capacity must be a valid quantity.',path:'/volumetricHeatCapacity'};
    const capacityObligation=ob.findIndex(x=>x.id===capacity.id);
    if(capacityObligation<0)ob.push(capacity);else ob[capacityObligation]=capacity;
    const required={id:'material_required_volumetricHeatCapacity_transient',name:'Volumetric heat capacity is required for transient studies',
      stage:'model',scope:structuredClone(capacity.scope),constraint:{type:'required',subject:{field:'/volumetricHeatCapacity'}},
      when:{all:capacity.when.all.filter(x=>x.subject?.field!=='/volumetricHeatCapacity')},
      diagnostic:{message:'Volumetric heat capacity is required for a transient Study.',path:'/volumetricHeatCapacity'}};
    const requiredIndex=ob.findIndex(x=>x.id===required.id);
    if(requiredIndex<0)ob.push(required);else ob[requiredIndex]=required;
    const boundaryRules=[];
    for(const id of ['boundary_quantity_thermal_value',`${entry.id.replace('-','')}_neumann_field_value`]){
      const row=unique(ob,x=>x.id===id,`${entry.id}/${id}`);
      row.when=condition('notMemberOf');
      const transient=structuredClone(row);
      transient.id=`${id}_transient`;
      transient.name+= ' (transient)';
      transient.when=condition('memberOf');
      transient.diagnostic.message=`Transient ${row.diagnostic.message}`;
      transient.constraint.inputs.find(x=>x.name==='symbols').operand.binding='transientThermalMaterialSymbols';
      transient.constraint.inputs.push({name:'runtimeSymbols',operand:{literal:descriptors}});
      const index=ob.findIndex(x=>x.id===transient.id);
      if(index<0)ob.push(transient);else ob[index]=transient;
      boundaryRules.push(row,transient);
    }
    const generated=composeScenarioContracts(base,[{registry:entry,contract:pkg}]).schema['x-fullmeta-rules'].rules;
    for(const row of [original,copy,capacity,required,...boundaryRules]){
      const match=x=>JSON.stringify(x).includes(JSON.stringify(row.diagnostic.message));
      const rule=unique(generated,match,`${entry.id}/${row.id}`),matches=program.rules.map((x,i)=>match(x)?i:-1).filter(i=>i>=0);
      if(matches.length>1)throw Error(`${entry.id}/${row.id}: duplicate rule`);
      if(matches.length)program.rules[matches[0]]=rule;else program.rules.push(rule);
    }
    const derived=unique(generated,x=>x.bind==='__derived_transientThermalMaterialSymbols',entry.id);
    const index=program.rules.findIndex(x=>x.bind==='__derived_transientThermalMaterialSymbols');
    if(index<0){const first=program.rules.findIndex(x=>JSON.stringify(x).includes('Transient material conductivity'));program.rules.splice(first,0,derived);}else program.rules[index]=derived;
    if(JSON.stringify(pkg)!==before){if(check)throw Error(`${entry.id}: transient thermal validation is stale; run with --write`);writeFileSync(path,`${JSON.stringify(pkg,null,2)}\n`);}
  }
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  if(process.argv.length>3||(process.argv[2]&&!['--check','--write'].includes(process.argv[2])))throw Error('Use --check or --write.');
  syncTransientThermalMaterialValidation({check:process.argv[2]!=='--write'});
}
