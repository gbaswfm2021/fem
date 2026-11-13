/** Clone a JSON-compatible FEM document without normalizing its stored values. */
export function preserveModelDocument(source) {
  return structuredClone(source);
}

export const modelPreservation = Object.freeze({ preserveModelDocument });
