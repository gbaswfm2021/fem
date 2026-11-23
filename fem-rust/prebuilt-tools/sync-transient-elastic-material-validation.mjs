// Build transient elastic material checks from the scenario's property and
// constitutive declarations, using the shared real quantity validator.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {composeScenarioContracts} from '../../js/scenario-discovery.js';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const read=path=>JSON.parse(readFileSync(path,'utf8'));
const study={type:'memberOf',subject:{literal:'transient'},other:{select:{from:{path:'/model/study',source:'root'},kind:'items',value:{field:'/type'}}}};
// Model validation is shared by all studies: retain harmonic frequency
// symbols here. Transient preparation rejects them for the selected study.
const descriptors=['x','y','z','freq','omega','t'].map(symbol=>({requiresRuntimeSample:true,symbol,unit:symbol==='t'?'s':['freq','omega'].includes(symbol)?'Hz':'m'}));
export function syncTransientElasticMaterialValidation({check=true}={}){
  const base=read(resolve(root,'../fem.schema.json')),registry=read(resolve(root,'src/scenarios/registry.json'));
  const acoustic=read(resolve(root,'src/scenarios/acoustic2d.json')).schemaContributions['x-fullmeta-rules'].declarative.obligations;
  const em=read(resolve(root,'src/scenarios/em2d.json')).schemaContributions['x-fullmeta-rules'].declarative.obligations;
  const copy=(rows,id)=>{const item=rows.find(value=>value.id===id);if(!item)throw Error(`Missing ${id}`);return structuredClone(item);};
  for(const entry of registry.packages.filter(row=>/^elastic-[23]d$/.test(row.id))){
    const path=resolve(root,'src/scenarios',entry.file),pkg=read(path),before=JSON.stringify(pkg);
    const program=pkg.schemaContributions['x-fullmeta-rules'],ob=program.declarative.obligations;
    const binding={id:'transient_elastic_material_runtime_symbols',name:'Transient elastic material runtime symbols',stage:'model',scope:{source:'root',path:'',nested:[]},
      constraint:{type:'semanticCheck',check:'expression',expose:'transientElasticMaterialSymbols',subject:{binding:'parameters',path:'/symbols'},subjectInput:'symbols',
        inputs:[{name:'kind',operand:{literal:'runtimeSymbols'}},{name:'descriptors',operand:{literal:descriptors}}]},
      diagnostic:{message:'Transient elastic material symbols must have valid dimensions.',path:'/model/materials'}};
    const bi=ob.findIndex(row=>row.id===binding.id);if(bi<0)ob.push(binding);else ob[bi]=binding;
    const scenario=pkg.schemaDocument['x-fem-scenario'],forms=Object.values(pkg.transientForms),rules=[];
    for(const property of scenario.materials.properties){
      const bindings=forms.map(form=>form.materialBindings.find(value=>value.source===property.symbol)).filter(Boolean);
      if(!bindings.length)continue;
      if(bindings.some(value=>JSON.stringify(value.bounds)!==JSON.stringify(bindings[0].bounds)||JSON.stringify(value.implementedValues)!==JSON.stringify(bindings[0].implementedValues)))
        throw Error(`${entry.id}/${property.key}: formulation-dependent material validation needs an explicit contract`);
      const row=copy(property.representation==='quantity'?acoustic:em,property.representation==='quantity'?'material_quantity_density_transient':'material_scalar_relativePermittivity_transient');
      row.id=`${entry.id.replace('-','')}_material_${property.key}_transient`;row.name=`${entry.id} transient ${property.key}`;
      row.diagnostic={message:`${entry.id}: transient material ${property.key} must be a valid real quantity.`,path:`/${property.key}`};
      row.when.all[0].other.literal=[entry.id];row.when.all[1].subject.field=`/${property.key}`;
      if(property.representation==='quantity')row.constraint.subject.field=`/${property.key}`;
      else row.constraint.subject.object.expression.field=`/${property.key}`;
      row.constraint.inputs=row.constraint.inputs.filter(value=>!['complex','imaginarySymbol','complexFunctions','runtimeSymbols','bounds'].includes(value.name));
      row.constraint.inputs.find(value=>value.name==='symbols').operand.binding='transientElasticMaterialSymbols';
      row.constraint.inputs.find(value=>value.name==='expectedUnit').operand.literal=property.unit;
      row.constraint.inputs.push({name:'runtimeSymbols',operand:{literal:descriptors}});
      const bounds={...bindings[0].bounds};
      if(Object.keys(bounds).length)row.constraint.inputs.push({name:'bounds',operand:{literal:bounds}});
      rules.push(row);
      if(property.required===true){
        const required=copy(acoustic,'material_required_density');required.id=`${entry.id.replace('-','')}_material_${property.key}_required_transient`;
        required.name=`${entry.id} transient ${property.key} is required`;required.constraint.subject.field=`/${property.key}`;
        required.diagnostic={message:`${entry.id}: transient material ${property.key} is required.`,path:`/${property.key}`};
        required.when={all:[{type:'equals',subject:{path:'/scenario/profileId',source:'root'},other:{literal:entry.id}},study]};rules.push(required);
      }
    }
    for(const row of rules){const index=ob.findIndex(value=>value.id===row.id);if(index<0)ob.push(row);else ob[index]=row;}
    const generated=composeScenarioContracts(base,[{registry:entry,contract:pkg}]).schema['x-fullmeta-rules'].rules;
    const derived=generated.filter(value=>value.bind==='__derived_transientElasticMaterialSymbols');if(derived.length!==1)throw Error(`${entry.id}: missing binding`);
    const di=program.rules.findIndex(value=>value.bind==='__derived_transientElasticMaterialSymbols');if(di<0)program.rules.push(derived[0]);else program.rules[di]=derived[0];
    for(const row of rules){const match=value=>JSON.stringify(value).includes(JSON.stringify(row.diagnostic.message)),found=generated.filter(match),indices=program.rules.flatMap((value,index)=>match(value)?[index]:[]);
      if(found.length!==1||indices.length>1)throw Error(`${entry.id}/${row.id}: ambiguous rule`);if(indices.length)program.rules[indices[0]]=found[0];else program.rules.push(found[0]);}
    if(JSON.stringify(pkg)!==before){if(check)throw Error(`${entry.id}: transient material validation is stale`);writeFileSync(path,`${JSON.stringify(pkg,null,2)}\n`);}
  }
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))syncTransientElasticMaterialValidation({check:process.argv[2]!=='--write'});
