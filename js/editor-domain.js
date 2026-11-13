import { resolveParameterTable } from '../../../platform/core-rust/js/api.js';
import { bindRuntimePolicy } from '../../../platform/model-rust/js/api.js';
import { modelWithSchemaValidSettingsItem } from '../../../platform/js/services.js';

/** Adapt the public Model Editor hooks to FEM's saved declarations. */
export function createEditorDomain({ schema, validate }) {
  const real = bindRuntimePolicy(null, {}, schema['x-fullmeta-expressionRuntime'].real, schema);
  const constants = real.builtinSymbols || [];
  const parameters = bindRuntimePolicy(null, { realPolicy: real, reservedSymbols: [...constants, 't'] },
    schema['x-fullmeta-expressionRuntime'].parameters, schema);
  return {
    resolveModelParameters(values) {
      return resolveParameterTable(values, parameters.reservedSymbols, parameters.allowedFunctions, parameters.path, real);
    },
    reservedExpressionSymbolsForModel() { return new Set(constants); },
    reservedParameterSymbolsForModel() { return new Set(parameters.reservedSymbols); },
    modelWithResultActionSettings(model, actionId, type, changes) {
      const matches = (model?.model?.results || []).map((action, index) => ({ action, index }))
        .filter(({ action }) => action.id === actionId);
      if (matches.length !== 1 || matches[0].action.type !== type) throw new Error('The selected result recipe changed or no longer exists.');
      if (!changes || typeof changes !== 'object' || Array.isArray(changes)) throw new Error('Result settings require a property map.');
      const { action, index } = matches[0];
      if (Object.hasOwn(changes, 'id') && changes.id !== action.id || Object.hasOwn(changes, 'type') && changes.type !== type)
        throw new Error('Result settings must preserve the recipe ID and type.');
      const candidate = modelWithSchemaValidSettingsItem(model, ['model', 'results', index], { ...action, ...structuredClone(changes) });
      const errors = validate(candidate);
      if (errors.length) { const error = new Error(errors[0]); error.validationErrors = errors; throw error; }
      return { model: candidate, action: candidate.model.results[index] };
    },
  };
}
