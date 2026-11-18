const wasmUrl = new URL("./wasm/fullmeta-core.wasm", import.meta.url);
const isNode = typeof process !== "undefined" && Boolean(process.versions?.node);

let wasmBytes;
if (isNode) {
  const { readFile } = await import("node:fs/promises");
  wasmBytes = await readFile(wasmUrl);
} else {
  const response = await fetch(wasmUrl);
  if (!response.ok) throw new Error(`Could not load the expression core (${response.status}).`);
  wasmBytes = await response.arrayBuffer();
}

const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const wasm = instance.exports;
if (!wasm.memory || !wasm.core_alloc || !wasm.core_execute || !wasm.core_output_pointer) {
  throw new Error("The expression core WASM file does not expose the expected API.");
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function coreRequest(operation, fields = {}) {
  const requestBytes = encoder.encode(JSON.stringify({ operation, ...fields }));
  const requestPointer = wasm.core_alloc(requestBytes.length) >>> 0;
  new Uint8Array(wasm.memory.buffer, requestPointer, requestBytes.length).set(requestBytes);
  try {
    wasm.core_execute(requestPointer, requestBytes.length);
  } finally {
    wasm.core_dealloc(requestPointer, requestBytes.length);
  }

  const responsePointer = wasm.core_output_pointer() >>> 0;
  const responseLength = wasm.core_output_length() >>> 0;
  const responseBytes = new Uint8Array(wasm.memory.buffer, responsePointer, responseLength).slice();
  wasm.core_clear_output();
  let response;
  try {
    response = JSON.parse(decoder.decode(responseBytes));
  } catch (error) {
    throw new Error(`The expression core returned invalid data: ${error.message}`);
  }
  if (!response.ok) throw new Error(response.error || "The expression core failed.");
  return response.value;
}

export function inspectExpression(expression) {
  return coreRequest("inspect_expression", { expression: String(expression ?? "0") });
}

export function evaluateExpression(expression, symbols = {}, definitions = [], stack = []) {
  return coreRequest("evaluate_expression", {
    expression: String(expression ?? "0"),
    symbols,
    definitions,
    stack,
  });
}

export function validateExpression(expression, symbols = {}, definitions = [], evaluate = false) {
  return coreRequest("validate_expression", {
    expression: String(expression ?? "0"),
    symbols,
    definitions,
    evaluate,
  });
}

export function getCoreUnit(unit) {
  return coreRequest("get_unit", { unit });
}

export function getCoreSupportedUnits() {
  return coreRequest("supported_units");
}

export function evaluateQuantityExpression(expression, symbols = {}, path = "expression", allowedFunctions = [], evaluationPolicy) {
  return coreRequest("evaluate_model_expression", { expression, symbols, path, allowedFunctions, evaluationPolicy });
}

export function evaluateQuantity(quantity, symbols = {}, path = "quantity", allowedFunctions = [], evaluationPolicy) {
  return coreRequest("evaluate_quantity", { quantity, symbols, path, allowedFunctions, evaluationPolicy });
}

export function inferQuantityDimension(quantity, symbols = {}, path = "quantity", allowedFunctions = []) {
  return coreRequest("infer_quantity_dimension", { quantity, symbols, path, allowedFunctions });
}

export function resolveParameterTable(parameters = [], reservedSymbols = [], allowedFunctions = [], parameterPath = "parameters", evaluationPolicy) {
  return coreRequest("resolve_parameters", { parameters, reservedSymbols, allowedFunctions, parameterPath, evaluationPolicy });
}

export function analyzeExpressionDefinitions(input) {
  return coreRequest("analyze_expression_definitions", { input });
}

export function evaluatePointwiseFields(input) {
  return coreRequest("evaluate_pointwise_fields", input);
}

// Trees use the same JSON representation returned by inspectExpression.
export function parseExpression(expression) { return coreRequest("parse_expression", { expression }); }
export function nodeSymbols(tree) { return coreRequest("node_symbols", { tree }); }
export function nodeCalls(tree) { return coreRequest("node_calls", { tree }); }
export function referencesAny(tree, names) { return coreRequest("references_any", { tree, names }); }
export function hasVariableExponent(tree, names) { return coreRequest("has_variable_exponent", { tree, names }); }
export function evaluateAst(tree, symbols = {}, definitions = [], stack = []) {
  return coreRequest("evaluate_ast", { tree, symbols, definitions, stack });
}
export function evaluateFunction(name, args) { return coreRequest("evaluate_function", { name, args }); }
export function dimensionFromJson(dimension) { return coreRequest("dimension_from_json", { dimension }); }
export function inferQuantityDimensionWithPolicy(quantity, symbols, path, evaluationPolicy) {
  return coreRequest("infer_quantity_dimension_with_policy", { quantity, symbols, path, evaluationPolicy });
}
export function symbolsFromPairs(pairs) { return coreRequest("symbols_from_pairs", { pairs }); }
// Complex numbers are objects of the form { re, im }. Definitions are
// [symbol, tree] pairs; symbols is a map of names to complex numbers.
export function evaluateComplexCall(name, args) { return coreRequest("evaluate_complex_call", { name, args }); }
export function evaluateComplexAst(tree, symbols = {}) { return coreRequest("evaluate_complex_ast", { tree, symbols }); }
export function evaluateComplexDefinitions(definitions, symbols = {}) {
  return coreRequest("evaluate_complex_definitions", { definitions, symbols });
}
export function validateEvaluationPolicy(evaluationPolicy) { return coreRequest("validate_evaluation_policy", { evaluationPolicy }); }
export function parseEvaluationPolicy(evaluationPolicy) { return coreRequest("parse_evaluation_policy", { evaluationPolicy }); }

// Handles belong to this WASM instance and remain live until explicitly released.
export function compileExpression(expression, evaluationPolicy, path = "expression") {
  return coreRequest("compile_expression", { expression, evaluationPolicy, path });
}
export function compileQuantity(quantity, evaluationPolicy, path = "quantity") {
  return coreRequest("compile_quantity", { quantity, evaluationPolicy, path });
}
export function compiledUsesSymbol(handle, name) { return coreRequest("compiled_uses_symbol", { handle, name }); }
export function evaluateCompiled(handle, symbols = {}, path = "expression") {
  return coreRequest("evaluate_compiled", { handle, symbols, path });
}
export function evaluateCompiledBatch(handle, symbolTables, path = "expression") {
  return coreRequest("evaluate_compiled_batch", { handle, symbolTables, path });
}
export function releaseCompiled(handle) { return coreRequest("release_compiled", { handle }); }
