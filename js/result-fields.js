const unitScaleToHz = Object.freeze({ Hz: 1, kHz: 1e3, MHz: 1e6, GHz: 1e9 });

function tidyNumber(value) {
  if (!Number.isFinite(value)) return String(value);
  return String(Number(value.toPrecision(6)));
}

function frequencyParameter(model) {
  const expression = model?.model?.physics?.frequency?.expression;
  if (typeof expression !== 'string' || !expression.trim()) return null;
  const symbol = expression.trim();
  const parameters = model?.model?.parameters;
  if (!Array.isArray(parameters)) return null;
  const matches = parameters.filter(parameter => parameter?.symbol === symbol);
  if (matches.length !== 1 || !Object.hasOwn(unitScaleToHz, matches[0]?.unit)) return null;
  return { symbol, unit: matches[0].unit };
}

function fallbackFrequencyUnit(model, declaredUnit) {
  if (Object.hasOwn(unitScaleToHz, declaredUnit)) return declaredUnit;
  return model?.scenario?.profileId === 'em-3d' ? 'GHz' : 'Hz';
}

/** Format checked GHz result coordinates using linked Physics/parameter declarations. */
export function formatFieldFrequency(frequencyGHz, model, _action, declaredUnit) {
  if (model?.scenario?.physics === 'thermal' || !Number.isFinite(frequencyGHz) || frequencyGHz <= 0) return '';
  const frequencyHz = frequencyGHz * unitScaleToHz.GHz;
  if (!Number.isFinite(frequencyHz) || frequencyHz <= 0) return '';

  const parameter = frequencyParameter(model);
  const unit = parameter?.unit || fallbackFrequencyUnit(model, declaredUnit);
  const value = frequencyHz / unitScaleToHz[unit];
  const label = `${tidyNumber(value)} ${unit}`;
  return parameter ? `${parameter.symbol} = ${label}` : label;
}

export const resultFields = Object.freeze({ formatFieldFrequency });
