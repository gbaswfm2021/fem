// Generated from panels.jsx by scripts/build-frontend.mjs.
import React from "../../../platform/ui/widgets/shared/react/react-runtime.js";
import { mountReactComponent } from "../../../platform/ui/widgets/shared/react/mount.js";
const summaries = /* @__PURE__ */ new WeakMap();
const histories = /* @__PURE__ */ new WeakMap();
function StudyHistory({ view }) {
  if (view.loading) return /* @__PURE__ */ React.createElement("p", { role: "status" }, "Reading the selected Study's saved run\u2026");
  if (view.loadError) return /* @__PURE__ */ React.createElement("p", { role: "alert" }, view.loadError);
  return /* @__PURE__ */ React.createElement(React.Fragment, null, view.active ? /* @__PURE__ */ React.createElement("p", null, "Observed run ", view.active.runId, " \xB7 worker ", view.active.liveness) : null, view.empty ? /* @__PURE__ */ React.createElement("p", null, view.message) : /* @__PURE__ */ React.createElement("dl", null, /* @__PURE__ */ React.createElement("dt", null, "Run"), /* @__PURE__ */ React.createElement("dd", null, view.runId), /* @__PURE__ */ React.createElement("dt", null, "State"), /* @__PURE__ */ React.createElement("dd", null, view.state), /* @__PURE__ */ React.createElement("dt", null, "Started"), /* @__PURE__ */ React.createElement("dd", null, view.startedAt), /* @__PURE__ */ React.createElement("dt", null, "Finished"), /* @__PURE__ */ React.createElement("dd", null, view.completedAt), /* @__PURE__ */ React.createElement("dt", null, view.durationLabel), /* @__PURE__ */ React.createElement("dd", null, view.durationSeconds === null ? "Unavailable" : `${view.durationSeconds} s`), view.error ? /* @__PURE__ */ React.createElement(React.Fragment, null, /* @__PURE__ */ React.createElement("dt", null, "Diagnostic"), /* @__PURE__ */ React.createElement("dd", null, view.error)) : null));
}
function renderStudyHistory(host, view) {
  if (!host) return;
  const mount = histories.get(host);
  if (mount?.inspect().mounted) mount.update({ view });
  else histories.set(host, mountReactComponent(host, StudyHistory, { view }));
}
function FieldStatus({ context }) {
  const state = context.fieldPlotState, selected = context.selectedPath && context.valueAt(context.selectedPath), ready = state?.actionId === selected?.id && state.isCurrent;
  return /* @__PURE__ */ React.createElement("section", { "aria-label": "Field source" }, /* @__PURE__ */ React.createElement("p", { role: "status" }, ready ? `${state.resolved.studyName || state.data.studyId} \xB7 run ${state.data.runId} \xB7 ${state.data.stationary ? "Stationary" : context.fieldPointLabel(state)} \xB7 ${state.data.nodes.length} nodes \xB7 ${state.data.cells.length} elements` : context.fieldPlotLoadStatus?.message || "Select a saved field result to inspect its source."));
}
function renderResultSettings(context) {
  const host = context.$('[data-model-editor-extension="resultSettings"]');
  if (!host) return;
  const action = Array.isArray(context.selectedPath) && context.isResultPath(context.selectedPath) ? context.valueAt(context.selectedPath) : null;
  host.hidden = !context.isFieldResultAction(action);
  if (host.hidden) return;
  const existing = summaries.get(host);
  if (existing?.inspect().mounted) existing.update({ context });
  else summaries.set(host, mountReactComponent(host, FieldStatus, { context }));
}
function ClearResultsDialog() {
  return /* @__PURE__ */ React.createElement("dialog", { "data-model-editor-extension": "clearResults", "aria-labelledby": "clear-results-title" }, /* @__PURE__ */ React.createElement("h2", { id: "clear-results-title" }, "Clear results"), /* @__PURE__ */ React.createElement("p", { id: "clear-results-description" }), /* @__PURE__ */ React.createElement("p", { id: "clear-results-path" }), /* @__PURE__ */ React.createElement("button", { type: "button", id: "cancel-clear-results", onClick: (event) => event.currentTarget.closest("dialog").close("cancel") }, "Cancel"), /* @__PURE__ */ React.createElement("button", { type: "button", id: "confirm-clear-results", onClick: (event) => event.currentTarget.closest("dialog").close("clear") }, "Clear results"));
}
function mountEditorPanels({ settingsView }) {
  const host = settingsView.ownerDocument.createElement("div");
  host.dataset.applicationPanel = "fem";
  settingsView.append(host);
  const dialogHost = host.ownerDocument.createElement("div"), fieldHost = host.ownerDocument.createElement("div");
  fieldHost.dataset.modelEditorExtension = "resultSettings";
  fieldHost.hidden = true;
  host.append(dialogHost, fieldHost);
  const history = host.ownerDocument.createElement("section");
  history.id = "fem-study-history";
  history.dataset.modelEditorPanel = "studyHistory";
  history.hidden = true;
  const heading = host.ownerDocument.createElement("h3");
  heading.dataset.modelEditorPanelHeading = "";
  heading.textContent = "Last finished run";
  history.append(heading);
  const historyBody = host.ownerDocument.createElement("div");
  historyBody.dataset.femStudyHistory = "";
  history.append(historyBody);
  host.append(history);
  const mount = mountReactComponent(dialogHost, ClearResultsDialog, {});
  return () => {
    summaries.get(fieldHost)?.dispose();
    summaries.delete(fieldHost);
    histories.get(historyBody)?.dispose();
    histories.delete(historyBody);
    mount.dispose();
    host.remove();
  };
}
export {
  mountEditorPanels,
  renderResultSettings,
  renderStudyHistory
};

//# sourceMappingURL=panels.js.map
