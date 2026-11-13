import { modelWithItemMoveDraft } from '../../../platform/js/document/edit-transactions.js';

const root = ['model', 'geometry', 'features'];
const equal = (a, b) => a.length === b.length && a.every((part, index) => part === b[index]);
const starts = (path, prefix) => path.length >= prefix.length && prefix.every((part, index) => path[index] === part);
const at = (value, path) => path.reduce((node, key) => node?.[key], value);
const group = item => item?.type === 'union' || item?.type === 'intersection';

function geometryArrayPath(model, path) {
  if (!Array.isArray(path) || !starts(path, root) || path.length <= root.length) return null;
  let array = at(model, root), cursor = root.length;
  while (cursor < path.length) {
    const index = path[cursor];
    if (!Array.isArray(array) || !Number.isSafeInteger(index) || index < 0 || index >= array.length) return null;
    if (cursor === path.length - 1) return path.slice(0, -1);
    if (path[cursor + 1] !== 'features' || !group(array[index])) return null;
    array = array[index].features;
    cursor += 2;
  }
  return null;
}

function destinationAccepts(model, destinationPath, feature) {
  if (!Array.isArray(destinationPath) || !Array.isArray(at(model, destinationPath))) return false;
  if (equal(destinationPath, root)) return true;
  if (destinationPath.at(-1) !== 'features') return false;
  const parentPath = destinationPath.slice(0, -1);
  const parent = at(model, parentPath);
  if (!geometryArrayPath(model, parentPath) || !group(parent)) return false;
  const dimension = model?.scenario?.dimension;
  return ['union', 'intersection', ...(dimension === 2 ? ['rectangle', 'circle']
    : dimension === 3 ? ['box', 'cylinder'] : [])].includes(feature?.type);
}

export function geometryInsertionIssue(model, insertion) {
  if (!Array.isArray(insertion?.arrayPath) || !starts(insertion.arrayPath, root)) return '';
  return destinationAccepts(model, insertion.arrayPath, insertion.item) ? ''
    : 'A Union or Intersection can contain only domain geometry for the current dimension.';
}

export function arrayPathForItem(model, path) {
  const geometry = geometryArrayPath(model, path);
  if (geometry) return geometry;
  if (!Array.isArray(path) || !Number.isSafeInteger(path.at(-1))) return null;
  const parent = path.slice(0, -1);
  const eligible = [ ['model', 'selections'], ['model', 'integrationOperators'], ['model', 'materials'],
    ['model', 'results'], ['model', 'mesh', 'meshSizeControls'], ['model', 'mesh', 'periodicPairs'] ];
  const array = at(model, parent);
  return eligible.some(candidate => equal(parent, candidate)) && Array.isArray(array)
    && path.at(-1) >= 0 && path.at(-1) < array.length ? parent : null;
}

export const isSpecialItemPath = (path, model) => Boolean(geometryArrayPath(model, path));
export const specialItemDragTitle = label => `${label} · Drag to reorder or move between groups; drop onto Union or Intersection to add a child`;

export function dropPlan({ model, sourcePath, targetPath, targetNode, clientY }) {
  const sourceArray = geometryArrayPath(model, sourcePath), targetArray = geometryArrayPath(model, targetPath);
  if (!sourceArray || !targetArray || equal(sourcePath, targetPath) || starts(targetPath, sourcePath)) return null;
  const bounds = targetNode?.getBoundingClientRect?.();
  if (!bounds || !Number.isFinite(clientY) || !Number.isFinite(bounds.top) || !(bounds.height > 0)) return null;
  const ratio = (clientY - bounds.top) / bounds.height;
  if (ratio < 0 || ratio > 1) return null;
  const target = at(model, targetPath), feature = at(model, sourcePath);
  if (group(target) && ratio >= 0.25 && ratio <= 0.75) {
    const destinationPath = [...targetPath, 'features'];
    if (!destinationAccepts(model, destinationPath, feature)) return null;
    return { kind: 'move', destinationPath, index: at(model, destinationPath).length, nested: true };
  }
  const dropAfter = ratio > 0.5;
  if (equal(sourceArray, targetArray)) return { kind: 'reorder', dropAfter };
  if (!destinationAccepts(model, targetArray, feature) || starts(targetArray, sourcePath)) return null;
  return { kind: 'move', destinationPath: targetArray, index: targetPath.at(-1) + Number(dropAfter), nested: false, dropAfter };
}

export function moveItem(model, sourcePath, destinationPath, index) {
  const sourceArray = geometryArrayPath(model, sourcePath), feature = at(model, sourcePath);
  const destination = Array.isArray(destinationPath) && at(model, destinationPath);
  if (!sourceArray || !destinationAccepts(model, destinationPath, feature) || starts(destinationPath, sourcePath)
    || !Number.isSafeInteger(index) || index < 0 || index > destination.length) return null;
  const moved = modelWithItemMoveDraft(model, sourcePath, destinationPath, index);
  const parent = equal(destinationPath, root) ? null : at(model, destinationPath.slice(0, -1));
  const changedAddressability = equal(sourceArray, root) !== equal(destinationPath, root);
  const destinationLabel = parent?.name || parent?.id || 'Geometry features';
  const featureLabel = feature.name || feature.id || feature.type;
  return { model: moved, featureLabel, destinationLabel, destinationIsGroup: Boolean(parent),
    message: `Moved “${featureLabel}” ${parent ? 'under' : 'to'} ${destinationLabel}. ${changedAddressability
      ? 'Review selections and validate the group before saving.' : 'Save to keep the change.'}` };
}

export function affectsPreviewForReorder(_model, arrayPath) {
  return Array.isArray(arrayPath) && (starts(arrayPath, root)
    || [ ['model', 'selections'], ['model', 'mesh', 'meshSizeControls'], ['model', 'mesh', 'periodicPairs'] ]
      .some(candidate => equal(arrayPath, candidate)));
}

export const geometryTreeBehavior = { arrayPathForItem, isSpecialItemPath, specialItemDragTitle,
  dropPlan, moveItem, affectsPreviewForReorder };
