import '../../../../tests/helpers/browser-dom.js';
import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveParameterTable } from '../../js/runtime/core-api.js';
import { mountParameterValues } from '../../ui/parameter-values.js';

const settled = () => new Promise(resolve => setTimeout(resolve, 0));

test('FEM renders parameter values with compound units after edits and table redraws', async () => {
  const table = document.createElement('table');
  table.className = 'definition-table';
  table.innerHTML = `<tbody><tr>
    <td><input data-definition-field="symbol" data-definition-committed-value="q"></td>
    <td><input data-definition-field="expression" data-definition-committed-value="2"></td>
    <td><select data-definition-field="unit" data-definition-committed-value="W/m^3"></select></td>
    <td class="definition-value">—</td></tr></tbody>`;
  document.body.append(table);
  const dispose = mountParameterValues(table, values => resolveParameterTable(values));
  const cell = table.querySelector('.definition-value');
  assert.equal(cell.textContent, '2.00000');
  const expression = table.querySelector('[data-definition-field="expression"]');
  expression.dataset.definitionCommittedValue = '3';
  cell.textContent = '—'; // An older shared editor can overwrite its own cell.
  await settled();
  assert.equal(cell.textContent, '3.00000');
  expression.dataset.definitionCommittedValue = 'invalid_symbol';
  await settled();
  assert.equal(cell.textContent, '—');
  assert.ok(cell.title);
  expression.dataset.definitionCommittedValue = '4';
  await settled();
  assert.equal(cell.textContent, '4.00000');
  table.tBodies[0].innerHTML = `<tr>
    <td><input data-definition-field="symbol" data-definition-committed-value="force"></td>
    <td><input data-definition-field="expression" data-definition-committed-value="5"></td>
    <td><select data-definition-field="unit" data-definition-committed-value="N/m^3"></select></td>
    <td class="definition-value">—</td></tr>`;
  await settled();
  assert.equal(table.querySelector('.definition-value').textContent, '5.00000');
  dispose();
  const replacement = table.querySelector('.definition-value');
  replacement.textContent = '—';
  await settled();
  assert.equal(replacement.textContent, '—');
  table.remove();
});
