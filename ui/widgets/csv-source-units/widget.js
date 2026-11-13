// Generated from widget.jsx by scripts/build-frontend.mjs.
import React from "../../../../../platform/ui/widgets/shared/react/react-runtime.js";
import { mountReactComponent } from "../../../../../platform/ui/widgets/shared/react/mount.js";
function Widget(props) {
  const { label = "CSV source units", value = {}, rawValue, path = [], schema = {}, context = {}, onCommand = context.onCommand, disabled = false } = { ...props, ...props.view };
  const pending = rawValue ?? context.readDraft?.(path);
  const text = pending === void 0 ? JSON.stringify(value, null, 2) : String(pending);
  const columns = schema.type === "array" || Array.isArray(value);
  return /* @__PURE__ */ React.createElement("tr", null, /* @__PURE__ */ React.createElement("td", null, label), /* @__PURE__ */ React.createElement("td", null, /* @__PURE__ */ React.createElement(
    "textarea",
    {
      "aria-label": label,
      "data-setting-path": JSON.stringify(path),
      "data-setting-type": "json-value",
      rows: 4,
      value: text,
      disabled: disabled || schema.readOnly === true,
      onChange: (event) => onCommand?.({ type: "field.change", path, value: event.target.value, rawValue: event.target.value, valueType: "json-value" }, event)
    }
  ), /* @__PURE__ */ React.createElement("div", { className: "settings-empty" }, columns ? "Enter an ordered JSON array of literal CSV column headers." : /* @__PURE__ */ React.createElement(React.Fragment, null, "Map literal CSV column headers to units. Use ", "{}", " to leave columns unmapped. Display units are configured separately."))));
}
const mountWidget = (container, props) => mountReactComponent(container, Widget, props);
export {
  Widget,
  mountWidget
};

//# sourceMappingURL=widget.js.map
