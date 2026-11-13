import { modelAfterTreeDeletion } from '../../../platform/js/services.js';
import { inspectExpression } from '../../../platform/core-rust/js/api.js';
import { projectModel } from './model-projection.js';
import { duplicateArrayAvailability } from './physics-tree.js';

const at = (value, path) => path.reduce((node, key) => node?.[key], value);
const within = (path, parent) => parent.every((key, index) => path[index] === key);

function referencingName(model, path) {
  for (let length = path.length; length > 0; length--) {
    const value = at(model, path.slice(0, length));
    if (typeof value?.name === 'string' && value.name) return value.name;
  }
  return path.join('/');
}

/** Check the current target and references after schema-owned key cleanup. */
export function prepareArrayDeletion(model, target) {
  const guard = duplicateArrayAvailability(model, target, () => {});
  if (guard.disabled) return { reason: guard.reason };
  const path = target.itemPath, item = at(model, path);
  try {
    const before = projectModel(model), document = modelAfterTreeDeletion(model, path);
    const removed = Object.entries(before.records).flatMap(([collection, records]) =>
      records.filter(record => within(record.path, path)).map(record => ({ ...record, collection })));
    const after = projectModel(document);
    const reference = after.relationships.find(edge => edge.status === 'missing'
      && removed.some(record => record.collection === edge.targetCollection && record.id === edge.reference));
    if (reference) return { reason: `Cannot delete ${item.name || item.id}: ${referencingName(document, reference.sourcePath)} references ${reference.reference} at ${[...reference.sourcePath, reference.field].join('/')}.` };
    if (removed.some(record => record.collection === 'periodicPairs'
      && document.model.mesh?.periodicPairIds?.includes(record.id)))
      return { reason: `Cannot delete ${item.name || item.id}: Mesh enables this periodic pair. Remove it from Periodic pair meshing constraints first.` };
    const aliases = new Set(removed.filter(record => record.collection === 'integrationOperators')
      .flatMap(record => [record.id, record.value.name]).filter(value => typeof value === 'string' && value));
    if (aliases.size) {
      // Integration calls belong to expression definitions. Unknown extension
      // objects must not become executable expressions merely by sharing a key.
      for (const collection of ['variables', 'pointwiseVariables']) {
        for (const definition of document.model[collection] || []) {
          if (typeof definition.expression !== 'string') continue;
          const used = inspectExpression(definition.expression).calls.find(name => aliases.has(name));
          if (used) return { reason: `Cannot delete ${item.name || item.id}: ${definition.name || definition.symbol || definition.id} calls ${used} in ${collection}.` };
        }
      }
    }
    return { document, label: item.name || item.id };
  } catch (error) {
    return { reason: `Cannot check deletion references: ${error.message}` };
  }
}

export function deleteArrayItem(model, target) {
  const prepared = prepareArrayDeletion(model, target);
  if (!prepared.document) throw new Error(prepared.reason);
  return { document: prepared.document, path: null, label: '',
    message: `Removed ${prepared.label}. Validate the model, then Save to keep the change.` };
}
