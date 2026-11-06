import {atoms} from './wire.mjs';

const $=s=>document.querySelector(s);
const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
let token=0, activeRun=null;

export function routeCard(r) {
 if(!r)return '';
 const residuals=Object.entries(r.residuals).map(([asset,n])=>`${esc(n)} ${esc(asset)}`).join(', ');
 return `<article class="route-result"><h3>${esc(r.asset_in)} → ${esc(r.asset_out)} · ${esc(r.residual_policy)}</h3><p>Вход: ${esc(r.input_budget)} ${esc(r.asset_in)}<br>Потрачено: ${esc(r.input_spent)} · Не потрачено на первой ноге: ${esc(r.input_unspent)}<br>Выход: <strong>${esc(r.output)} ${esc(r.asset_out)}</strong></p><p>Остатки в кошельке: <strong>${residuals||'нет'}</strong></p><table><tr><th>Площадка</th><th>Бюджет / расход</th><th>Выход</th><th>Остаток</th></tr>${r.hops.map(h=>`<tr><td>${esc(h.hop.venue)}<br>${esc(h.hop.kind)}</td><td>${esc(h.budget)} / ${esc(h.spent)} ${esc(h.asset_in)}</td><td>${esc(h.output)} ${esc(h.asset_out)}</td><td>${esc(h.unspent)} ${esc(h.asset_in)}</td></tr>`).join('')}</table><p class="hint">Комиссия исполнения указана в receipt отдельно. Остатки не пересчитываются в стоимость портфеля.</p></article>`;
}

export function renderRoutes(run,state,api) {
 const panel=$('#route-panel');
 token++;$('#route-quote-result').innerHTML='';
 panel.hidden=!run.scenario.model_versions.includes('heterogeneous-route-v1');
 if(panel.hidden)return;
 const owners=[...new Set(Object.values(run.scenario.agents).flatMap(a=>a.controlled_accounts))];
 const options=(el,items,preferred)=>{let old=el.value;if(!items.includes(old))old=preferred??items[0];el.innerHTML=items.map(x=>`<option ${x===old?'selected':''}>${esc(x)}</option>`).join('');};
 options($('#route-owner'),owners,owners.includes('trader')?'trader':owners[0]);
 const assets=Object.keys(run.scenario.assets);
 options($('#route-input'),assets,'X');options($('#route-output'),assets,assets.includes('R')?'R':assets[1]);
 const limits=[...Object.entries(run.scenario.books??{}).flatMap(([id,c])=>[c.base_asset,c.quote_asset].map(a=>({id,asset:a,kind:'Book',value:'50'}))),...Object.entries(run.scenario.clmm_pools??{}).flatMap(([id,c])=>[c.x,c.q].map(a=>({id,asset:a,kind:'Clmm',value:a===c.x?'-100':'100'})))];
 if(activeRun!==run.run_id) {
  activeRun=run.run_id;
  $('#route-limits').innerHTML=limits.map((l,i)=>`<div class="route-limit"><label><input type="checkbox" data-include="${i}"> ${esc(l.id)} · вход ${esc(l.asset)}</label><label>${l.kind==='Book'?'Предел цены, ticks':'Предел sqrt-price, tick'}<input data-limit="${i}" value="${l.value}" ${l.kind==='Clmm'?'type="number" step="1" min="-32768" max="32768"':'inputmode="numeric"'}></label></div>`).join('');
 }
 const venues=[...Object.keys(run.scenario.protocols).map(id=>`${id} · CPMM`),...Object.keys(run.scenario.stable_pools??{}).map(id=>`${id} · StableSwap`),...Object.keys(run.scenario.clmm_pools??{}).map(id=>`${id} · CLMM`),...Object.keys(run.scenario.books??{}).map(id=>`${id} · Book`)];
 $('#route-venues').textContent=venues.join(' / ');
 const form=$('#route-quote-form');
 form.oninput=()=>{token++;$('#route-quote-result').innerHTML='';};
 form.onsubmit=async e=>{
  e.preventDefault();const pending=++token;
  try {
   const sender=$('#route-owner').value,agent=Object.entries(run.scenario.agents).find(([,a])=>a.controlled_accounts.includes(sender))?.[0];
   const enabled=limits.flatMap((l,i)=>{
    if(!form.querySelector(`[data-include="${i}"]`).checked)return [];
    const value=form.querySelector(`[data-limit="${i}"]`).value;
    let hop;
    if(l.kind==='Clmm') {const tick=Number(value);if(!Number.isInteger(tick)||tick< -32768||tick>32768)throw Error('Tick вне диапазона ±32768');hop={kind:'Clmm',venue:l.id,limit_tick:tick};}
    else hop={kind:'Book',venue:l.id,limit_price_ticks:atoms(value)};
    return [{asset_in:l.asset,hop}];
   });
   const query={agent,sender,asset_in:$('#route-input').value,asset_out:$('#route-output').value,amount:atoms($('#route-amount').value),min_out:atoms($('#route-minimum').value),residual_policy:$('#route-residual').value,resource_limit:atoms($('#route-resource').value),resource_price:atoms($('#route-price').value),limits:enabled};
   const q=await api('route-quote',{block:Number(state.block),query});
   if(pending!==token)return;
   const candidates=q.search.candidates;
   $('#route-quote-result').innerHTML=`<p>Проверено кандидатов: ${esc(q.search.examined)} / 64${q.search.truncated?' · поиск ограничен бюджетом':''}</p><p class="hint">Порядок: больше конечного выхода, затем меньше ресурса, затем ID площадок. Остатки показаны отдельно.</p>`+(candidates.length?candidates.map((c,i)=>`<details ${i===0?'open':''}><summary>${i===0&&c.receipt?.status==='Success'?'Лучший выход · ':''}${c.hops.map(h=>esc(h.venue)).join(' → ')} · ${esc(c.receipt?.status??'Rejected')}</summary><p>${esc(c.admission_error??c.receipt?.error??'')} ${c.receipt?`${esc(c.receipt.used)} units · fee ${esc(c.receipt.fee)} ${esc(run.scenario.chain.fee_asset)}`:''}</p>${routeCard(c.receipt?.route_results?.['0'])}${c.receipt?.status!=='Success'?(c.receipt?.trace??[]).map(t=>routeCard(t.route)).join(''):''}</details>`).join(''):'<p>Пути с выбранными пределами не найдены.</p>')+`<p class="hint">Пробное исполнение · блок ${esc(q.block)}<br>State ref: <code>${esc(q.state_ref)}</code></p>`;
  }catch(e){if(pending===token)$('#route-quote-result').textContent=e.message;}
 };
}
