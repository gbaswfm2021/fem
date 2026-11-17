const unitScaleToHz = Object.freeze({ Hz: 1, kHz: 1e3, MHz: 1e6, GHz: 1e9 });
const displayFrequencyUnits = Object.freeze([
  ['pHz', 1e-12], ['nHz', 1e-9], ['µHz', 1e-6], ['mHz', 1e-3],
  ['Hz', 1], ['kHz', 1e3], ['MHz', 1e6], ['GHz', 1e9], ['THz', 1e12], ['PHz', 1e15],
]);

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

/** Choose a display unit by magnitude; persisted coordinates remain in Hz. */
export function engineeringFrequencyUnit(frequencyHz) {
  if (!Number.isFinite(frequencyHz) || frequencyHz <= 0) return {unit:'Hz',scale:1};
  let index = displayFrequencyUnits.findLastIndex(([, scale]) => frequencyHz >= scale);
  if (index < 0) index = 0;
  if (index + 1 < displayFrequencyUnits.length && Number((frequencyHz / displayFrequencyUnits[index][1]).toPrecision(6)) >= 1000) index++;
  const [unit, scale] = displayFrequencyUnits[index];
  return {unit,scale};
}

export function frequencyDisplayScale(unit) {
  return displayFrequencyUnits.find(([name]) => name === unit)?.[1];
}

/** Format a frequency with one engineering prefix; Hz remains the stored unit. */
export function formatFrequencyHz(frequencyHz) {
  if (!Number.isFinite(frequencyHz) || frequencyHz <= 0) return '';
  const {unit,scale} = engineeringFrequencyUnit(frequencyHz);
  return `${tidyNumber(frequencyHz / scale)} ${unit}`;
}

/** Format checked GHz result coordinates using linked Physics/parameter declarations. */
export function formatFieldFrequency(frequencyGHz, model, _action, _declaredUnit) {
  if (!model?.model?.physics?.frequency || !Number.isFinite(frequencyGHz) || frequencyGHz <= 0) return '';
  const frequencyHz = frequencyGHz * unitScaleToHz.GHz;
  if (!Number.isFinite(frequencyHz) || frequencyHz <= 0) return '';

  const parameter = frequencyParameter(model);
  const label = formatFrequencyHz(frequencyHz);
  return parameter ? `${parameter.symbol} = ${label}` : label;
}

export const resultFields = Object.freeze({ formatFieldFrequency });
