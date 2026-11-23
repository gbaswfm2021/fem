// Generate metadata checks from each scenario's declared mesh formulations.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
import {compileDeclarativeValidation} from '../../../../platform/ui/widgets/validation-program/declarative-rules.js';

const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
export function syncFormulationOrderValidation({check=true,scenarioRoot=root}={}) {
  const registry=JSON.parse(readFileSync(resolve(scenarioRoot,'src/scenarios/registry.json'),'utf8'));
  for (const entry of registry.packages) {
    const file=resolve(scenarioRoot,'src/scenarios',entry.file),pkg=JSON.parse(readFileSync(file,'utf8'));
    if (pkg.validationGeneration?.formulationOrders!==true) continue;
    pkg.schemaContributions ||= {};
    let program=pkg.schemaContributions['x-fullmeta-metadataValidation'],dirty=false;
    if (program===undefined) {program=pkg.schemaContributions['x-fullmeta-metadataValidation']={declarative:{obligations:[]},rules:[]};dirty=true;}
    if (!Array.isArray(program?.declarative?.obligations)||!Array.isArray(program.rules)) throw Error(`${entry.id}: invalid metadata program`);
    const forms=pkg.capabilities?.mesh?.fieldFormulations;
    if (!forms || typeof forms!=='object' || Array.isArray(forms) || !Object.keys(forms).length) throw Error(`${entry.id}: no formulation declarations`);
    const expectedIds=new Set();
    for (const [form,definition] of Object.entries(forms)) {
      const orders=definition.elementOrders;
      if (!/^[A-Za-z][A-Za-z0-9_]*$/.test(form)||!Array.isArray(orders)||!orders.length||orders.some(n=>!Number.isSafeInteger(n)||n<1)||new Set(orders).size!==orders.length) throw Error(`${entry.id}/${form}: invalid element orders`);
      const pointer=`/capabilities/mesh/fieldFormulations/${form}/elementOrders`,id=`formulation_order_${form}`;
      expectedIds.add(id);
      const obligation={id,name:`${form} element orders`,stage:'metadata',scope:{source:'root',path:'',nested:[]},
        constraint:{subject:{field:pointer},type:'setEqual',other:{literal:orders}},
        diagnostic:{path:pointer,message:`The ${form} formulation must declare element orders ${orders.join(', ')}.`}};
      const compiled=compileDeclarativeValidation([obligation])[0],list=program.declarative.obligations,index=list.findIndex(row=>row.id===id),old=index<0?undefined:list[index];
      const oldCompiled=old && compileDeclarativeValidation([old])[0];
      const matches=program.rules.map((row,i)=>isDeepStrictEqual(row,oldCompiled||compiled)?i:-1).filter(i=>i>=0);
      if (matches.length>1) throw Error(`${entry.id}/${form}: duplicate compiled order rule`);
      if (!isDeepStrictEqual(old,obligation)||matches.length!==1||!isDeepStrictEqual(program.rules[matches[0]],compiled)) {
        dirty=true;if(index<0)list.push(obligation);else list[index]=obligation;
        if(!matches.length)program.rules.push(compiled);else program.rules[matches[0]]=compiled;
      }
    }
    for(const old of [...program.declarative.obligations]) {
      if(!old.id?.startsWith('formulation_order_')||expectedIds.has(old.id))continue;
      const compiled=compileDeclarativeValidation([old])[0];
      const matches=program.rules.map((row,i)=>isDeepStrictEqual(row,compiled)?i:-1).filter(i=>i>=0);
      if(matches.length!==1)throw Error(`${entry.id}: stale formulation-order rule has no unique executable`);
      program.rules.splice(matches[0],1);program.declarative.obligations.splice(program.declarative.obligations.indexOf(old),1);dirty=true;
    }
    if(dirty&&check)throw Error(`${entry.id}: formulation order rules are stale; run node prebuilt-tools/sync-formulation-order-validation.mjs --write`);
    if(dirty)writeFileSync(file,`${JSON.stringify(pkg,null,2)}\n`);
  }
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  if(process.argv.length>3||(process.argv[2]&&!['--check','--write'].includes(process.argv[2])))throw Error('Use --check or --write.');
  syncFormulationOrderValidation({check:process.argv[2]!=='--write'});
}
