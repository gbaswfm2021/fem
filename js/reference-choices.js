// FEM eligibility policy layered over the shared declared-reference service.
// The service still owns labels, uniqueness and unavailable saved values.
export function femReferenceOptions(options, resolve) {
  const {reference, model, settingsDraft} = options;
  if (settingsDraft?.type !== 'periodicCondition'
      || reference?.scope !== 'root'
      || JSON.stringify(reference.targetPath) !== JSON.stringify(['model','mesh','periodicPairs'])) {
    return resolve(options);
  }
  const mesh = model?.model?.mesh;
  const enabled = new Set(mesh?.periodicPairIds || []);
  return resolve({...options, model: {...model, model: {...model.model, mesh: {...mesh,
    periodicPairs: (mesh?.periodicPairs || []).filter(pair => enabled.has(pair.id)),
  }}}});
}
