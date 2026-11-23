// Keep the editor's scenario metadata aligned with the solver's enabled states.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
const registry=JSON.parse(readFileSync(resolve(root,'src/scenarios/registry.json'),'utf8'));
const check=process.argv.includes('--check');
for(const entry of registry.packages){
  const path=resolve(root,'src/scenarios',entry.file),text=readFileSync(path,'utf8'),pkg=JSON.parse(text);
  if(!pkg.transientForms)continue;
  const fields=Object.fromEntries(Object.entries(pkg.transientForms).map(([form,contract])=>[form,
    contract.fields.map(({id,components,unit})=>({id,components,unit}))]));
  pkg.schemaDocument['x-fem-scenario'].transientInitialFieldsByFormulation=fields;
  const updated=JSON.stringify(pkg,null,2)+'\n';
  if(updated!==text.replace(/\r\n/g,'\n')){if(check)throw Error(`${entry.id}: transient initial-field metadata is stale`);writeFileSync(path,updated);}
}
