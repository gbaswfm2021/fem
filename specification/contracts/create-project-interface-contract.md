# Launcher project lifecycle interface contract

This document describes the interfaces needed for the application-card **Create project** and **Delete project** actions on the FullMeta launcher. It focuses on the boundary between the application schema, catalog registration, schema loader, application package, and the generic Kernel endpoints.

The launcher, Schema Editor, and Model Editor each use their own project or model creation interface. Both project creation actions and the model creation interface are defined below.

## Launcher button visibility and successful creation

The launcher renders **Create project** when the application package loads, `applicationPackage.launcher.profiles` contains at least one available profile, and `applicationPackage.createModelDocument` is a function. Those are the button-visibility conditions.

For the click to create and open a project successfully, the selected schema must also declare the project-create API route and storage locations required by the generic Kernel handler, and `createModelDocument` must return a model matching the schema and Kernel identity checks.

The button condition comes from the launcher. The schema does not independently turn the button on. A schema-only catalog registration does not satisfy this target because it supplies no application package containing profiles and a model factory.

## Schema Editor and launcher creation

Exercise both entry points with target application registered as a runtime application. The Schema Editor's **Create project from this schema** action uses the registered target application package's `createModelDocument` and `editorApi.projectCreate`. Choose an available profile and a fresh project ID, then verify that the editor opens the file returned by the Kernel and that its persisted starter passes structural validation. Exercise the launcher **Create project** action for each of the five profiles through the same factory and route.

The generic initializer for schemas without a runtime package is a shared
Schema Editor capability, covered by its own acceptance. FEM uses its
registered profile factory. FEM acceptance checks that both entry points
create the correct FEM starter; it does not create an unrelated schema to
test the generic fallback.

Schema declarations alone cannot provide the launcher's profile list or `createModelDocument` factory. This target application scope includes application-owned package JavaScript for those interfaces; it does not require modifying shared schema-editor code or adding a custom Rust project-creation endpoint.

| File | Needed for app-card Create project? | Required interface |
| --- | --- | --- |
| `applications/{APP_ID}/catalog.json` | Yes | Register ID `{APP_ID}`, target application schema path, `runtimePackage: true`, and paths to the loader and package modules. |
| `applications/{APP_ID}/{SCHEMA_FILE}` | Yes | Declare project structure, defaults, `fullmeta-model-runtime/1` storage and `editorApi.projectCreate` / `editorApi.projectDelete` contracts through Model Editor. |
| `applications/{APP_ID}/schema-documents.js` | Yes | Export `loadModelSchema(schemaUrl, applicationId)`; also support `activateModelSchema(model, modelLocation)` if existing-model activation uses that route. |
| `applications/{APP_ID}/package.js` | Yes | Export `createApplicationPackage(options)` and return the validated application package, `launcher.profiles`, and `createModelDocument`. |
| `applications/{APP_ID}/runtime-build.json` | No for project creation | Add when native programs must be built or packaged reproducibly. |
| application-owned UI/result modules | No for an empty project button; yes for the rest of the runtime UI | Supply only modules referenced by the package and schema, such as action adapters, graphics renderers, or result adapters. |

Implement the application-owned package and loader interfaces below so the launcher-card action is available. The builder owns these interfaces as well as their schema declarations.

## Interface ownership

| Concern | Owner | Required interface |
| --- | --- | --- |
| Application identity and module locations | `applications/{APP_ID}/catalog.json` | Stable application ID, schema path, runtime-package flag, schema-loader path, package path |
| Project data contract | Application model schema | Application constant, model format, metadata and model declarations, runtime storage and project-create API contract |
| Available project types | Application package JavaScript | `launcher.profiles` plus profile-field mapping and default profile |
| Initial model document | Application package JavaScript | Synchronous `createModelDocument({ modelId, profileId, folder })` returning a JSON object |
| Folder and file creation | Shared Rust Kernel | Schema-registered `kernel.workspace.project.create` endpoint |
| Project folder deletion | Shared launcher and Rust Kernel | Launcher confirmation plus schema-registered `kernel.workspace.project.delete` endpoint |
| Post-create open | Shared launcher | Reads returned `folder` and `file`, then opens the saved document |

For the currently selected application folder, application-owned integration paths must remain inside `applications/{APP_ID}/`. Use the catalog's registered application ID and schema filename; do not infer them from this document's title.

## Catalog registration

The launcher needs a runtime package so it can load the package's `launcher` metadata and `createModelDocument` function. The registration shape is:

```json
{
  "id": "{APP_ID}",
  "name": "{APP_DISPLAY_NAME}",
  "description": "<short description>",
  "schema": "applications/{APP_ID}/{SCHEMA_FILE}",
  "runtimePackage": true,
  "schemaLoader": "applications/{APP_ID}/schema-documents.js",
  "package": "applications/{APP_ID}/package.js"
}
```

The `schemaLoader` and `package` fields may use the catalog's conventional defaults when omitted for a runtime package. Explicit paths make the interface easier to audit. The catalog validator requires the schema and JavaScript module paths to stay inside the registered application's folder. A `runtimeBuild` registration is not needed just to create a project; it is needed separately when native programs must be built or deployed.

## Application schema contract

The selected model schema is the source of the route and storage contract used by the generic Kernel. The schema must have the same application ID as the catalog entry and package.

### Root project fields

The initial model returned by JavaScript must satisfy the full project schema. At minimum, the generic project-create handler checks:

- `modelFormat` is a non-empty string;
- `metadata` is an object with an `id` that matches `modelId` and uses the Kernel's valid project identifier form;
- `application` matches the schema's `properties.application.const`, when declared.

The schema and created document should also declare and populate `x-fullmeta-schema` with the correct relative schema path from the model JSON location. It must be a string field whose value can differ by project location; it cannot be fixed to one schema-relative path with a `const`.

Every other required model field must be initialized by `createModelDocument` or be expressible through schema defaults/constants/enums. The returned model should already be schema-valid so the launcher can immediately open it in the editor. Profile-specific fields, nested model defaults, and the selected profile identifier belong in the returned model wherever the application schema declares them.

### Runtime storage and route

Declare the `fullmeta-model-runtime/1` contract. Its `storage` object must provide `projectRootFolder`, `modelFile`, `runsDirectory`, and `resultsDirectory`. The Kernel uses these values to create projects, choose a non-colliding model filename, and limit project deletion to the declared project root.

The launcher looks up the exact `editorApi.projectCreate` key. Use a structured route contract so the Kernel can dispatch it:

```json
{
  "x-fullmeta-runtime": {
    "schema": "fullmeta-model-runtime/1",
    "storage": {
      "projectRootFolder": "projects",
      "modelFile": "fem_model.json",
      "runsDirectory": "fem_runs",
      "resultsDirectory": "fem_results"
    },
    "editorApi": {
      "projectCreate": {
        "route": "/api/project/create",
        "method": "POST",
        "handler": "kernel.workspace.project.create",
        "input": {
          "contentType": "application/json",
          "fields": [
            ["profileId", "string"],
            ["projectId", "string"],
            ["model", "object"]
          ]
        },
        "output": {
          "contentType": "application/json",
          "fields": [
            ["folder", "string"],
            ["file", "string"]
          ]
        }
      },
      "projectDelete": {
        "route": "/api/project/delete",
        "method": "POST",
        "handler": "kernel.workspace.project.delete",
        "input": {
          "contentType": "application/json",
          "fields": [["id", "string"]]
        },
        "output": {
          "contentType": "application/json",
          "fields": [
            ["id", "string"],
            ["folder", "string"],
            ["deleted", "boolean"]
          ]
        }
      }
    }
  }
}
```

Use one unique route for this handler. The route registration is schema-owned; the shared Kernel implements the operation. Do not add an application-specific HTTP endpoint just to make a directory and write the initial model.

### Launcher project deletion

The launcher shows **Delete project** for a project card when its registered application package loads and its schema declares `x-fullmeta-runtime.editorApi.projectDelete`. The package does not need a custom deletion callback. The launcher reads the route from the selected application's schema, confirms the exact project folder, and sends the registered application/schema context with the JSON request:

```http
POST /api/project/delete
content-type: application/json
x-fullmeta-application: {APP_ID}
x-fullmeta-model-schema: applications%2F{APP_ID}%2F{SCHEMA_FILE}

{"id":"<project-id>"}
```

The Kernel resolves `id` only as a direct child of `storage.projectRootFolder`, verifies that the folder contains a FullMeta project document, rejects unsafe paths or a project with an active run lock, and returns `{ "id": "<project-id>", "folder": "projects/<project-id>", "deleted": true }` after the folder is removed. The schema declares the request/response contract; the generic Kernel owns path safety and deletion.

Deletion is permanent. Before sending the request, the launcher must name the project and full folder in a confirmation dialog and explain that the model and generated project files will be removed. Cancel leaves the project untouched. On success, refresh the project list and report the deleted folder. On failure, keep the card visible, re-enable the button, and show the exact Kernel error. If deletion succeeds but refreshing the list fails, report both facts accurately.

## Schema-loader JavaScript interface

The catalog loader calls the application's schema loader before constructing the application package. For launcher startup, it calls:

```js
await schemaLoader.loadModelSchema(schemaUrl, applicationId);
```

The loader must expose `loadModelSchema(schemaUrl, applicationId)` and make the selected schema graph available to the package module. It should use the shared schema-document loading facilities, resolve local schema references, and reject a schema whose declared application constant does not match `applicationId`.

When an existing project is opened, the package loader may call `activateModelSchema(model, modelLocation)` or call `loadModelSchema` with the model's `x-fullmeta-schema` URL. Both paths must resolve the same application-owned schema graph.

## Application-package JavaScript interfaces

These loader, factory and launcher signatures remain ordinary compatible JavaScript interfaces. New application-owned visual components invoked by the package use the [Frontend provider contract](../../../../platform/docs/frontend-provider-contract.md): JSX source under the application's `ui/`, compiled public `.js` imports, one shared React runtime and explicit mount/update/disposal. Frontend compilation is separate from native `runtimeBuild` registration. Load compiled public JavaScript in the browser and use the existing checked project-create route.

The package module exports `createApplicationPackage(options)`. It may be asynchronous. It must return a package whose `id`, `manifest.id`, and `modelSchema.properties.application.const` agree with the catalog ID, and whose schema declares `fullmeta-model-runtime/1`.

The application-package loader requires a complete editor package, even if the immediate goal is only to display the launcher button. Its required package members include:

- data/declaration members: `manifest`, `modelSchema`, `schemaDocuments`, `uiProfileSchema`, `modelEditorSchema`, `modelEditorUi`, `graphicsWidgets`, `graphicsConsole`, and `actionBindings`;
- functions: `createWidgetRenderers`, `treeActionPlans`, `resolveResourceUrl`, and `prepareModelForEditor`;
- a manifest that lists valid graphics widgets and a valid default widget;
- a valid `fullmeta-model-editor-ui/1` declaration and any renderer functions required by custom schema widgets.

This broader package contract is checked before the launcher gets a usable package. Missing editor-package members can prevent the entire application card from loading, including its Create project control.

### Launcher metadata

Return `launcher` metadata from `createApplicationPackage`. At least one profile must have an identifier, a display label, and the configured available status. The package may project launcher entries from the loaded scenario profile declarations so the schema remains the source of profile IDs and labels; the package still supplies launcher status and maps each selected ID to its model factory. Default field names are shown below; the property-name overrides allow another profile shape.

```js
launcher: {
  name: "<application display name>",
  description: "<short launcher description>",
  profiles: [
    { id: "acoustic-2d", name: "2D acoustics", description: "2D pressure acoustics", status: "available" },
    { id: "acoustic-3d", name: "3D acoustics", description: "3D pressure acoustics", status: "available" },
    { id: "em-2d", name: "2D electromagnetics", description: "2D Ez formulation", status: "available" },
    { id: "em-3d", name: "3D electromagnetics", description: "3D electric formulation", status: "available" },
    { id: "thermal-2d", name: "2D thermal", description: "Stationary temperature", status: "available" }
  ],
  defaultProfileId: "em-3d",
  profileIdProperty: "id",
  profileLabelProperty: "name",
  profileDescriptionProperty: "description",
  profileStatusProperty: "status",
  availableStatus: "available"
}
```

The defaults are `id`, `name`, `description`, `status`, and `available`. The launcher filters out profiles whose status does not equal `availableStatus`. If `defaultProfileId` does not match an available profile, it selects the first available profile. An absent or empty available profile list means there is no Create project button.

### Required profile IDs and initial model output

The application exposes the following required launcher profile IDs. Each is available only when `package.js` returns it in `launcher.profiles` with status `available`.

| `profileId` | `scenario.dimension` | `scenario.physics` / `model.physics.type` | Initial formulation | `model.mesh.elementFamily` | Wave fields |
| --- | ---: | --- | --- | --- | --- |
| `acoustic-2d` | 2 | `acoustics` | `pressure` | `triangle` | Frequency `1 Hz` |
| `acoustic-3d` | 3 | `acoustics` | `pressure` | `tetrahedron` | Frequency `1 Hz` |
| `em-2d` | 2 | `electromagnetics` | `Ez` initial choice | `triangle` | Frequency `1 Hz` and `phasorConvention: "exp(+i omega t)"` |
| `em-3d` | 3 | `electromagnetics` | `electric` | `tetrahedron` | Frequency `1 Hz` and `phasorConvention: "exp(+i omega t)"` |
| `thermal-2d` | 2 | `thermal` | `T` | `triangle` | Stationary Study |

The exact `createModelDocument` output is one shared blank-project shape with the typed profile values from that table substituted. The application ID and schema path below are configuration captured from the same validated catalog registration used by `createApplicationPackage`; they are not placeholder strings in the returned document.

```js
const profileDefaults = {
  "acoustic-2d": { dimension: 2, physics: "acoustics", formulation: "pressure", elementFamily: "triangle", wave: true },
  "acoustic-3d": { dimension: 3, physics: "acoustics", formulation: "pressure", elementFamily: "tetrahedron", wave: true },
  "em-2d": { dimension: 2, physics: "electromagnetics", formulation: "Ez", elementFamily: "triangle", wave: true },
  "em-3d": { dimension: 3, physics: "electromagnetics", formulation: "electric", elementFamily: "tetrahedron", wave: true },
  "thermal-2d": { dimension: 2, physics: "thermal", formulation: "T", elementFamily: "triangle", wave: false },
};

const APPLICATION_ID = /* configured registered application ID */;
const APPLICATION_SCHEMA_PATH = /* exact catalog schema path */;

function createModelDocument({ modelId, profileId, folder }) {
  if (typeof modelId !== "string" || !/^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(modelId)) {
    throw new Error("Use a project ID starting with a letter, followed by letters, digits, or underscores.");
  }
  if (!Object.hasOwn(profileDefaults, profileId)) {
    throw new Error(`Unsupported target application profile: ${profileId}`);
  }
  const profile = profileDefaults[profileId];
  return {
    application: APPLICATION_ID,
    metadata: { id: modelId, name: modelId },
    modelFormat: "fem-model/2",
    scenario: { dimension: profile.dimension, physics: profile.physics, profileId },
    "x-fullmeta-schema": relativeSchemaReference(folder, APPLICATION_SCHEMA_PATH),
    model: {
      units: "m",
      parameters: [],
      variables: [],
      pointwiseVariables: [],
      integrationOperators: [],
      geometry: { features: [], tolerance: { expression: "1e-9", unit: "m" } },
      materials: [],
      mesh: {
        elementFamily: profile.elementFamily,
        gmsh: {
          algorithm3D: 1, elementOrder: 1, optimize: true, optimizeNetgen: true,
          optimizeThreshold: 0.25, smoothing: 10, threads: 1,
        },
        maxSize: { expression: "0.01", unit: "m" },
        minSize: { expression: "0.001", unit: "m" },
        meshSizeControls: [],
        periodicPairs: [],
      },
      physics: {
        id: "physics",
        name: "Physics",
        type: profile.physics,
        formulation: profile.formulation,
        domainSelectionIds: [],
        ...(profile.wave ? { frequency: { expression: "1", unit: "Hz" } } : {}),
        ...(profile.physics === "electromagnetics" ? { phasorConvention: "exp(+i omega t)" } : {}),
      },
      study: profileId === "thermal-2d"
        ? [{ id: "study", name: "Stationary study", type: "stationary", physicsId: "physics" }]
        : [],
      results: [],
      selections: [],
    },
  };
}
```

Use the required Physics fields for the selected profile. Wave starters supply a frequency quantity and electromagnetic starters supply the phasor convention. The thermal starter supplies the required Stationary Study. [Project JSON contract](../../docs/fullmeta-model-2.md)

The EM3D initializer must specifically set `scenario.dimension` to `3`, `scenario.physics` and `model.physics.type` to `electromagnetics`, `scenario.profileId` to `em-3d`, `model.physics.formulation` to `electric`, and `model.mesh.elementFamily` to `tetrahedron`. A global `triangle` mesh default is invalid for this profile. The same dimension-to-mesh-family selection applies to `acoustic-3d`.

For the standard launcher layout (`projectRootFolder: "projects"`, project folder `projects/<modelId>`, model file `fem_model.json`), the relative schema reference resolves to `../../applications/{APP_ID}/{SCHEMA_FILE}`. The package must calculate the relative path from the actual project folder if the project layout changes. The initializer must generate `metadata.id` and `metadata.name` from `modelId`; fixed sample IDs/names are not valid project-creation defaults.

The starter passes structural validation and opens for editing. The user adds geometry, materials, selections, and result definitions before running the model. Model readiness checks govern completed simulation inputs. Use the synchronous `createModelDocument` factory for profile-based creation from the launcher and the installed Schema Editor action.

### Initial-model factory

The launcher calls `createModelDocument({ modelId, profileId, folder })` synchronously before making the request. Return the complete plain JSON document specified above as the request's `model` value. The folder and schema are canonical workspace-relative paths. To construct the stored reference, remove their common leading path components, prefix one `../` for each remaining folder component, and append the remaining schema components joined with `/`. Resolve the result from the model directory to verify it selects the registered target application schema.

## Launcher-to-Kernel request and response

The launcher submits a JSON `POST` to the route named by `editorApi.projectCreate`. It adds these headers:

- `x-fullmeta-application`: the registered application ID;
- `x-fullmeta-model-schema`: the schema path, from the package's `schemaPath` or the catalog's `schema` value.

The request body is:

```json
{
  "profileId": "<selected-profile-id>",
  "projectId": "<user-entered-project-id>",
  "model": { "...": "document returned by createModelDocument" }
}
```

The generic Kernel validates the application/model identity, creates or reuses the project folder under `projectRootFolder`, selects an available filename based on `modelFile`, writes the model JSON, and responds with at least `folder` and `file`. The launcher opens that returned document. The folder is derived from the declared project root and project ID; the JS factory's `folder` argument is for constructing correct project-relative values such as `x-fullmeta-schema`.

Reject a project ID reserved for generated data and a folder that already contains a target application model. An eligible existing folder can contain a model for another application. Preserve its files, use an available filename when the default is occupied, and open the exact returned folder and filename.

The browser package returns a complete, structurally valid starter for the chosen profile. Check that the opened file has that exact starter shape and profile. Apply completed-model domain rules when validating or preparing it for Study.

## Keep project actions separate

| User action | Main interface | Purpose |
| --- | --- | --- |
| Launcher application-card **Create project** | `launcher.profiles`, package `createModelDocument`, schema `editorApi.projectCreate` | Creates a project folder and its initial model, then opens it |
| Model Editor **Create model** | `modelEditorUi.newModel`, package `createModelDocument`, and the registered model-create API | Adds a model document through the editor's New Model dialog |
| Schema Editor **Create project from this schema** with target application registered | `createSchemaProject`, registered target application `createModelDocument`, and `editorApi.projectCreate` | Creates and opens a profile starter from the currently open schema document |
| Launcher app-card **Delete project** | Schema `editorApi.projectDelete`; generic launcher and Kernel | Confirms and removes only the selected project folder, then refreshes the list |

The launcher action does not require `modelEditorUi.newModel.api`; that declaration serves the editor's separate Create model workflow. Conversely, `modelEditorUi.newModel` alone does not expose the launcher button.

## Launcher project lifecycle acceptance checklist

Exercise the launcher and installed Schema Editor creation actions with the
saved FEM schema and each supported FEM profile.

- [ ] Catalog entry validates and all application module paths stay in the registered app folder.
- [ ] The schema loader resolves the registered schema and verifies the application ID.
- [ ] The package loads with all mandatory members and matching IDs.
- [ ] The launcher card shows at least one available profile and the Create project button.
- [ ] The default profile is selected; changing profile updates the chosen profile passed to JavaScript.
- [ ] `createModelDocument` returns a structurally valid starter for each profile, and the Model Editor opens it for editing. Study readiness checks report the geometry and other inputs still required for a completed model.
- [ ] `editorApi.projectCreate` resolves to `POST /api/project/create` with handler `kernel.workspace.project.create`.
- [ ] `editorApi.projectDelete` resolves to `POST /api/project/delete` with handler `kernel.workspace.project.delete` and a typed `{ id }` request and `{ id, folder, deleted }` response.
- [ ] A disposable target application project card confirms the exact folder; cancel preserves it, success removes only that folder and refreshes the list, and errors leave the card visible with an actionable message.
- [ ] A UI-created project contains the declared model filename, correct `application`, and working relative `x-fullmeta-schema` reference.
- [ ] The launcher opens the created model and the Model Editor loads it without manual JSON repair.
- [ ] Invalid and reserved project IDs, a duplicate target application model, and invalid initializer data produce visible errors. An eligible folder with another application's model preserves that file and opens the returned target application filename.

## Generic implementation references

- [Application catalog loader](../../../../platform/js/application-catalog.js)
- [Application package loader](../../../../platform/js/application-packages.js)
- [Launcher project-creation flow](../../../../launcher.js)
- [Runtime contract schema](../../../../platform/contracts/fullmeta-model-runtime-1.schema.json)
- [Generic schema-project package](../../../../platform/ui/widgets/model-editor/schema-project-package.js)
- [Generic project initializer](../../../../platform/ui/widgets/model-editor/schema-project-initializer.js)
- [Kernel project-create handler](../../../../platform/kernel-server-rust/src/main.rs)

These documents define the shared platform interface contract. They are not proposed edits to shared editor code.


