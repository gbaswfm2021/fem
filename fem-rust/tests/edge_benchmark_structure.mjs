// Reconstruct the structurally stored sparse matrix entries for the benchmark's
// non-periodic Nedelec mesh. This counts stored entries, including values that
// may numerically be zero; it does not instrument or alter the solver.
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const edges=[[0,1],[0,2],[0,3],[1,2],[1,3],[2,3]];
const faces=[[0,1,2],[0,1,3],[0,2,3],[1,2,3]];
const key=nodes=>nodes.slice().sort((a,b)=>a-b).join(',');
export function structure(request,mesh,map) {
  const order=request.elementOrder;
  if(![1,2].includes(order)) throw new Error('Only Nedelec orders 1 and 2 are supported.');
  if((request.modelConfiguration.periodicPairs ?? []).length) throw new Error('Structural benchmark estimate requires a non-periodic mesh.');
  const tags=new Map((map.boundarySelections ?? []).map(row=>[row.id ?? row.selectionId,row.physicalTag]));
  const fixedTags=new Set((request.modelConfiguration.prescribedBoundaries ?? []).filter(row=>row.type==='dirichlet').map(row=>{
    const tag=tags.get(row.selectionId); if(!(Number.isSafeInteger(tag)&&tag>0)) throw new Error('Essential selection has no positive mesh tag.'); return tag;
  }));
  const cells=new Map(), selected=[];
  for(const raw of mesh.split(/\r?\n/)) {
    const row=raw.trim().split(',');
    if(row[0]==='T' && row.length>=7) cells.set(Number(row[1]),row.slice(2,6).map(Number));
    if(row[0]==='F' && row.length>=6 && fixedTags.has(Number(row[5]))) selected.push(row.slice(2,5).map(Number));
  }
  if(!cells.size || [...cells.keys()].sort((a,b)=>a-b).some((id,index)=>id!==index)) throw new Error('Cell IDs must be contiguous.');
  const edgeDofs=new Map(), faceDofs=new Map(), adjacency=[];
  const allocate=(table,k,n)=>{
    if(!table.has(k)) { const ids=Array.from({length:n},()=>{adjacency.push(new Set());return adjacency.length-1;}); table.set(k,ids); }
    return table.get(k);
  };
  for(const cell of [...cells.entries()].sort((a,b)=>a[0]-b[0]).map(([,v])=>v)) {
    if(cell.some(v=>!Number.isSafeInteger(v)||v<0) || new Set(cell).size!==4) throw new Error('Cell connectivity is invalid.');
    const local=edges.flatMap(e=>allocate(edgeDofs,key(e.map(i=>cell[i])),order));
    if(order===2) for(const face of faces) local.push(...allocate(faceDofs,key(face.map(i=>cell[i])),2));
    for(const i of local) for(const j of local) adjacency[i].add(j);
  }
  const fixed=new Set();
  for(const face of selected) {
    if(face.some(v=>!Number.isSafeInteger(v)||v<0) || new Set(face).size!==3) throw new Error('Selected face is invalid.');
    for(const local of [[0,1],[0,2],[1,2]]) {
      const dofs=edgeDofs.get(key(local.map(i=>face[i]))); if(!dofs) throw new Error('Selected edge is absent from the mesh.');
      for(const dof of dofs) fixed.add(dof);
    }
    if(order===2) {const dofs=faceDofs.get(key(face)); if(!dofs) throw new Error('Selected face is absent from the mesh.'); for(const dof of dofs) fixed.add(dof);}
  }
  let entries=0;
  for(let i=0;i<adjacency.length;i++) entries+=fixed.has(i)?1:[...adjacency[i]].filter(j=>!fixed.has(j)).length;
  return {dofs:adjacency.length,essentialDofs:fixed.size,storedEntries:entries};
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const requestPath=resolve(process.argv[2]);
  const request=JSON.parse(readFileSync(requestPath,'utf8'));
  const resolveInput=value=>resolve(dirname(requestPath),value);
  const mesh=readFileSync(resolveInput(request.meshFile),'utf8');
  const map=JSON.parse(readFileSync(resolveInput(request.meshMap),'utf8'));
  process.stdout.write(JSON.stringify(structure(request,mesh,map))+'\n');
}
