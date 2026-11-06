const $ = s => document.querySelector(s);
const esc = s => String(s).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
let request = 0;
const fraction = q => q ? (q.denominator === '1' ? esc(q.numerator) : `${esc(q.numerator)} / ${esc(q.denominator)}`) : '—';
const mark = m => m.value ? fraction(m.value) : `<strong>Unpriced</strong><br><small>${m.reasons.map(esc).join(', ')}${m.truncated ? ' · достигнут лимит' : ''}<br>Оценённая часть: ${fraction(m.priced_part)}</small>`;
const kinds = {Wallet:'Кошелёк',CpmmLP:'Доля CPMM',StableLP:'Доля StableSwap',ClmmClaim:'CLMM: principal + fees',BookEscrow:'Escrow заявки',DepositClaim:'Депозитное право',Collateral:'Залог',Debt:'Долг'};
function provenance(price) {
 if (!price) return 'Цена не требуется для нуля';
 const r = price.record;
 return `${esc(price.source_id ?? 'Источник отсутствует')}${r ? `<br>${esc(r.price.numerator)} / ${esc(r.price.denominator)} · seq ${esc(r.sequence)}<br>Наблюдение ${esc(r.observed_at)} s · доступность ${esc(r.available_at)} s<br>Возраст ${esc(price.age_seconds ?? '—')} s` : ''}`;
}
export async function renderValuation(run, state, api) {
 const ticket = ++request, panel = $('#valuation-panel'), policy = run.scenario.valuation_policy;
 panel.hidden = !policy;
 if (!policy) return;
 const selected = $('#valuation-portfolio').value;
 $('#valuation-body').textContent = 'Вычисляется оценка сохранённого состояния…';
 $('#valuation-download').disabled = true;
 try {
  const report = await api('valuation/' + state.block);
  if (ticket !== request) return;
  $('#valuation-portfolio').innerHTML = report.portfolios.map(p => `<option value="${esc(p.portfolio)}">${esc(p.portfolio)}</option>`).join('');
  if (report.portfolios.some(p => p.portfolio === selected)) $('#valuation-portfolio').value = selected;
  function draw() {
   const p = report.portfolios.find(p => p.portfolio === $('#valuation-portfolio').value), unit = esc(report.numeraire);
   $('#valuation-body').innerHTML = `<p>Блок <strong>${esc(report.block)}</strong> · ${esc(report.timestamp)} s · счета: ${p.accounts.map(esc).join(', ')}</p>${p.overlaps.length ? `<p class="notice">Пересечение со счетами портфелей ${p.overlaps.map(esc).join(', ')}. Эти оценки нельзя складывать.</p>` : ''}<div class="stats"><div class="stat"><span>Стоимость, atoms ${unit}</span><strong id="valuation-value">${mark(p.current.mark)}</strong></div><div class="stat"><span>PnL после внешнего капитала, atoms ${unit}</span><strong id="valuation-pnl">${mark(p.pnl)}</strong></div></div><p>Начальная стоимость: ${mark(p.initial)} ${unit}<br>Внешние взносы: ${mark(p.inflows)} ${unit}<br>Вывод внешнего капитала: ${mark(p.outflows)} ${unit}</p><p class="hint">PnL = текущая стоимость − начальная стоимость − взносы + вывод. Значения показаны точными дробями в atoms; округлённые строки не суммируются.</p><h3>Состав портфеля</h3><p class="hint">LP — доля резервов; CLMM — признанные principal и collectable fees, без rounding surplus. Депозитное право учитывает убытки. Оценка позиции не обещает её немедленный вывод.</p><table id="valuation-components"><thead><tr><th>Позиция</th><th>Количество</th><th>Стоимость, ${unit}</th></tr></thead><tbody>${p.current.components.map(c => `<tr><td>${esc(c.key.account)}<br>${esc(kinds[c.key.kind] ?? c.key.kind)} ${esc(c.key.protocol)} ${esc(c.key.position)}</td><td>${fraction(c.quantity)} ${esc(c.key.asset)}</td><td>${mark(c.mark)}${c.deposit_cash ? `<br><small>Cash ${esc(c.deposit_cash.reserve_cash)} · cash bound ${esc(c.deposit_cash.cash_bound)} ${esc(c.key.asset)}<br>Это предел по cash, не обещание вывода.</small>` : ''}</td></tr>`).join('')}</tbody></table><details><summary>Цены и причины оценки · ${p.current.component_checks} проверок</summary><p>Reference source: ${esc(policy.source)} · max age ${esc(policy.max_age_seconds)} s. Эти цены не используются агентами и исполнителем.</p>${p.current.components.filter(c => c.price).map(c => `<p><strong>${esc(c.key.asset)} · ${esc(c.key.account)} · ${esc(c.key.kind)}</strong><br>${provenance(c.price)}<br>${mark(c.price.mark)}</p>`).join('')}</details><h3>Внешний капитал в этом блоке</h3>${report.capital_flows.filter(f => f.portfolio === p.portfolio).map(f => `<article><code>${esc(f.tx_id)}</code><p>${f.direction === 'Inflow' ? 'Взнос' : 'Вывод'}: ${esc(f.amount)} ${esc(f.asset)} · ${mark(f.mark)} ${unit}<br>Исполнен в ${esc(f.timestamp)} s<br>${provenance(f.price)}</p></article>`).join('') || '<p>Исполненных помеченных переводов нет.</p>'}<details><summary>Остальные переводы через границу</summary><p class="hint">Переводы без метки внешнего капитала входят в PnL. Протокольные операции, комиссии и займы не становятся взносами автоматически.</p>${report.boundary_transfers.filter(n => n.portfolio === p.portfolio && !n.capital_label).map(n => `<p><code>${esc(n.tx_id)}</code><br>${esc(n.event.from)} → ${esc(n.event.to)} · ${esc(n.event.amount)} ${esc(n.event.asset)} · без capital label</p>`).join('') || '<p>Нет переводов без метки.</p>'}</details>`;
  }
  $('#valuation-portfolio').onchange = draw;
  draw();
  $('#valuation-download').disabled = false;
  $('#valuation-download').onclick = () => {
   const url = URL.createObjectURL(new Blob([JSON.stringify({policy, boundary:report}, null, 2)], {type:'application/json'}));
   const a = document.createElement('a'); a.href = url; a.download = `valuation-${report.block}.json`; a.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
 } catch (e) {
  if (ticket === request) $('#valuation-body').textContent = `Оценка недоступна: ${e.message}`;
 }
}
