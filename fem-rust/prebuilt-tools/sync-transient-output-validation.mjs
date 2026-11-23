// Keep time-aware global/pointwise expression scopes in scenario data. Legacy
// models retain their previous rules; a mixed model is checked per Study at
// preparation, since its shared definitions may be used by either Study.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
import {composeScenarioContracts} from '../../js/scenario-discovery.js';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const read=file=>JSON.parse(readFileSync(file,'utf8'));
const condition=type=>({type,subject:{literal:'transient'},other:{select:{from:{path:'/model/study',source:'root'},kind:'items',value:{field:'/type'}}}});
const ids=['global_definition_expressions','pointwise_definition_expressions'];
export function syncTransientOutputValidation({check=true}={}) {
  const base=read(resolve(root,'../fem.schema.json')),registry=read(resolve(root,'src/scenarios/registry.json'));
  const shared=read(resolve(root,'validation/thermal-common.json'));
  const templates=ids.map(id=>shared.rows.find(row=>row.obligation.id===`${id}_transient`)?.obligation);
  if(templates.some(row=>!row))throw Error('Shared transient output validation templates are missing');
  let thermalRows;
  for(const entry of registry.packages){
    const file=resolve(root,'src/scenarios',entry.file),pkg=read(file),program=pkg.schemaContributions?.['x-fullmeta-rules'];
    const obligations=program?.declarative?.obligations;
    if(!obligations)continue;
    const before=JSON.stringify(pkg);
    if(!ids.every(id=>obligations.some(row=>row.id===id))){
      if(!pkg.schemaDocument?.['x-fem-scenario']?.editorOptionsByStudyType?.transient?.globalVariables)continue;
      for(const template of templates){const index=obligations.findIndex(row=>row.id===template.id);
        if(index<0)obligations.push(structuredClone(template));else obligations[index]=structuredClone(template);}
      const compiled=composeScenarioContracts(base,[{registry:entry,contract:pkg}]).schema['x-fullmeta-rules'].rules;
      for(const template of templates){const match=rule=>JSON.stringify(rule).includes(JSON.stringify(template.diagnostic.message));
        const matches=compiled.filter(match);if(matches.length!==1)throw Error(`${entry.id}/${template.id}: expected one compiled rule`);
        const indices=program.rules.flatMap((rule,index)=>match(rule)?[index]:[]);if(indices.length>1)throw Error(`${entry.id}/${template.id}: duplicate compiled rules`);
        if(indices.length)program.rules[indices[0]]=matches[0];else program.rules.push(matches[0]);}
      if(JSON.stringify(pkg)!==before){if(check)throw Error(`${entry.id}: transient output validation is stale`);writeFileSync(file,`${JSON.stringify(pkg,null,2)}\n`);}
      continue;
    }
    for(const id of ids){
      const original=obligations.find(row=>row.id===id);
      const existing=original.when?.all?.filter(row=>!isDeepStrictEqual(row,condition('notMemberOf')))??(original.when?[original.when]:[]);
      original.when={all:[...existing,condition('notMemberOf')]};
      const transient=structuredClone(original);transient.id=`${id}_transient`;transient.name+=' (transient)';
      transient.when={all:[...existing,condition('memberOf')]};
      transient.diagnostic.message=`Transient ${original.diagnostic.message}`;
      const literals=transient.constraint.subject.object.symbols.select.from.array[0].literal;
      const time={allowedScopes:['globalExpression','pointwiseExpression','integrationArgument'],kind:'time',symbol:'t',unit:'s'};
      const index=literals.findIndex(row=>row.symbol==='t');if(index<0)literals.push(time);else literals[index]=time;
      const old=obligations.findIndex(row=>row.id===transient.id);if(old<0)obligations.push(transient);else obligations[old]=transient;
    }
    const compiled=composeScenarioContracts(base,[{registry:entry,contract:pkg}]).schema['x-fullmeta-rules'].rules;
    for(const id of ids)for(const row of [obligations.find(x=>x.id===id),obligations.find(x=>x.id===`${id}_transient`)]){
      const match=rule=>JSON.stringify(rule).includes(JSON.stringify(row.diagnostic.message));
      const matches=compiled.filter(match);if(matches.length!==1)throw Error(`${entry.id}/${row.id}: expected one compiled rule`);
      const indices=program.rules.flatMap((rule,index)=>match(rule)?[index]:[]);
      if(indices.length>1)throw Error(`${entry.id}/${row.id}: duplicate compiled rules`);
      if(indices.length)program.rules[indices[0]]=matches[0];else program.rules.push(matches[0]);
    }
    if(entry.id==='thermal-2d')thermalRows=ids.flatMap(id=>[id,`${id}_transient`].map(key=>structuredClone(obligations.find(row=>row.id===key))));
    if(JSON.stringify(pkg)!==before){if(check)throw Error(`${entry.id}: transient output validation is stale`);writeFileSync(file,`${JSON.stringify(pkg,null,2)}\n`);}
  }
  if(!thermalRows)throw Error('Thermal output validation source is missing');
  const sharedPath=resolve(root,'validation/thermal-common.json'),before=JSON.stringify(shared);
  for(const obligation of thermalRows){const index=shared.rows.findIndex(row=>row.obligation.id===obligation.id);
    if(index<0)shared.rows.push({obligation});else shared.rows[index].obligation=obligation;}
  if(JSON.stringify(shared)!==before){if(check)throw Error('Shared thermal output validation is stale');writeFileSync(sharedPath,`${JSON.stringify(shared,null,2)}\n`);}
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))syncTransientOutputValidation({check:process.argv[2]!=='--write'});
