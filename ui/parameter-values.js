import { getCoreUnit } from '../js/runtime/core-api.js';

// The older shared editor owns the editable rows, but its value column uses
// the platform's unit catalog. Render that column with FEM's unit catalog and
// FEM's existing parameter resolver, without changing the committed inputs.
export function mountParameterValues(settingsTable, resolveParameters) {
  const refresh = () => {
    if (!settingsTable.classList.contains('definition-table')) return;
    const rows = [...settingsTable.querySelectorAll('tbody tr')]
      .filter(row => row.querySelector('.definition-value'));
    if (!rows.length) return;
    const parameters = rows.map(row => Object.fromEntries(
      [...row.querySelectorAll('[data-definition-field]')].map(input =>
        [input.dataset.definitionField, input.dataset.definitionCommittedValue ?? ''])));
    let resolved, error = '';
    try { resolved = resolveParameters(parameters); }
    catch (cause) { error = cause.message; }
    rows.forEach((row, index) => {
      const cell = row.querySelector('.definition-value');
      let value = '—', title = error;
      if (resolved) try {
        const parameter = parameters[index];
        const si = resolved[parameter.symbol]?.value;
        if (Number.isFinite(si)) value = Number(si / getCoreUnit(parameter.unit).scale).toPrecision(6);
      } catch (cause) { title = cause.message; }
      if (cell.textContent !== value) cell.textContent = value;
      if (cell.title !== title) cell.title = title;
    });
  };
  const observer = new MutationObserver(refresh);
  observer.observe(settingsTable, { subtree: true, childList: true, characterData: true,
    attributes: true, attributeFilter: ['class', 'data-definition-committed-value'] });
  refresh();
  return () => observer.disconnect();
}
