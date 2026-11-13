/** A field plot starts with the active formulation's first declared primary field. */
export function fieldPlotTreeItem({ model, destinationPath, makeId, makeName, profile }) {
  const formulation = model?.model?.physics?.formulation;
  const fields = profile?.fields?.formulations?.[formulation]?.fieldVariables;
  const component = fields?.find(field => typeof field?.symbol === 'string' && /^[A-Za-z_][A-Za-z0-9_]*$/.test(field.symbol));
  if (!component || typeof makeId !== 'function' || typeof makeName !== 'function') return undefined;
  return { id: makeId('fieldPlot'), name: makeName(destinationPath, 'Domain field plot'), type: 'field',
    quantity: { type: 'fieldComponent', component: component.symbol, part: 'magnitude' } };
}

export function globalPlotTreeItem({ model, destinationPath, makeId, makeName }) {
  const study = (model?.model?.study || []).find(item => item?.type === 'frequencyDomain'
    && typeof item.id === 'string' && /^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(item.id)
    && Array.isArray(item.parameterSweep?.parameters) && item.parameterSweep.parameters.length > 0
    && (model?.model?.parameters || []).some(parameter => parameter?.symbol === item.parameterSweep.parameters[0]?.parameter
      && typeof parameter.unit === 'string' && parameter.unit));
  const axis = study?.parameterSweep?.parameters?.[0]?.parameter;
  const parameter = (model?.model?.parameters || []).find(item => item?.symbol === axis && typeof item.unit === 'string' && item.unit);
  const variable = (model?.model?.variables || []).find(item => typeof item?.symbol === 'string'
    && /^[A-Za-z_][A-Za-z0-9_]*$/.test(item.symbol) && typeof item.unit === 'string' && item.unit);
  if (!study || !parameter || !variable || typeof makeId !== 'function' || typeof makeName !== 'function') return undefined;
  return { id: makeId('globalPlot'), name: makeName(destinationPath, 'Global variable plot'), type: 'plot.sweep',
    datasets: [{ id: makeId('dataset'), name: 'Study data', source: { type: 'study', studyId: study.id },
      xColumn: parameter.symbol, xUnit: parameter.unit, style: 'solid' }],
    series: [{ id: makeId('series'), name: variable.symbol, column: variable.symbol, unit: variable.unit, axis: 'left' }],
    output: { directory: 'plots', figureFilename: 'plot.svg' } };
}
