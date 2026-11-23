// Guard wildcard component checks so a malformed non-array is reported by the
// generated vector-type rule instead of aborting semantic validation.
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {isDeepStrictEqual} from 'node:util';
import {composeScenarioContracts} from '../../js/scenario-discovery.js';

const root=resolve(fileURLToPath(new URL('..',import.meta.url)));
export function syncVectorComponentGuards({check=true}={}) {
  const registry=JSON.parse(readFileSync(resolve(root,'src/scenarios/registry.json'),'utf8'));
  const base=JSON.parse(readFileSync(resolve(root,'../fem.schema.json'),'utf8'));
  const guard={type:'countAtMost',subject:{field:'/fieldValues'},other:{literal:Number.MAX_SAFE_INTEGER}};
  for(const entry of registry.packages) {
    const file=resolve(root,'src/scenarios',entry.file),pkg=JSON.parse(readFileSync(file,'utf8'));
    const program=pkg.schemaContributions?.['x-fullmeta-rules'];
    const targets=(program?.declarative?.obligations||[]).filter(row=>row.scope?.nested?.some(scope=>scope.path==='/fieldValues/*'));
    if(!targets.length)continue;
    let dirty=false;
    for(const row of targets) {
      const conditions=row.scope?.when?.all;
      if(!Array.isArray(conditions))throw Error(`${entry.id}/${row.id}: unsupported component rule guard`);
      if(!conditions.some(value=>isDeepStrictEqual(value,guard))) {conditions.push(structuredClone(guard));dirty=true;}
    }
    const {schema}=composeScenarioContracts(base,[{registry:entry,contract:pkg}]);
    for(const row of targets) {
      const match=rule=>JSON.stringify(rule).includes(JSON.stringify(row.diagnostic?.message));
      const generated=schema['x-fullmeta-rules'].rules.filter(match),indices=program.rules.map((value,i)=>match(value)?i:-1).filter(i=>i>=0);
      if(generated.length!==1||indices.length!==1)throw Error(`${entry.id}/${row.id}: component rule must have one executable`);
      if(!isDeepStrictEqual(program.rules[indices[0]],generated[0])) {program.rules[indices[0]]=generated[0];dirty=true;}
    }
    if(dirty&&check)throw Error(`${entry.id}: vector component guards are stale; run node prebuilt-tools/sync-vector-component-guards.mjs --write`);
    if(dirty)writeFileSync(file,`${JSON.stringify(pkg,null,2)}\n`);
  }
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  if(process.argv.length>3||(process.argv[2]&&!['--check','--write'].includes(process.argv[2])))throw Error('Use --check or --write.');
  syncVectorComponentGuards({check:process.argv[2]!=='--write'});
}
