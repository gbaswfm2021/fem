// Generated from widget.jsx by scripts/build-frontend.mjs.
import React from "../../../../../platform/ui/widgets/shared/react/react-runtime.js";
import { mountReactComponent } from "../../../../../platform/ui/widgets/shared/react/mount.js";
function Widget(props) {
  const { label = "Quantity", value, path = [], ui = {}, context = {}, onCommand, disabled = false } = { ...props, ...props.view };
  const records = context.choiceRecords?.(ui.choiceSource) || [];
  const identity = (quantity) => JSON.stringify([quantity?.type, quantity?.component, quantity?.part]);
  const match = records.find((record) => identity(record.value) === identity(value));
  const selected = JSON.stringify(match?.value ?? value), unavailable = !match;
  return /* @__PURE__ */ React.createElement("tr", null, /* @__PURE__ */ React.createElement("td", null, label), /* @__PURE__ */ React.createElement("td", null, /* @__PURE__ */ React.createElement("select", { "aria-label": label, value: selected, disabled, onChange: (event) => onCommand({ type: "field.change", path, value: event.target.value, rawValue: event.target.value, valueType: "json-value" }, event) }, unavailable ? /* @__PURE__ */ React.createElement("option", { value: selected, disabled: true }, value?.component || "Choose quantity", " \xB7 unavailable") : null, records.map((record) => /* @__PURE__ */ React.createElement("option", { key: JSON.stringify(record.value), value: JSON.stringify(record.value), disabled: record.disabled, title: record.reason || "" }, record.label, record.disabled ? " \xB7 no saved samples" : "")))));
}
const mountWidget = (container, props) => mountReactComponent(container, Widget, props);
export {
  Widget,
  mountWidget
};

//# sourceMappingURL=widget.js.map
