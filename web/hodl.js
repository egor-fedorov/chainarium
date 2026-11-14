const $ = s => document.querySelector(s);
const esc = s => String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fraction = q => q ? (q.denominator === '1' ? esc(q.numerator) : `${esc(q.numerator)} / ${esc(q.denominator)}`) : '—';
const mark = m => m.value ? fraction(m.value) : `<strong>Unpriced</strong><br><small>${m.reasons.map(esc).join(', ')}${m.truncated ? ' · достигнут лимит' : ''}</small>`;
const point = p => p.type === 'Boundary' ? `граница блока ${esc(p.block)} · ${esc(p.timestamp)} s` : `блок ${esc(p.cursor.block)} · receipt ${esc(p.cursor.receipt_index)} · event ${esc(p.cursor.event_index)} · ${esc(p.cursor.tx_id)}`;
const quantities = h => h ? `<p>${point(h.at)}</p><table><thead><tr><th>Актив</th><th>Количество</th></tr></thead><tbody>${Object.entries(h.quantities).map(([a,q]) => `<tr><td>${esc(a)}</td><td>${fraction(q)}</td></tr>`).join('')}</tbody></table>` : '<p>Начальная корзина недоступна.</p>';
let request = 0;
export async function renderHodl(run, state, api) {
 const ticket = ++request, policy = run.scenario.hodl_policy;
 $('#hodl-panel').hidden = !policy;
 if (!policy) return;
 const selected = $('#hodl-portfolio').value;
 $('#hodl-body').textContent = 'Восстанавливается пассивная корзина…';
 $('#hodl-download').disabled = true;
 try {
  const report = await api('hodl/' + state.block);
  if (ticket !== request) return;
  // A different run may have been installed while this request was in flight.
  if (report.provenance.run_id !== run.run_id) throw Error('Запуск изменился; выберите блок повторно.');
  $('#hodl-portfolio').innerHTML = report.portfolios.map(p => `<option value="${esc(p.portfolio)}">${esc(p.portfolio)}</option>`).join('');
  if (report.portfolios.some(p => p.portfolio === selected)) $('#hodl-portfolio').value = selected;
  function draw() {
   const p = report.portfolios.find(p => p.portfolio === $('#hodl-portfolio').value), l = p.ledger, unit = esc(report.provenance.numeraire);
   const stats = [['Фактическая стоимость','actual',p.actual],['Стоимость HODL','value',p.hodl.mark],['Фактический PnL','actual-pnl',p.actual_pnl],['PnL HODL','pnl',p.hodl_pnl],['Excess PnL: факт − HODL','excess',p.excess_pnl],['Разница текущих стоимостей (не PnL)','gap',p.value_gap]];
   $('#hodl-body').innerHTML = `<p>Блок <strong>${esc(report.block)}</strong> · ${esc(report.timestamp)} s · счета: ${p.accounts.map(esc).join(', ')}</p>
    ${p.overlaps.length ? `<p class="notice">Пересечение с ${p.overlaps.map(esc).join(', ')}. Эти оценки нельзя складывать.</p>` : ''}
    <p id="hodl-status"><strong>${esc(l.status)}</strong>${l.status === 'Unavailable' ? ` · ${esc(l.failure.reason)}<br>${point(l.failure.at)}${l.failure.asset ? `<br>${esc(l.failure.asset)}: запрошено ${fraction(l.failure.requested)}, доступно ${fraction(l.failure.available)}` : ''}<br>Benchmark остановлен. Поздние взносы его не восстанавливают.` : ''}</p>
    <div class="stats">${stats.map(([name,id,m]) => `<div class="stat"><span>${name}, atoms ${unit}</span><strong id="hodl-${id}">${mark(m)}</strong></div>`).join('')}</div>
    <p class="hint">HODL хранит начальные количества underlying и повторяет только исполненные помеченные взносы и выводы. Нет сделок, процентов и собственных комиссий. Фактические комиссии уже входят в MTM. Excess PnL доступен, только когда оба PnL оценены; разница текущих стоимостей не заменяет неизвестную историю.</p>
    <p>Начальная стоимость: ${mark(p.initial)} ${unit}<br>Взносы: ${mark(p.inflows)} ${unit}<br>Вывод: ${mark(p.outflows)} ${unit}</p>
    <h3>${l.status === 'Ready' ? 'Текущая пассивная корзина' : 'Последняя допустимая корзина · только история'}</h3>
    <div id="hodl-holdings">${quantities(l.status === 'Ready' ? l.current : l.last_feasible)}</div>
    <details><summary>Начальная корзина и состав</summary>${quantities(p.initial_holdings)}<table><thead><tr><th>Позиция</th><th>Количество</th></tr></thead><tbody>${p.initial_components.map(c => `<tr><td>${esc(c.key.account)} · ${esc(c.key.kind)} · ${esc(c.key.protocol)} · ${esc(c.key.position)}</td><td>${fraction(c.quantity)} ${esc(c.key.asset)}</td></tr>`).join('')}</tbody></table></details>
    <h3>Внешний капитал в этом блоке</h3><p class="hint">Порядок исполнения receipt/event, индексы с нуля. Всего помеченных событий: ${esc(p.flow_events)}.</p>
    <div id="hodl-flows">${p.flows.map(f => `<article><code>${esc(f.cursor.tx_id)}</code><p>receipt ${esc(f.cursor.receipt_index)} · event ${esc(f.cursor.event_index)}<br>${f.flow.direction === 'Inflow' ? 'Взнос' : 'Вывод'} ${esc(f.flow.amount)} ${esc(f.flow.asset)} · ${esc(f.effect.status)}<br>${f.effect.status === 'Applied' ? `${fraction(f.effect.before)} → ${fraction(f.effect.after)}` : esc(f.effect.reason)}</p></article>`).join('') || '<p>Исполненных помеченных переводов нет.</p>'}</div>
    <details><summary>Цены и происхождение расчёта</summary>${Object.entries(p.hodl.assets).map(([a,v]) => `<p><strong>${esc(a)}</strong> · ${mark(v.mark)} ${unit}<br>${esc(v.price?.source_id ?? '—')}${v.price?.record ? `<br>Цена ${esc(v.price.record.price.numerator)} / ${esc(v.price.record.price.denominator)} · seq ${esc(v.price.record.sequence)}<br>Наблюдение ${esc(v.price.record.observed_at)} s · доступность ${esc(v.price.record.available_at)} s · возраст ${esc(v.price.age_seconds ?? '—')} s` : ''}</p>`).join('')}<dl>${Object.entries(report.provenance).map(([k,v]) => `<dt>${esc(k)}</dt><dd><code>${esc(v)}</code></dd>`).join('')}</dl></details>`;
  }
  $('#hodl-portfolio').onchange = draw;
  draw();
  $('#hodl-download').disabled = false;
  $('#hodl-download').onclick = () => {
   const url = URL.createObjectURL(new Blob([JSON.stringify({policy, valuation_policy:run.scenario.valuation_policy, boundary:report}, null, 2)], {type:'application/json'}));
   const a = document.createElement('a'); a.href = url; a.download = `hodl-${report.block}.json`; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
 } catch (e) {
  if (ticket === request) $('#hodl-body').textContent = `HODL недоступен: ${e.message}`;
 }
}
