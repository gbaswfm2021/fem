import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { composeScenarioContracts } from '../../js/scenario-discovery.js';
import { guardPhysicsRoot, preparePhysicsCondition } from '../../js/physics-tree.js';

const read = path => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'));
const root = read('../../fem.schema.json');
const registry = read('../src/scenarios/registry.json');
const packages = registry.packages.map(item => ({
  registry: item,
  contract: read(`../src/scenarios/${item.file}`),
}));

test('only declared boundary field layouts enable component controls', () => {
  const { schema } = composeScenarioContracts(root, packages);
  for (const profile of registry.packages.map(item => item.id)) {
    const branches = schema.$defs.femProfiles[profile].then.properties.model.properties.physics.additionalProperties.oneOf;
    for (const type of ['dirichlet', 'neumann']) {
      const branch = branches.find(item => item.properties.type.const === type);
      assert.ok(branch, `${profile} lacks ${type}`);
      assert.equal(Boolean(branch.properties.fieldValues), profile === 'em-3d' || (['em-2d', 'acoustic-2d', 'acoustic-3d', 'thermal-2d'].includes(profile) && type === 'neumann'));
      assert.equal(branch.properties.value.type, 'object');
    }
    const impedance = branches.find(item => item.properties.type.const === 'impedance');
    if (impedance) assert.equal(impedance.properties.polarization.type, 'array');
  }
});

test('thermal2D Neumann fields have two heat-flux components', () => {
  const physics = packages.find(item => item.registry.id === 'thermal-2d').contract.schemaDocument['x-fem-physics'];
  const {schema} = composeScenarioContracts(root, packages);
  const branches = schema.$defs.femProfiles['thermal-2d'].then.properties.model.properties.physics.additionalProperties.oneOf;
  const field = branches.find(item => item.properties.type.const === 'neumann').properties.fieldValues;
  assert.equal(field.minItems, 2);
  assert.equal(field.maxItems, 2);
  assert.deepEqual(field['x-fullmeta-ui-behavior'].payload.widget.settings.componentLabels, ['qx','qy']);
  const model = {scenario:{profileId:'thermal-2d'},model:{physics:{id:'physics',formulation:'T'},selections:[{id:'wall',entity:'boundary'}],mesh:{periodicPairs:[]}}};
  const target = {itemPath:['model','physics']};
  target.femPhysicsGuard = guardPhysicsRoot(model, target);
  const spec = physics.conditionActions.find(action => action.conditionType === 'neumann');
  const {condition, reason} = preparePhysicsCondition({model,target,spec,profile:physics,makeId:()=> 'bc'});
  assert.ok(condition, reason);
  assert.deepEqual(condition.value, {expression:'0',unit:'W/m^2'});
  assert.deepEqual(condition.fieldValues, Array.from({length:2},()=>({expression:'0',unit:'W/m^2'})));
});

test('acoustic2D Neumann fields have two velocity components', () => {
  const physics = packages.find(item => item.registry.id === 'acoustic-2d').contract.schemaDocument['x-fem-physics'];
  const {schema} = composeScenarioContracts(root, packages);
  const branches = schema.$defs.femProfiles['acoustic-2d'].then.properties.model.properties.physics.additionalProperties.oneOf;
  const field = branches.find(item => item.properties.type.const === 'neumann').properties.fieldValues;
  assert.equal(field.minItems, 2);
  assert.equal(field.maxItems, 2);
  assert.deepEqual(field['x-fullmeta-ui-behavior'].payload.widget.settings.componentLabels, ['vx','vy']);
  const model = {scenario:{profileId:'acoustic-2d'},model:{physics:{id:'physics',formulation:'pressure'},selections:[{id:'wall',entity:'boundary'}],mesh:{periodicPairs:[]}}};
  const target = {itemPath:['model','physics']};
  target.femPhysicsGuard = guardPhysicsRoot(model, target);
  const spec = physics.conditionActions.find(action => action.conditionType === 'neumann');
  const {condition, reason} = preparePhysicsCondition({model,target,spec,profile:physics,makeId:()=> 'bc'});
  assert.ok(condition, reason);
  assert.deepEqual(condition.value, {expression:'0',unit:'m/s'});
  assert.deepEqual(condition.fieldValues, Array.from({length:2},()=>({expression:'0',unit:'m/s'})));
});

test('acoustic3D Neumann fields have three velocity components', () => {
  const physics = packages.find(item => item.registry.id === 'acoustic-3d').contract.schemaDocument['x-fem-physics'];
  const {schema} = composeScenarioContracts(root, packages);
  const branches = schema.$defs.femProfiles['acoustic-3d'].then.properties.model.properties.physics.additionalProperties.oneOf;
  const field = branches.find(item => item.properties.type.const === 'neumann').properties.fieldValues;
  assert.equal(field.minItems, 3);
  assert.equal(field.maxItems, 3);
  assert.deepEqual(field['x-fullmeta-ui-behavior'].payload.widget.settings.componentLabels, ['vx','vy','vz']);
  const model = {scenario:{profileId:'acoustic-3d'},model:{physics:{id:'physics',formulation:'pressure'},selections:[{id:'wall',entity:'boundary'}],mesh:{periodicPairs:[]}}};
  const target = {itemPath:['model','physics']};
  target.femPhysicsGuard = guardPhysicsRoot(model, target);
  const spec = physics.conditionActions.find(action => action.conditionType === 'neumann');
  const {condition, reason} = preparePhysicsCondition({model,target,spec,profile:physics,makeId:()=> 'bc'});
  assert.ok(condition, reason);
  assert.deepEqual(condition.value, {expression:'0',unit:'m/s'});
  assert.deepEqual(condition.fieldValues, Array.from({length:3},()=>({expression:'0',unit:'m/s'})));
});

test('EM2D Neumann fields follow the selected polarization', () => {
  const physics = packages.find(item => item.registry.id === 'em-2d').contract.schemaDocument['x-fem-physics'];
  const {schema} = composeScenarioContracts(root, packages);
  const branches = schema.$defs.femProfiles['em-2d'].then.properties.model.properties.physics.additionalProperties.oneOf;
  const field = branches.find(item => item.properties.type.const === 'neumann').properties.fieldValues;
  assert.equal(field.minItems, 2);
  assert.equal(field.maxItems, 2);
  for (const [formulation, scalarUnit, fieldUnit] of [['Ez', 'V/m^2', 'A/m'], ['Hz', 'A/m^2', 'V/m']]) {
    const model = {scenario:{profileId:'em-2d'},model:{physics:{id:'physics',formulation},selections:[{id:'wall',entity:'boundary'}],mesh:{periodicPairs:[]}}};
    const target = {itemPath:['model','physics']};
    target.femPhysicsGuard = guardPhysicsRoot(model, target);
    const spec = physics.conditionActions.find(action => action.conditionType === 'neumann');
    const {condition, reason} = preparePhysicsCondition({model,target,spec,profile:physics,makeId:()=> 'bc'});
    assert.ok(condition, reason);
    assert.deepEqual(condition.value, {expression:'0',unit:scalarUnit});
    assert.deepEqual(condition.fieldValues, Array.from({length:2},()=>({expression:'0',unit:fieldUnit})));
  }
});

test('new EM3D conditions contain three quantities with the correct units', () => {
  const physics = packages.find(item => item.registry.id === 'em-3d').contract.schemaDocument['x-fem-physics'];
  for (const [type, scalarUnit, vectorUnit] of [['dirichlet', 'V/m', 'V/m'], ['neumann', 'V/m^2', 'A/m']]) {
    const model = { scenario: { profileId: 'em-3d' }, model: {
      physics: { id: 'physics', formulation: 'electric' },
      selections: [{ id: 'wall', entity: 'boundary' }], mesh: { periodicPairs: [] },
    } };
    const target = { itemPath: ['model', 'physics'] };
    target.femPhysicsGuard = guardPhysicsRoot(model, target);
    const spec = physics.conditionActions.find(action => action.conditionType === type);
    const { condition, reason } = preparePhysicsCondition({ model, target, spec, profile: physics, makeId: () => 'bc' });
    assert.ok(condition, reason);
    assert.deepEqual(condition.fieldValues, Array.from({ length: 3 }, () => ({ expression: '0', unit: vectorUnit })));
    assert.deepEqual(condition.value, { expression: '0', unit: scalarUnit });
  }
});

test('shared field layout supports a tensor-shaped component list', () => {
  const candidate = structuredClone(packages);
  const physics = candidate.find(item => item.registry.id === 'em-3d').contract.schemaDocument['x-fem-physics'];
  physics.boundaryFieldLayouts.neumann = {
    role: 'derived', shape: [3, 3], zeroScalarOnly: true,
    componentLabels: ['xx', 'xy', 'xz', 'yx', 'yy', 'yz', 'zx', 'zy', 'zz'],
  };
  physics.boundaryDefaultsByFormulation.electric.neumann.fieldValues = Array.from({ length: 9 }, () => ({ expression: '0', unit: 'A/m' }));
  const { schema } = composeScenarioContracts(root, candidate);
  const branches = schema.$defs.femProfiles['em-3d'].then.properties.model.properties.physics.additionalProperties.oneOf;
  const field = branches.find(item => item.properties.type.const === 'neumann').properties.fieldValues;
  assert.equal(field.minItems, 9);
  assert.equal(field.maxItems, 9);
  assert.deepEqual(field['x-fullmeta-ui-behavior'].payload.widget.settings.componentLabels, physics.boundaryFieldLayouts.neumann.componentLabels);
});
