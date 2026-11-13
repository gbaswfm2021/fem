import { loadSchemaDocuments, loadWorkspaceJson } from '../../platform/js/schema-document-loader.js';
import { configureModelSchema } from '../../platform/model-rust/js/api.js';

const workspaceUrl = new URL('../../', import.meta.url);
let active;

// Mirror the native caller's embedded scenario documents, including their
// logical identifiers. The schema graph remains the source of profile data.
function embeddedScenarioDocuments(root) {
  const profiles = root?.$defs?.femProfiles;
  if (!profiles || typeof profiles !== 'object' || Array.isArray(profiles)) return [];
  if (typeof root.$id !== 'string' || !root.$id.includes('/')) {
    throw new Error('The FEM model schema must declare a $id with a document path before embedding profiles.');
  }
  const base = root.$id.slice(0, root.$id.lastIndexOf('/') + 1);
  return Object.entries(profiles).map(([key, profile]) => {
    const id = profile?.['x-fem-scenario']?.profile?.id;
    if (typeof id !== 'string') throw new Error(`Embedded FEM profile ${JSON.stringify(key)} must declare x-fem-scenario.profile.id.`);
    if (key !== id) throw new Error(`Embedded FEM profile key ${JSON.stringify(key)} must match its profile ID ${JSON.stringify(id)}.`);
    const document = { ...structuredClone(profile), $id: `${base}${id}.schema.json` };
    if (Object.hasOwn(root, '$schema')) document.$schema = root.$schema;
    return document;
  });
}

// Check each import's destination before asking the shared graph loader to
// follow it. In particular, a FEM schema cannot read another application.
async function checkSchemaBoundary(schemaUrl) {
  const applicationBase = new URL('applications/fem/', workspaceUrl);
  const platformBase = new URL('platform/', workspaceUrl);
  const allowed = url => url.origin === workspaceUrl.origin
    && (url.pathname.startsWith(applicationBase.pathname) || url.pathname.startsWith(platformBase.pathname));
  const root = new URL(schemaUrl, workspaceUrl);
  root.hash = '';
  if (!allowed(root) || !root.pathname.startsWith(applicationBase.pathname)) {
    throw new Error('The selected FEM schema must stay inside the FEM application.');
  }
  const seen = new Set(), identifiers = new Set();
  let identifierBase;
  async function visit(url) {
    url = new URL(url); url.hash = '';
    if (!allowed(url)) throw new Error('FEM schema imports must stay inside the FEM application or shared platform.');
    if (seen.has(url.href)) return;
    if (seen.size >= 4096) throw new Error('Too many schema documents.');
    seen.add(url.href);
    const document = await loadWorkspaceJson(url, workspaceUrl);
    if (!document || typeof document !== 'object' || Array.isArray(document) || typeof document.$id !== 'string') {
      throw new Error('Each schema document requires a string $id.');
    }
    const id = new URL(document.$id);
    identifierBase ||= new URL('./', id);
    identifiers.add(id.href);
    const refs = [];
    function collect(value) {
      if (!value || typeof value !== 'object') return;
      for (const [key, child] of Object.entries(value)) {
        if (key === '$ref' && typeof child === 'string') refs.push(child);
        else collect(child);
      }
    }
    collect(document);
    for (const reference of refs) {
      if (reference.includes('\\') || reference.startsWith('//')) throw new Error(`Unsupported schema reference ${reference}.`);
      const logical = new URL(reference, id); logical.hash = '';
      if (!logical.search && logical.href === id.href) continue;
      let target;
      if (!/^[A-Za-z][A-Za-z0-9+.-]*:/.test(reference) && !reference.startsWith('/')) {
        target = new URL(reference, url);
      } else {
        if (identifiers.has(logical.href)) continue;
        if (logical.search || logical.origin !== identifierBase.origin || !logical.pathname.startsWith(identifierBase.pathname)) {
          throw new Error(`Unsupported schema reference ${reference}.`);
        }
        target = new URL(logical.pathname.slice(identifierBase.pathname.length), new URL('./', root));
      }
      await visit(target);
    }
  }
  await visit(root);
}

export async function loadModelSchema(schemaUrl, applicationId) {
  if (applicationId !== 'fem') throw new Error('The registered application ID must be fem.');
  await checkSchemaBoundary(schemaUrl);
  const sources = await loadSchemaDocuments(new URL(schemaUrl, workspaceUrl), workspaceUrl);
  const applicationBase = new URL('applications/fem/', workspaceUrl);
  const platformBase = new URL('platform/', workspaceUrl);
  if (!sources[0]?.url.href.startsWith(applicationBase.href)
    || sources.some(source => !source.url.href.startsWith(applicationBase.href)
      && !source.url.href.startsWith(platformBase.href))) {
    throw new Error('FEM schemas and imports must stay inside the FEM application or shared platform.');
  }
  const schema = sources[0]?.document;
  if (schema?.properties?.application?.const !== applicationId) {
    throw new Error(`The selected schema does not declare application "${applicationId}".`);
  }
  const documents = [...sources.slice(1).map(source => source.document), ...embeddedScenarioDocuments(schema)];
  configureModelSchema(schema, documents);
  active = { schema, sources, documents };
  return active;
}

export async function activateModelSchema(model, modelLocation) {
  if (model?.application !== 'fem') throw new Error('The model does not declare application "fem".');
  const reference = model['x-fullmeta-schema'];
  if (typeof reference !== 'string' || !reference || reference.includes('\\')
    || /[:?#]/.test(reference) || reference.startsWith('/') || !reference.endsWith('.json')) {
    throw new Error('The model must specify x-fullmeta-schema as a relative JSON schema path.');
  }
  return loadModelSchema(new URL(reference, modelLocation), model.application);
}

export function activeModelSchema() {
  if (!active) throw new Error('Load the FEM model schema before creating its package.');
  return active;
}
