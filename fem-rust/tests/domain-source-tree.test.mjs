import test from 'node:test';
import assert from 'node:assert/strict';
import { addPhysicsCondition, deletePhysicsCondition, guardPhysicsCondition, guardPhysicsRoot, togglePhysicsCondition } from '../../js/physics-tree.js';
import { readFileSync } from 'node:fs';
import { compileDeclarativeValidation } from '../../../../platform/ui/widgets/validation-program/declarative-rules.js';
import { validateModelRules } from '../../../../platform/model-rust/js/api.js';

test('domain sources use domain selections and may overlap', () => {
  const model = { scenario: { profileId: 'example' }, model: {
    physics: { id: 'physics', formulation: 'field', source1: { id: 'source1', name: 'Source', type: 'domainSource', selectionId: 'domain' } },
    selections: [{ id: 'domain', entity: 'domain' }, { id: 'wall', entity: 'boundary' }], mesh: {},
  } };
  const profile = { supportedBoundaryTypes: ['dirichlet'], supportedFeatureTypes: ['domainSource'],
    featureHandlingByType: { domainSource: 'domainSource' },
    boundaryDefaultsByFormulation: { field: { domainSource: { values: [{ expression: '0', unit: '1' }] }, dirichlet: { value: 0 } } } };
  const target = { itemPath: ['model', 'physics'] };
  target.femPhysicsGuard = guardPhysicsRoot(model, target);
  const add = (type, id) => addPhysicsCondition({ model, target, profile, makeId: () => id,
    spec: { conditionType: type, itemName: type } });
  assert.equal(add('domainSource', 'source2').document.model.physics.source2.selectionId, 'domain');
  assert.equal(add('dirichlet', 'bc').document.model.physics.bc.selectionId, 'wall');
});

test('domain-source validation rejects a boundary or missing selection', () => {
  const full = JSON.parse(readFileSync(new URL('../../fem.schema.json', import.meta.url), 'utf8'));
  const obligation = full['x-fullmeta-rules'].declarative.obligations.find(row => row.id === 'domain_source_selection_exists');
  assert.ok(obligation);
  const schema = { 'x-fullmeta-rules': { version: 'fullmeta-rules/1', maximumErrors: 50,
    rules: compileDeclarativeValidation([obligation], { programKey: 'x-fullmeta-rules' }) } };
  const model = { model: { selections: [{ id: 'd', entity: 'domain' }, { id: 'b', entity: 'boundary' }],
    physics: { drive: { id: 'drive', type: 'domainSource', selectionId: 'd' } } } };
  const errors = () => validateModelRules(model, {}, schema);
  assert.deepEqual(errors(), []);
  model.model.physics.drive.selectionId = 'b';
  assert.ok(errors().some(error => String(error).includes('existing domain selection')));
  model.model.physics.drive.selectionId = 'missing';
  assert.ok(errors().some(error => String(error).includes('existing domain selection')));
  model.model.physics.drive.enabled = false;
  assert.deepEqual(errors(), []);
});

test('all declared formulations support repeatable source lifecycle and guarded deletion', () => {
  const registry = JSON.parse(readFileSync(new URL('../src/scenarios/registry.json', import.meta.url), 'utf8'));
  for (const entry of registry.packages) {
    const contract = JSON.parse(readFileSync(new URL(`../src/scenarios/${entry.file}`, import.meta.url), 'utf8'));
    const profile = contract.schemaDocument['x-fem-physics'];
    const profileId = entry.id;
    for (const [formulation, source] of Object.entries(contract.domainSources)
      .filter(([name]) => Object.hasOwn(profile.boundaryDefaultsByFormulation || {}, name))) {
      let model = { metadata:{id:'fixture'}, scenario:{profileId}, model:{
        physics:{id:'physics',formulation,domainSelectionIds:['domain']},
        selections:[{id:'domain',entity:'domain'}],mesh:{} } };
      for (const id of ['sourceA','sourceB']) {
        const target={itemPath:['model','physics']};
        target.femPhysicsGuard=guardPhysicsRoot(model,target);
        model=addPhysicsCondition({model,target,profile,makeId:()=>id,
          spec:{conditionType:'domainSource',itemName:'Domain Source'}}).document;
      }
      assert.equal(model.model.physics.sourceA.selectionId,'domain',`${profileId}/${formulation}`);
      assert.equal(model.model.physics.sourceB.values.length,source.routes.domainSource.input.components,`${profileId}/${formulation}`);
      const target={itemPath:['model','physics','sourceB']};
      target.femConditionGuard=guardPhysicsCondition(model,target,profile);
      model=togglePhysicsCondition(model,target,profile).document;
      assert.equal(model.model.physics.sourceB.enabled,false);
      model=JSON.parse(JSON.stringify(model)); // persisted and reopened representation
      assert.equal(model.model.physics.sourceB.enabled,false);
      target.femConditionGuard=guardPhysicsCondition(model,target,profile);
      model=togglePhysicsCondition(model,target,profile).document;
      assert.equal(model.model.physics.sourceB.enabled,true);
      target.femConditionGuard=guardPhysicsCondition(model,target,profile);
      const validator=value=>value.model.physics.sourceB?[]:['referenced source is missing'];
      assert.throws(()=>deletePhysicsCondition({model,target,profile,validate:validator,maximumErrors:50}),/referenced source is missing/);
      model=deletePhysicsCondition({model,target,profile,validate:()=>[],maximumErrors:50}).document;
      assert.ok(model.model.physics.sourceA);
      assert.equal(model.model.physics.sourceB,undefined);
    }
  }
});
