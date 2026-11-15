/** Project profile material declarations into shared Dynamic Property Set descriptors. */
export function materialPropertyDescriptors(profile, materialSchema) {
  const declarations = profile?.materials?.properties;
  const properties = materialSchema?.properties;
  if (!Array.isArray(declarations) || !properties || typeof properties !== 'object') return [];
  return declarations.map(declaration => {
    // The shared editor modelSchema intentionally stays scenario-neutral. A
    // scenario's JSON contract therefore supplies the fallback schema for its
    // dynamic material fields when the editor projection has only generic
    // material properties.
    const fieldSchema = properties[declaration?.key] || declaration?.schema;
    if (typeof declaration?.key !== 'string' || !declaration.key || !fieldSchema
      || typeof fieldSchema !== 'object' || Array.isArray(fieldSchema)) {
      throw new Error(`Material property ${String(declaration?.key)} has no matching field schema.`);
    }
    if (!Object.hasOwn(declaration, 'default')) {
      throw new Error(`Material property ${declaration.key} has no declared default.`);
    }
    const descriptorSchema = structuredClone(fieldSchema);
    const presentation = descriptorSchema['x-fullmeta-ui-behavior']?.payload?.presentation;
    if (presentation) delete presentation.hidden;
    return { key: declaration.key, label: declaration.label || declaration.key,
      schema: descriptorSchema, default: structuredClone(declaration.default) };
  });
}
