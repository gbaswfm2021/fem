import test from 'node:test';
import assert from 'node:assert/strict';
import { structure } from './edge_benchmark_structure.mjs';
const mesh='T,0,0,1,2,3,1\nF,x,0,1,2,8';
const map={boundarySelections:[{id:'fixed',physicalTag:8}]};
const request=order=>({elementOrder:order,modelConfiguration:{periodicPairs:[],prescribedBoundaries:[{type:'dirichlet',selectionId:'fixed'}]}});
test('tetrahedral structural entries account for constrained edge and face moments',()=>{
  assert.deepEqual(structure(request(1),mesh,map),{dofs:6,essentialDofs:3,storedEntries:12});
  assert.deepEqual(structure(request(2),mesh,map),{dofs:20,essentialDofs:8,storedEntries:152});
});
test('periodic topology is explicitly outside the structural estimate',()=>{
  const value=request(1); value.modelConfiguration.periodicPairs=[{}];
  assert.throws(()=>structure(value,mesh,map),/non-periodic/);
});
