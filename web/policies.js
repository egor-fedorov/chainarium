import {routeCard} from './routes.js';
const $=s=>document.querySelector(s);
const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function target(t) {return t.type==='Exit'?'Вывести позиции':`Диапазон [${t.lower}, ${t.upper}) · L ${esc(t.liquidity)} · max X/Q ${esc(t.max_x)} / ${esc(t.max_q)}`;}
export function renderPolicies(run) {
 const lp=Object.entries(run.scenario.lp_policies??{}),mm=Object.entries(run.scenario.mm_policies??{});
 $('#policy-panel').hidden=!(lp.length+mm.length);
 $('#policy-config').innerHTML=lp.map(([id,p])=>`<article class="policy-config"><h3>${esc(id)} · LP ${esc(p.pool)}</h3><p>Счёт ${esc(p.account)} · ресурс ${esc(p.resource_limit)} × ${esc(p.resource_price)}</p><ol>${p.schedule.map(e=>`<li>Блок ${esc(e.block)}: ${target(e.target)}</li>`).join('')}</ol><p class="hint">Между событиями — Hold. Перестановка собирает fees и выводит собственные позиции, затем создаёт новую. Неуспешное событие пропускается.</p></article>`).join('')+mm.map(([id,p])=>`<article class="policy-config"><h3>${esc(id)} · Market maker ${esc(p.book)}</h3><p>Счёт ${esc(p.account)} · ${esc(p.lots)} lots на сторону · отступ ${esc(p.spread_ticks)} ticks · обновление каждые ${esc(p.refresh_every)} блоков</p><p>${p.anchor.type==='Fixed'?`Фиксированный anchor: ${esc(p.anchor.ticks)} ticks`:`Публичный oracle ${esc(p.anchor.feed)} · max age ${esc(p.anchor.max_age)} s`}</p><p class="hint">Старые заявки отменяются перед заменой; нужны средства на обе стороны. При устаревшей цене — только отмена. GTC-заявка может исполниться как taker и заплатить комиссию.</p></article>`).join('');
}
export function decisionCard(d,run,block) {
 const tx=d.transaction,receipt=tx?block.receipts.find(r=>r.tx_id===tx.id):null,rejection=tx?block.rejected.find(r=>r.tx_id===tx.id):null;
 let html=`<div class="decision"><strong>${esc(d.agent)} · ${esc(d.action??(tx?'Submit':'Skip'))}</strong><p>Наблюдал блок ${esc(d.observed_block)} · ${esc(d.reason)}</p>`;
 if(tx)html+=`<p>Кандидат ${esc(tx.id)} · ${receipt?esc(receipt.status):rejection?'Rejected: '+esc(rejection.reason):'ожидает исполнения'}</p>`;
 if(d.expected_net_gain!==null)html+=`<p>Ожидание после комиссии: +${esc(d.expected_net_gain)} atoms ${esc(run.scenario.chain.fee_asset)}</p>`;
 if(d.details?.target)html+=`<p>${target(d.details.target)}</p>`;
 const a=d.details?.anchor;
 if(a)html+=`<p>Anchor: ${a.ticks===null?'недоступен':esc(a.ticks)+' ticks'}${a.feed?'<br>Oracle '+esc(a.feed):''}${a.record?` · цена ${esc(a.record.price.numerator)}/${esc(a.record.price.denominator)} ${esc(a.record.price.quote_asset)}/${esc(a.record.price.base_asset)}<br>Observed at ${esc(a.record.observed_at)} · возраст ${esc(a.age)} / ${esc(a.max_age)} s`:''}</p>`;
 if(d.route)html+=d.route.hops.map(h=>`${esc(h.venue)}: ${esc(h.input)} ${esc(h.asset_in)} → ${esc(h.output)} ${esc(h.asset_out)}`).join('<br>');
 if(d.executed_route)html+='<p class="hint">Результат пробного исполнения на наблюдаемом состоянии:</p>'+routeCard(d.executed_route);
 if(d.search_truncated)html+='<p>Поиск ограничен бюджетом</p>';
 return html+'</div>';
}
