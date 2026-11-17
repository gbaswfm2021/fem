// Run after building FEM: node --test applications/fem/fem-rust/tests/geometry_3d_gmsh.test.cjs
// FULLMETA_FEM_EXE can select a candidate executable before it is staged.
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');

const crate = path.resolve(__dirname, '..');
const app = path.resolve(crate, '..');
const workspace = path.resolve(app, '../..');
const executable = process.env.FULLMETA_FEM_EXE || path.join(app, '.runtime/x86_64-pc-windows-gnu/release/fullmeta-fem.exe');
const gmsh = path.join(crate, 'vendor/gmsh-4.15.2/gmsh-4.15.2-Windows64/gmsh.exe');
const env = {...process.env, FULLMETA_ROOT: workspace, FULLMETA_MODEL_SCHEMA_FILE: path.join(app, 'fem.schema.json')};
for (const key of ['FULLMETA_CORE_DLL', 'FULLMETA_MODEL_DLL', 'FULLMETA_CONTRACTS_DLL']) delete env[key];

const quantity = n => ({expression: String(n), unit: 'm'});
const vector = (...values) => values.map(quantity);
const box = (id, position, size) => ({id, name:id, type:'box', position:vector(...position), size:vector(...size)});
const rectangle = (id, position, size) => ({id, name:id, type:'rectangle', position:vector(...position), size:vector(...size)});
const selection = (id, entity, membership) => ({id, name:id, entity, addresses:[{membership}]});
const material = selectionId => ({id:'material', name:'Material', selectionId});
function model(profile, dimension, features, selections, active, materials) {
  return {modelFormat:'fem-model/2', metadata:{id:'geometry-regression', name:'Geometry regression'}, scenario:{profileId:profile, dimension},
    model:{parameters:[], geometry:{tolerance:quantity(1e-7), features}, selections, materials, physics:{domainSelectionIds:active},
      mesh:{elementFamily:dimension===3?'tetrahedron':'triangle', periodicPairs:[], meshSizeControls:[], minSize:quantity(0.3), maxSize:quantity(0.7),
        gmsh:{algorithm3D:1, elementOrder:1, optimize:false, optimizeNetgen:false, optimizeThreshold:0.3, smoothing:0, threads:1}}}};
}

function run(file, args) {
  const result = spawnSync(file, args, {cwd:workspace, env, encoding:'utf8', timeout:120000, windowsHide:true});
  assert.equal(result.error, undefined, result.error?.message);
  assert.equal(result.status, 0, `${file} ${args.join(' ')}\n${result.stderr}\n${result.stdout}`);
  return result.stdout;
}
function compile(directory, value, program=executable) {
  fs.mkdirSync(directory, {recursive:true});
  const input = path.join(directory, 'model.json');
  fs.writeFileSync(input, JSON.stringify(value));
  const result = JSON.parse(run(program, ['compile', input, '--output-dir', directory, '--overwrite']));
  return {...result, geo:fs.readFileSync(result.geoPath, 'utf8'), directory};
}
function mesh(result, dimension=3) {
  const file = path.join(result.directory, 'output.msh');
  run(gmsh, [result.geoPath, `-${dimension}`, '-format', 'msh2', '-o', file, '-v', '2']);
  const lines = fs.readFileSync(file, 'utf8').split(/\r?\n/), nodes = new Map(), elements = [];
  let i = lines.indexOf('$Nodes'); assert.ok(i>=0, 'mesh has nodes');
  let count = Number(lines[++i]);
  for (let n=0; n<count; n++) { const row=lines[++i].trim().split(/\s+/).map(Number); nodes.set(row[0], row.slice(1,4)); }
  i=lines.indexOf('$Elements'); assert.ok(i>=0, 'mesh has elements'); count=Number(lines[++i]);
  for (let n=0; n<count; n++) {
    const row=lines[++i].trim().split(/\s+/).map(Number), type=row[1], tags=row[2];
    if (type!==2 && type!==4) continue;
    const ids=row.slice(3+tags);
    elements.push({type, tag:row[3], center:[0,1,2].map(axis=>ids.reduce((sum,id)=>sum+nodes.get(id)[axis],0)/ids.length)});
  }
  return elements;
}
function fixture(t) {
  const dir=fs.mkdtempSync(path.join(os.tmpdir(), 'fullmeta-geometry-'));
  t.after(()=>fs.rmSync(dir, {recursive:true, force:true}));
  return dir;
}
function assertSelected(elements, maxX) {
  const volumes=elements.filter(item=>item.type===4);
  assert.ok(volumes.length, 'expected tetrahedra');
  assert.ok(volumes.every(item=>item.tag===1 && item.center[0]<maxX), 'unselected volume or unassigned material was meshed');
}
function assertHollow(elements) {
  const volumes=elements.filter(item=>item.type===4), faces=elements.filter(item=>item.type===2 && item.tag===1);
  assert.ok(volumes.length, 'expected tetrahedra');
  assert.ok(volumes.every(item=>item.tag===1 && !item.center.every(v=>v>1+1e-5 && v<3-1e-5)), 'the hollow core was meshed');
  assert.ok(faces.some(item=>item.center.every(v=>v>=1-1e-5 && v<=3+1e-5)), 'the inner-wall physical selection was lost');
}
const outer=box('outer',[0,0,0],[4,4,4]), inner=box('inner',[1,1,1],[2,2,2]);
const first=box('first',[0,0,0],[1,1,1]), second=box('second',[2,0,0],[1,1,1]);

for (const profile of ['acoustic-3d','em-3d']) test(`${profile}: preserve inner wall and omit inactive core`, t=>{
  const domain=selection('shell','domain',{outer:'inside',inner:'outside'}), wall=selection('wall','boundary',{outer:'inside',inner:'boundary'});
  assertHollow(mesh(compile(fixture(t),model(profile,3,[outer,inner],[domain,wall],['shell'],[material('shell')]))));
});

test('single union: mesh only the selected child', t=>{
  const group={id:'assembly',name:'Assembly',type:'union',features:[first,second]};
  const domain=selection('part','domain',{assembly:'inside',first:'inside',second:'outside'});
  assertSelected(mesh(compile(fixture(t),model('em-3d',3,[group],[domain],['part'],[material('part')]))),1.5);
});

test('single union: classify a hollow nested feature by CAD overlap', t=>{
  const group={id:'assembly',name:'Assembly',type:'union',features:[outer,inner]};
  const domain=selection('shell','domain',{assembly:'inside',outer:'inside',inner:'outside'});
  const wall=selection('wall','boundary',{assembly:'inside',outer:'inside',inner:'boundary'});
  assertHollow(mesh(compile(fixture(t),model('em-3d',3,[group],[domain,wall],['shell'],[material('shell')]))));
});

test('nested unions: retain membership through multiple Boolean levels', t=>{
  const third=box('third',[4,0,0],[1,1,1]);
  const subgroup={id:'subgroup',name:'Subgroup',type:'union',features:[first,second]};
  const group={id:'assembly',name:'Assembly',type:'union',features:[subgroup,third]};
  const domain=selection('part','domain',{assembly:'inside',subgroup:'inside',first:'inside',second:'outside',third:'outside'});
  assertSelected(mesh(compile(fixture(t),model('em-3d',3,[group],[domain],['part'],[material('part')]))),1.5);
});

test('single intersection: preserve the selected overlap', t=>{
  const overlap=box('overlap',[0.5,0,0],[1,1,1]);
  const group={id:'assembly',name:'Assembly',type:'intersection',features:[first,overlap]};
  const domain=selection('part','domain',{assembly:'inside',first:'inside',overlap:'inside'});
  const volumes=mesh(compile(fixture(t),model('em-3d',3,[group],[domain],['part'],[material('part')]))).filter(item=>item.type===4);
  assert.ok(volumes.length, 'expected tetrahedra in the intersection');
  assert.ok(volumes.every(item=>item.tag===1 && item.center[0]>0.5 && item.center[0]<1));
});

test('multiple features: remove unselected volumes', t=>{
  const domain=selection('part','domain',{first:'inside',second:'outside'});
  assertSelected(mesh(compile(fixture(t),model('em-3d',3,[first,second],[domain],['part'],[material('part')]))),1.5);
});

test('empty Physics preserves the complete geometry', t=>{
  const result=compile(fixture(t),model('em-3d',3,[first,second],[],[],[]));
  assert.ok(!result.geo.includes('inactiveVolumes[]'));
  const volumes=mesh(result).filter(item=>item.type===4);
  assert.ok(volumes.some(item=>item.center[0]<1.5) && volumes.some(item=>item.center[0]>1.5));
});

test('single primitive keeps the original meshing path', t=>{
  const domain=selection('part','domain',{first:'inside'});
  const dir=fixture(t), value=model('em-3d',3,[first],[domain],['part'],[material('part')]);
  const result=compile(path.join(dir,'candidate'),value);
  if (process.env.FULLMETA_BASELINE_EXE) assert.equal(result.geo,compile(path.join(dir,'baseline'),value,process.env.FULLMETA_BASELINE_EXE).geo);
  assert.ok(!result.geo.includes('inactiveVolumes[]'));
  assertSelected(mesh(result),1.5);
});

for (const profile of ['acoustic-2d','em-2d','thermal-2d']) test(`${profile}: 2D geometry still meshes`, t=>{
  const domain=selection('part','domain',{plate:'inside'});
  const dir=fixture(t), value=model(profile,2,[rectangle('plate',[0,0],[1,1])],[domain],['part'],[material('part')]);
  const result=compile(path.join(dir,'candidate'),value);
  if (process.env.FULLMETA_BASELINE_EXE) assert.equal(result.geo,compile(path.join(dir,'baseline'),value,process.env.FULLMETA_BASELINE_EXE).geo);
  assert.ok(mesh(result,2).some(item=>item.type===2 && item.tag===1), 'expected material triangles');
});

test('changing the Physics domain changes the mesh cache key', t=>{
  const a=selection('a','domain',{first:'inside',second:'outside'}), b=selection('b','domain',{first:'outside',second:'inside'});
  const original=model('em-3d',3,[first,second],[a,b],['a'],[material('a')]), changed=structuredClone(original);
  changed.model.physics.domainSelectionIds=['b'];
  const dir=fixture(t), left=compile(path.join(dir,'left'),original), right=compile(path.join(dir,'right'),changed);
  assert.notEqual(left.geometryHash,right.geometryHash);
});
