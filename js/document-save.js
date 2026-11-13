/** FEM Model 2 identity allocation through the public shared document routes. */
export async function saveFemDocument({ requestJson, folder, file, value, expectedHash, copyName, allowApplicationChange = false },
  { randomUUID = () => crypto.randomUUID() } = {}) {
  if (typeof expectedHash !== 'string' || !/^[a-fA-F0-9]{64}$/.test(expectedHash)) throw new Error('Reopen this project before saving.');
  if (copyName === undefined) {
    const result = await requestJson('/api/model/save', { method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ folder, file, model: value, expectedContentHash: expectedHash, overwrite: true, allowApplicationChange }) });
    return { ...result, file: [folder === '.' ? '' : folder, file].filter(Boolean).join('/') };
  }
  if (typeof copyName !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._-]*\.json$/i.test(copyName)) throw new Error('Choose a JSON filename without folder separators.');
  if (copyName.toLowerCase() === file.toLowerCase()) throw new Error('Choose a different filename for Save As.');
  const listing = await requestJson(`/api/models?folder=${encodeURIComponent(folder)}`);
  if (!listing || listing.folder !== folder || !Array.isArray(listing.documents) || !Array.isArray(listing.files))
    throw new Error('Could not check the current folder for existing model documents. Nothing was saved.');
  if (listing.files.some(name => typeof name === 'string' && name.toLowerCase() === copyName.toLowerCase()))
    throw new Error(`${copyName} already exists. Choose another filename.`);
  const used = new Set([value.metadata.id, ...listing.documents.map(document => document.modelId)]
    .filter(id => typeof id === 'string').map(id => id.toLowerCase()));
  let id;
  for (let attempt = 0; attempt < 16; attempt++) {
    const uuid = randomUUID();
    if (typeof uuid !== 'string' || !/^[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12}$/.test(uuid))
      throw new Error('The Model ID allocator returned an invalid UUID. Nothing was saved.');
    const candidate = `ModelCopy_${uuid.replaceAll('-', '')}`;
    if (!used.has(candidate.toLowerCase())) { id = candidate; break; }
  }
  if (!id) throw new Error('Could not allocate a unique Model ID in 16 attempts. Nothing was saved.');
  const copy = structuredClone(value); copy.metadata.id = id;
  return requestJson('/api/program/document/save', { method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ file: [folder === '.' ? '' : folder, file].filter(Boolean).join('/'), value: copy, expectedHash, copyName }) });
}
