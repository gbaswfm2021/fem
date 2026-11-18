import { documentValue, documentSnapshot, currentDocumentKey } from "../../../../platform/js/document-state.js";
const validation = documentValue('fem.model.validation', () => ({ activeSchema: undefined, activeDocuments: [], activeSchemaBundle: undefined,
  activeSchemaHandle: undefined, schemaBundles: new WeakMap(), schemaFragmentBundles: new Map(), schemaNodeContexts: new WeakMap(), resolvedSchemaNodes: new WeakMap() }));
const wasmUrl = new URL("./wasm/fullmeta-model.wasm", import.meta.url);
const isNode = typeof process !== "undefined" && Boolean(process.versions?.node);

let wasmBytes;
if (isNode) {
  const { readFile } = await import("node:fs/promises");
  wasmBytes = await readFile(wasmUrl);
} else {
  const response = await fetch(wasmUrl, { cache: "no-store" });
  if (!response.ok) throw new Error(`Could not load the model contract module (${response.status}).`);
  wasmBytes = await response.arrayBuffer();
}

const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const wasm = instance.exports;
if (!wasm.memory || !wasm.model_alloc || !wasm.model_execute || !wasm.model_output_pointer) {
  throw new Error("The model contract WASM file does not expose the expected API.");
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const isSchema = (schema) => typeof schema === "boolean"
  || (schema !== null && typeof schema === "object" && !Array.isArray(schema));
const pointerKey = (key) => String(key).replace(/~/g, "~0").replace(/\//g, "~1");
const schemaMapKeywords = new Set(["$defs", "definitions", "properties", "patternProperties", "dependentSchemas", "dependencies"]);
const schemaArrayKeywords = new Set(["allOf", "anyOf", "oneOf", "prefixItems"]);
const schemaValueKeywords = new Set(["additionalProperties", "additionalItems", "items", "contains", "propertyNames", "unevaluatedItems", "unevaluatedProperties", "not", "if", "then", "else", "contentSchema"]);
const registeredSchemaOperations = new Set(["validate_schema", "validate_json_schema", "validate_json_schema_at", "schema_resolve", "schema_property", "schema_item", "schema_at_path", "validate_rules", "validate_model", "bind_runtime_policy", "transaction", "validated_transaction"]);

function schemaChildren(value, includeDefinitions = true) {
  return Object.entries(value).flatMap(([key, child]) => {
    if (key === "x-fullmeta-modelReference" && child && typeof child === "object"
      && Object.hasOwn(child, "targetSchema") && isSchema(child.targetSchema)) {
      return [["/x-fullmeta-modelReference/targetSchema", child.targetSchema]];
    }
    if (!includeDefinitions && (key === "$defs" || key === "definitions")) return [];
    if (schemaMapKeywords.has(key) && child && typeof child === "object" && !Array.isArray(child)) {
      return Object.entries(child).filter(([, schema]) => isSchema(schema))
        .map(([name, schema]) => [`/${pointerKey(key)}/${pointerKey(name)}`, schema]);
    }
    if ((schemaArrayKeywords.has(key) || key === "items") && Array.isArray(child)) {
      return child.map((schema, index) => [`/${pointerKey(key)}/${index}`, schema]);
    }
    return schemaValueKeywords.has(key) && isSchema(child) ? [[`/${pointerKey(key)}`, child]] : [];
  });
}

/** Supply immutable application schemas; reconfigure after replacing their data. */
export function configureModelSchema(schema, documents = []) {
  schema = documentSnapshot(schema);
  documents = documents.map(documentSnapshot);
  if (!isSchema(schema)) throw new Error("The active model schema must be a JSON Schema object or boolean.");
  if (!Array.isArray(documents)) throw new Error("Model schema documents must be an array.");
  const seen = new Map();
  const configured = [];
  for (const document of [schema, ...documents]) {
    if (!isSchema(document)) throw new Error("Every model schema document must be a JSON Schema object or boolean.");
    if (configured.includes(document)) continue;
    const id = document?.$id;
    if (id !== undefined) {
      if (typeof id !== "string" || !id) throw new Error("A schema document $id must be a non-empty string.");
      if (seen.has(id)) throw new Error(`More than one model schema document declares $id ${JSON.stringify(id)}.`);
      seen.set(id, document);
    }
    configured.push(document);
  }
  validation.activeSchema = schema;
  validation.activeDocuments = configured;
  validation.schemaBundles = new WeakMap();
  validation.schemaFragmentBundles = new Map();
  validation.schemaNodeContexts = new WeakMap();
  validation.resolvedSchemaNodes = new WeakMap();
  validation.activeSchemaBundle = bundledSchema(schema);
  const previousHandle = validation.activeSchemaHandle;
  validation.activeSchemaHandle = undefined;
  const nextHandle = currentDocumentKey() || 'browser-document';
  const registered = modelRequest("configure_schema", {
    schemaHandle: nextHandle,
    schema,
    documents: configured.slice(1),
    bundledSchema: validation.activeSchemaBundle,
  });
  validation.activeSchemaHandle = registered.schemaHandle;
  if (previousHandle && previousHandle !== nextHandle) modelRequest("release_schema", { schemaHandle: previousHandle });
}

/** Whether this schema is registered for the current opened file (or browser tab). */
export function isModelSchemaConfigured(schema) {
  return validation.activeSchema === documentSnapshot(schema) && Boolean(validation.activeSchemaHandle);
}

function configuredSchema() {
  if (validation.activeSchema === undefined) throw new Error("Configure the application's model schema with configureModelSchema(schema, documents) before validating or editing models.");
  return validation.activeSchema;
}

function resourceId(id, base) {
  try { return new URL(id, base).href.replace(/#$/, ""); }
  catch { return id.replace(/#$/, ""); }
}

function schemaDocument(source, resources) {
  const document = { source, scopes: new Map() };
  const initial = { document, pointer: "", id: undefined };
  const ancestors = new Set();
  function visit(value, pointer, inherited) {
    if (!value || typeof value !== "object") return;
    if (ancestors.has(value)) throw new Error("JSON Schema documents must not contain circular JavaScript objects.");
    ancestors.add(value);
    let scope = inherited;
    if (!Array.isArray(value) && Object.hasOwn(value, "$id")) {
      if (typeof value.$id !== "string" || !value.$id) throw new Error("A schema document $id must be a non-empty string.");
      scope = { document, pointer, id: resourceId(value.$id, inherited.id) };
      const previous = resources.get(scope.id);
      if (previous && (previous.document.source !== source || previous.pointer !== pointer)) {
        throw new Error(`More than one schema resource declares $id ${JSON.stringify(scope.id)}.`);
      }
      resources.set(scope.id, scope);
    }
    document.scopes.set(pointer, scope);
    for (const [suffix, child] of schemaChildren(value)) visit(child, `${pointer}${suffix}`, scope);
    ancestors.delete(value);
  }
  visit(source, "", initial);
  document.root = document.scopes.get("") || initial;
  document.register = visit;
  return document;
}

/** Convert registered document references to local pointers for the WASM validator. */
function bundledSchema(schema, entrypoints = []) {
  schema = documentSnapshot(schema);
  if (!isSchema(schema)) throw new Error("Validation requires a JSON Schema object or boolean.");
  if (typeof schema === "boolean") return schema;
  const entrypointKey = JSON.stringify([...new Set(entrypoints)].sort());
  const cached = validation.schemaBundles.get(schema)?.get(entrypointKey);
  if (cached) return cached;

  const resources = new Map();
  for (const source of validation.activeDocuments) {
    // An explicitly supplied replacement for a registered root takes precedence.
    if (source === schema || (schema.$id && source?.$id === schema.$id)) continue;
    schemaDocument(source, resources);
  }
  const root = schemaDocument(schema, resources);
  let namespace = "__modelSchemaDocuments";
  while (Object.hasOwn(schema.$defs || {}, namespace)) namespace += "_";
  const bundle = structuredClone(schema);
  const prefixes = new Map([[root, "#"]]);
  const outputs = new Map([[root, bundle]]);
  const pending = [{ document: root, pointer: "" }, ...entrypoints.map((pointer) => ({ document: root, pointer }))];
  const visited = new Map();
  function prefix(document) {
    if (!prefixes.has(document)) {
      const key = `document${prefixes.size - 1}`;
      prefixes.set(document, `#/$defs/${namespace}/${key}`);
      const output = structuredClone(document.source);
      outputs.set(document, output);
      if (!bundle.$defs) bundle.$defs = {};
      if (!bundle.$defs || typeof bundle.$defs !== "object" || Array.isArray(bundle.$defs)) {
        throw new Error("Schema $defs must be an object.");
      }
      if (!Object.hasOwn(bundle.$defs, namespace)) bundle.$defs[namespace] = {};
      bundle.$defs[namespace][key] = output;
    }
    return prefixes.get(document);
  }
  function atPointer(source, pointer) {
    let value = source;
    for (const raw of pointer.split("/").slice(1)) {
      const key = raw.replace(/~1/g, "/").replace(/~0/g, "~");
      if (!value || typeof value !== "object" || !Object.hasOwn(value, key)) {
        throw new Error(`Unresolved JSON Schema pointer ${JSON.stringify(pointer)}.`);
      }
      value = value[key];
    }
    return value;
  }
  function reference(value, scope) {
    if (typeof value !== "string") throw new Error("JSON Schema $ref values must be strings.");
    const marker = value.indexOf("#");
    const targetId = marker < 0 ? value : value.slice(0, marker);
    const fragment = marker < 0 ? "" : value.slice(marker + 1);
    if (fragment && !fragment.startsWith("/")) throw new Error(`JSON Schema reference ${JSON.stringify(value)} must use a JSON Pointer fragment.`);
    const target = targetId
      ? resources.get(resourceId(targetId, scope.id)) || resources.get(targetId)
      : scope;
    if (!target) throw new Error(`JSON Schema reference ${JSON.stringify(value)} requires document ${JSON.stringify(resourceId(targetId, scope.id))}, which was not supplied to configureModelSchema.`);
    let pointer;
    try { pointer = decodeURIComponent(fragment); }
    catch { throw new Error(`JSON Schema reference ${JSON.stringify(value)} has an invalid encoded fragment.`); }
    const targetPointer = `${target.pointer}${pointer}`;
    if (!isSchema(atPointer(target.document.source, targetPointer))) {
      throw new Error(`JSON Schema reference ${JSON.stringify(value)} must target an object or boolean schema.`);
    }
    const rewritten = `${prefix(target.document)}${targetPointer}`;
    pending.push({ document: target.document, pointer: targetPointer });
    return rewritten;
  }
  for (let index = 0; index < pending.length; index++) {
    const { document, pointer } = pending[index];
    if (!visited.has(document)) visited.set(document, new Set());
    if (visited.get(document).has(pointer)) continue;
    visited.get(document).add(pointer);
    const node = atPointer(document.source, pointer);
    if (!node || typeof node !== "object") continue;
    // A reference can enter a namespaced definition map whose entries are
    // schemas only when referenced. Register and rewrite that target using the
    // closest enclosing schema scope, while leaving annotations/defaults alone.
    if (!document.scopes.has(pointer)) {
      let ancestor = pointer;
      while (!document.scopes.has(ancestor) && ancestor) ancestor = ancestor.slice(0, ancestor.lastIndexOf("/"));
      document.register(node, pointer, document.scopes.get(ancestor) || document.root);
    }
    const scope = document.scopes.get(pointer) || document.root;
    const output = atPointer(outputs.get(document), pointer);
    delete output.$id;
    delete output.$schema;
    if (Object.hasOwn(node, "$ref")) output.$ref = reference(node.$ref, scope);
    for (const [suffix] of schemaChildren(node, false)) pending.push({ document, pointer: `${pointer}${suffix}` });
  }
  if (!validation.schemaBundles.has(schema)) validation.schemaBundles.set(schema, new Map());
  validation.schemaBundles.get(schema).set(entrypointKey, bundle);
  return bundle;
}

export function modelRequest(operation, fields = {}) {
  if (Object.hasOwn(fields, 'schema')) fields = { ...fields, schema: documentSnapshot(fields.schema) };
  if (validation.activeSchemaHandle && registeredSchemaOperations.has(operation)
    && (fields.schema === validation.activeSchema || fields.schema === validation.activeSchemaBundle)) {
    const { schema, documents, ...remaining } = fields;
    fields = { ...remaining, schemaHandle: validation.activeSchemaHandle };
  }
  const requestBytes = encoder.encode(JSON.stringify({ operation, ...fields }));
  const requestPointer = wasm.model_alloc(requestBytes.length) >>> 0;
  new Uint8Array(wasm.memory.buffer, requestPointer, requestBytes.length).set(requestBytes);
  try {
    wasm.model_execute(requestPointer, requestBytes.length);
  } finally {
    wasm.model_dealloc(requestPointer, requestBytes.length);
  }

  const responsePointer = wasm.model_output_pointer() >>> 0;
  const responseLength = wasm.model_output_length() >>> 0;
  const responseBytes = new Uint8Array(wasm.memory.buffer, responsePointer, responseLength).slice();
  wasm.model_clear_output();
  let response;
  try {
    response = JSON.parse(decoder.decode(responseBytes));
  } catch (error) {
    throw new Error(`The model contract module returned invalid data: ${error.message}`);
  }
  if (!response.ok) throw new Error(response.error || "The model contract operation failed.");
  return response.value;
}

export function validateModelSchema(model) {
  return modelRequest("validate_schema", { model, schema: bundledSchema(configuredSchema()) });
}

/** Run the schema-declared rule program with caller-supplied application context. */
export function validateModelRules(model, context = {}, schema = configuredSchema()) {
  const documents = validation.activeDocuments.filter((document) => document !== schema
    && !(schema?.$id && document?.$id === schema.$id));
  return modelRequest("validate_rules", { model, schema, documents, context });
}

/** Bind an explicit runtime-policy declaration using the existing generic rule graph. */
export function bindRuntimePolicy(model, context, declaration, schema = configuredSchema()) {
  return modelRequest("bind_runtime_policy", { model, schema, context, declaration });
}

/** Run another rule declaration from a schema without changing the configured model schema. */
export function validateDeclaredRules(model, schema, annotation, documents = [], context = {}) {
  if (!schema || typeof schema !== "object" || Array.isArray(schema)
    || typeof annotation !== "string" || !Object.hasOwn(schema, annotation)) {
    throw new Error(`The schema must declare ${annotation}.`);
  }
  if (!Array.isArray(documents)) throw new Error("Rule declaration documents must be an array.");
  return modelRequest("validate_rules", {
    model, schema: { ...schema, "x-fullmeta-rules": schema[annotation] }, documents, context,
  });
}

/** Validate JSON structure and schema-declared application-neutral rules in one WASM call. */
export function validateModelReport(model, context = {}, schema = configuredSchema()) {
  const documents = validation.activeDocuments.filter((document) => document !== schema
    && !(schema?.$id && document?.$id === schema.$id));
  return modelRequest("validate_model", { model, schema, documents, context });
}

/** Validate structure first, then run the schema-declared semantic rule program. */
export function validateModelWithRules(model, context = {}, schema = configuredSchema()) {
  return validateModelReport(model, context, schema).errors;
}

export function validateJsonSchema(value, schema) {
  return modelRequest("validate_json_schema", { value, schema: bundledSchema(schema) });
}

function schemaPointerForFragment(root, fragment) {
  const ancestors = new Set();
  function find(node, pointer) {
    if (node === fragment) return pointer;
    if (!node || typeof node !== "object") return null;
    if (ancestors.has(node)) throw new Error("JSON Schema documents must not contain circular JavaScript objects.");
    ancestors.add(node);
    for (const [suffix, child] of schemaChildren(node)) {
      const found = find(child, `${pointer}${suffix}`);
      if (found !== null) {
        ancestors.delete(node);
        return found;
      }
    }
    ancestors.delete(node);
    return null;
  }
  return find(root, "");
}

/** Keep localized references in returned schema nodes bound to their owner. */
function rememberSchemaNodes(node, schema) {
  const visited = new Set();
  function visit(value) {
    if (!value || typeof value !== "object" || visited.has(value)) return;
    visited.add(value);
    validation.schemaNodeContexts.set(value, { schema, schemaNode: value });
    for (const [, child] of schemaChildren(value)) visit(child);
  }
  visit(node);
  return node;
}

/** Original nodes preserve their pointer/$id; detached nodes use root scope. */
function schemaFragmentContext(fragment, root) {
  if (!isSchema(fragment)) throw new Error("Schema navigation requires an object or boolean schema.");
  const owner = fragment && typeof fragment === "object" ? validation.schemaNodeContexts.get(fragment) : undefined;
  if (owner) return owner;
  root = documentSnapshot(root ?? configuredSchema());
  if (!isSchema(root)) throw new Error("The schema context must be an object or boolean schema.");
  if (!validation.schemaFragmentBundles.has(root)) validation.schemaFragmentBundles.set(root, new Map());
  const fragments = validation.schemaFragmentBundles.get(root);
  let context = fragments.get(fragment);
  if (!context) {
    const existingPath = schemaPointerForFragment(root, fragment);
    if (existingPath !== null) {
      context = { schema: bundledSchema(root, [existingPath]), schemaPath: existingPath };
    } else {
      const wrapper = typeof root === "boolean" ? { allOf: [root] } : structuredClone(root);
      if (wrapper.$defs !== undefined && (!wrapper.$defs || typeof wrapper.$defs !== "object" || Array.isArray(wrapper.$defs))) {
        throw new Error("Schema $defs must be an object.");
      }
      wrapper.$defs ||= {};
      let name = "__modelSchemaFragment";
      while (Object.hasOwn(wrapper.$defs, name)) name += "_";
      wrapper.$defs[name] = structuredClone(fragment);
      const schemaPath = `/$defs/${pointerKey(name)}`;
      context = { schema: bundledSchema(wrapper, [schemaPath]), schemaPath };
    }
    fragments.set(fragment, context);
  }
  return context;
}

/** Validate a predicate using its supplied or remembered schema context. */
export function validateSchemaFragment(value, fragment, root) {
  const context = schemaFragmentContext(fragment, root);
  return modelRequest("validate_json_schema_at", { value, ...context });
}

/** Resolve references only; branch selection remains a separate operation. */
export function resolveSchemaNode(fragment, root) {
  if (!isSchema(fragment) || typeof fragment?.$ref !== "string") return fragment;
  const context = schemaFragmentContext(fragment, root);
  if (fragment && typeof fragment === "object") {
    const cached = validation.resolvedSchemaNodes.get(fragment);
    if (cached?.context === context) return cached.value;
  }
  const value = rememberSchemaNodes(modelRequest("schema_resolve", context), context.schema);
  if (fragment && typeof fragment === "object") validation.resolvedSchemaNodes.set(fragment, { context, value });
  return value;
}

/** Find the effective schema at a JSON tree path. */
export function schemaAtPath(model, path, { resolveVariant = true } = {}, root = configuredSchema()) {
  const schema = bundledSchema(root);
  return rememberSchemaNodes(modelRequest("schema_at_path", { model, schema, path: path || [], resolveVariant }), schema);
}

/** Select a property from a schema, accounting for the current draft value. */
export function schemaProperty(fragment, value, key, root) {
  const context = schemaFragmentContext(fragment, root);
  const fields = value === undefined ? {} : { value };
  return rememberSchemaNodes(modelRequest("schema_property", { ...context, ...fields, key }), context.schema);
}

/** Select an array item from a schema, accounting for the current draft value. */
export function schemaItem(fragment, value, index, root) {
  const context = schemaFragmentContext(fragment, root);
  const fields = value === undefined ? {} : { value };
  return rememberSchemaNodes(modelRequest("schema_item", { ...context, ...fields, index }), context.schema);
}

// Compatibility name for generic schema validation. Application semantics belong
// to the application's native validator and are not part of this WASM API.
export function validateModel(model) {
  return validateModelSchema(model);
}

export function modelTransaction(transaction, model, fields = {}) {
  const schema = Object.hasOwn(fields, "schema") ? fields.schema : configuredSchema();
  return modelRequest("transaction", { transaction, model, ...fields, schema: bundledSchema(schema) });
}

export function validatedModelTransaction(transaction, model, fields = {}) {
  const schema = Object.hasOwn(fields, "schema") ? fields.schema : configuredSchema();
  const response = modelRequest("validated_transaction", { transaction, model, ...fields, schema: bundledSchema(schema) });
  const validationErrors = response.validationErrors || [];
  if (validationErrors.length) {
    const error = new Error(validationErrors[0]);
    error.validationErrors = validationErrors;
    throw error;
  }
  return response.model;
}

// These operations work on explicit caller-supplied data and do not select
// application schemas or read the host filesystem.
export function decodeArguments(args, declaration, schema) { return modelRequest("argv_decode", {arguments: args, declaration, schema}); }
export function normalizeArgument(value, declaration, schema) { return modelRequest("argv_string_value", {value, declaration, schema}); }
export function collectProgramRegistry(documents) { return modelRequest("program_registry", {documents}); }
export function validateRuleDeclaration(model, schema, declaration, documents = [], context = {}) {
  return modelRequest("validate_rule_declaration", {model, schema, declaration, documents, context});
}
export function resolveOptions(resolution, optionSets, conditionalRules, sources, context) {
  return modelRequest("resolve_options", {resolution, optionSets, conditionalRules, sources, context});
}
export function validateResolvedNumber(value, rules, path, unit) { return modelRequest("validate_resolved_number", {value, rules, path, unit}); }
export function numericBounds(value, rules, path, unit, errors = []) { return modelRequest("numeric_bounds", {value, rules, path, unit, errors}); }
export function validateSemanticCollections(input) { return modelRequest("validate_semantic_collections", {input}); }
export function validateSemanticExpressions(input) { return modelRequest("validate_semantic_expressions", {input}); }
export function validateSemanticQuantities(input) { return modelRequest("validate_semantic_quantities", {input}); }
export function uniqueIdErrors(value) { return modelRequest("unique_id_errors", {value}); }
export function bundleModelSchema(schema, documents = []) { return modelRequest("bundle_schema", {schema, documents}); }
export function bundleModelSchemaWithEntrypoints(schema, documents = [], entrypoints = []) {
  return modelRequest("bundle_schema_with_entrypoints", {schema, documents, entrypoints});
}

// Context handles belong to this model WASM instance; release them explicitly.
export function createSchemaContext(schema, documents = [], entrypoints = []) {
  return modelRequest("schema_context_new", {schema, documents, entrypoints});
}
export function schemaContextFromDocuments(documents) { return modelRequest("schema_context_from_documents", {documents}); }
export function schemaContextRoot(handle) { return modelRequest("schema_context_root", {handle}); }
export function schemaContextDocuments(handle) { return modelRequest("schema_context_documents", {handle}); }
export function schemaContextDocumentAnnotations(handle, key) { return modelRequest("schema_context_document_annotations", {handle, key}); }
export function schemaContextSchemaAnnotations(handle, key) { return modelRequest("schema_context_schema_annotations", {handle, key}); }
export function schemaContextModelRoleValues(handle, model, role) { return modelRequest("schema_context_model_role_values", {handle, model, role}); }
export function takeSchemaContextRoot(handle) { return modelRequest("schema_context_take_root", {handle}); }
export function releaseSchemaContext(handle) { return modelRequest("schema_context_release", {handle}); }

export function evaluateComplexQuantity(quantity, symbols, policy, path = "value") {
  return modelRequest("evaluate_complex_quantity", {quantity, symbols, policy, path});
}
export function compileComplexQuantity(quantity, policy, path = "value") {
  return modelRequest("compile_complex_quantity", {quantity, policy, path});
}
export function evaluateCompiledComplexQuantity(handle, symbols = {}, path = "value") {
  return modelRequest("evaluate_compiled_complex_quantity", {handle, symbols, path});
}
export function evaluateCompiledComplexQuantityBatch(handle, symbolTables, path = "value") {
  return modelRequest("evaluate_compiled_complex_quantity_batch", {handle, symbolTables, path});
}
export function releaseCompiledComplexQuantity(handle) { return modelRequest("release_compiled_complex_quantity", {handle}); }

// Native file operations require an explicitly supplied host. The host is
// responsible for file confinement, link checks and native path semantics.
function hostOperation(host, name) {
  if (!host || typeof host[name] !== "function") throw new Error(`The model filesystem host must provide ${name}().`);
  return host[name].bind(host);
}
export async function loadSchemaDocumentsFromHost(path, host) {
  const documents = await hostOperation(host, "loadSchemaDocuments")(path);
  if (!Array.isArray(documents)) throw new Error("The model filesystem host must return a schema document array.");
  return documents;
}
export async function schemaContextFromHostPath(path, host) {
  return schemaContextFromDocuments(await loadSchemaDocumentsFromHost(path, host));
}
export async function installationRootFromHost(host) {
  return hostOperation(host, "installationRoot")();
}
export async function externalProgramPathFromHost(documents, schemaFile, root, id, host) {
  return hostOperation(host, "externalProgramPath")(documents, schemaFile, root, id);
}
export async function runtimeToolPathFromHost(documents, schemaFile, root, id, host) {
  return hostOperation(host, "runtimeToolPath")(documents, schemaFile, root, id);
}
