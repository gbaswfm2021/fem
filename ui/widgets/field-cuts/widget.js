// Generated from widget.jsx by scripts/build-frontend.mjs.
import React from "../../../../../platform/ui/widgets/shared/react/react-runtime.js";
import { mountReactComponent } from "../../../../../platform/ui/widgets/shared/react/mount.js";
import { Quantity } from "../../../../../platform/ui/widgets/quantity/widget.js";
function Widget(props) {
  const { label = "Cut-plane positions", value = {}, path = [], context = {}, ui = {}, onCommand, disabled = false } = { ...props, ...props.view };
  const records = context.choiceRecords?.(ui.choiceSource) || [];
  if (!records.length) return null;
  return /* @__PURE__ */ React.createElement(React.Fragment, null, /* @__PURE__ */ React.createElement("tr", { className: "settings-group" }, /* @__PURE__ */ React.createElement("th", { colSpan: 2 }, label)), records.map((record) => {
    const axis = record.axis, position = value?.[axis], mode = position?.mode || "midPlane", name = axis.toUpperCase() + " position";
    const coordinate = position?.coordinate || { expression: String(record.midpoint), unit: record.unit };
    return /* @__PURE__ */ React.createElement(React.Fragment, { key: axis }, /* @__PURE__ */ React.createElement("tr", null, /* @__PURE__ */ React.createElement("td", null, name), /* @__PURE__ */ React.createElement("td", null, /* @__PURE__ */ React.createElement("select", { "aria-label": name + " mode", value: mode, disabled, onChange: (event) => {
      const next = event.target.value === "midPlane" ? { mode: "midPlane" } : { mode: "explicitValues", coordinate };
      const serialized = JSON.stringify({ ...value, [axis]: next });
      onCommand({ type: "field.change", path, value: serialized, rawValue: serialized, valueType: "json-value" }, event);
    } }, /* @__PURE__ */ React.createElement("option", { value: "midPlane" }, "Mid-plane"), /* @__PURE__ */ React.createElement("option", { value: "explicitValues", disabled: !record.available }, "Explicit value")))), mode === "explicitValues" ? /* @__PURE__ */ React.createElement(Quantity, { label: name, value: coordinate, path: [...path, axis, "coordinate"], context: { ...context, defaultUnits: ui.units || ["m", "mm", "um", "nm"], unitChoices: () => ui.units || ["m", "mm", "um", "nm"] }, onCommand, disabled }) : null);
  }));
}
const mountWidget = (container, props) => mountReactComponent(container, Widget, props);
export {
  Widget,
  mountWidget
};

//# sourceMappingURL=widget.js.map
