import {atoms,formatUnits} from './wire.mjs';
import {routeCard,renderRoutes} from './routes.js';
import {decisionCard,renderPolicies} from './policies.js';
import {renderScheduler} from './scheduler.js';
import {renderBundles} from './bundles.js';
const $=s=>document.querySelector(s);
let run=null, previous=null, viewed=-1, selected=0, mode='canonical', selectedCall=0, playing=false, timer=null, busy=false;
const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
async function api(path,body,method=body===undefined?'GET':'POST') {const response=await fetch('/api/'+path,{method,headers:body===undefined?{}:{'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});if(!response.ok)throw Error(await response.text());return response.json();}
function error(e){$('#error').hidden=false;$('#error').textContent=e.message;stop();}
async function guard(fn){if(busy)return;busy=true;$('#error').hidden=true;try{await fn();}catch(e){error(e);}finally{busy=false;}}
function stop(){playing=false;clearTimeout(timer);$('#play').textContent='▶ Продолжить';}
function download(name,value){const a=document.createElement('a');const url=URL.createObjectURL(new Blob([JSON.stringify(value,null,2)],{type:'application/json'}));a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
function balance(state,account,asset){return state.accounts[account]?.balances[asset]??'0';}
function render(){
 if(!run)return;
 const block=run.blocks[viewed], state=block?.state??run.initial;
 renderRoutes(run,state,api);
 renderPolicies(run);
 renderScheduler(run,block);
 renderBundles(run,block);
 const pools={...run.scenario.protocols,...Object.fromEntries(Object.entries(run.scenario.stable_pools??{}).map(([id,cfg])=>[id,cfg.pool]))};
 const ids=Object.keys(pools);let venue=$('#venue').value;
 if(!ids.includes(venue))venue=ids[0];
 $('#venue').innerHTML=ids.map(id=>`<option value="${esc(id)}" ${venue===id?'selected':''}>${esc(id)} · ${esc(pools[id].x)}/${esc(pools[id].y)}</option>`).join('');
 const pool=pools[venue];
 const clmmCfg=Object.values(run.scenario.clmm_pools??{})[0];
 const bookCfg=Object.values(run.scenario.books??{})[0];
 const creditId=Object.keys(run.scenario.lending??{})[0],creditCfg=run.scenario.lending?.[creditId],creditState=state.lending?.[creditId];
 $('#venue-control').hidden=!pool;$('#lp-title').hidden=!pool;$('#positions').hidden=!pool;
 $('#scenario-title').textContent=run.scenario.name;
 $('#assumptions').innerHTML=`<p>Активы: ${Object.entries(run.scenario.assets).map(([id,a])=>`${esc(id)} (${a.decimals} decimals)`).join(', ')}. Ресурс блока: ${esc(run.scenario.chain.block_limit)}; интервал: ${esc(run.scenario.chain.interval)} s.</p><p>Комиссия исполнения и полный резерв задаются каждым конвертом. Подтверждённые события и трасса попытки разделены. Общий MTM/PnL не вычисляется.</p><code>${run.scenario.model_versions.map(esc).join('<br>')}</code>`;


 const poolAccount=pool?.account??creditCfg?.cash_account??bookCfg?.account??clmmCfg?.account??Object.keys(state.accounts)[0];
 const x=pool?.x??creditCfg?.debt_asset??bookCfg?.base_asset??clmmCfg?.x??Object.keys(run.scenario.assets)[0],y=pool?.y??creditCfg?.collateral_asset??bookCfg?.quote_asset??clmmCfg?.q??Object.keys(run.scenario.assets)[1]??x;
 const sender=run.scenario.blocks[0]?.[0]?.sender??run.scenario.scheduler?.emissions[0]?.transaction.sender??Object.keys(state.accounts)[0];
 $('#chart-title').textContent=creditCfg&&!pool?`Cash резерва, atoms ${x}`:bookCfg&&!pool?`Escrow, atoms ${x}`:`Баланс ${x} в пуле`;
 $('#chart').setAttribute('aria-label',$('#chart-title').textContent);

 $('#position').textContent=`Исполнено ${run.blocks.length} / ${(run.scenario.scheduler?.rounds??run.scenario.blocks.length)} блоков · просмотр ${state.block}`;
 $('#step').disabled=run.blocks.length>=(run.scenario.scheduler?.rounds??run.scenario.blocks.length);$('#play').disabled=$('#step').disabled;
 $('#blocks').innerHTML=[`<button data-block="-1" class="${viewed===-1?'active':''}">Начальное состояние</button>`,...run.blocks.map((b,i)=>`<button data-block="${i}" class="${viewed===i?'active':''}">Блок ${esc(b.number)} <small>· ${esc(b.timestamp)} s</small></button>`)].join('');
 $('#blocks').querySelectorAll('button').forEach(b=>b.onclick=()=>{viewed=Number(b.dataset.block);selected=0;selectedCall=0;render();});
 $('#stats').innerHTML=(creditState&&!pool?[[`Cash C, ${x}`,balance(state,poolAccount,x)],['Долг D',creditState.debt],['Требования S',creditState.claims],['Equity E',creditState.equity]]:[[`${bookCfg&&!pool?'Escrow':'Резерв'} ${x}`,balance(state,poolAccount,x)],[`${bookCfg&&!pool?'Escrow':'Резерв'} ${y}`,balance(state,poolAccount,y)],[`Nonce ${sender}`,state.accounts[sender]?.nonce??'0']]).map(([k,v])=>`<div class="stat"><span>${esc(k)}</span><strong>${esc(v)}</strong></div>`).join('');
 const states=[run.initial,...run.blocks.map(b=>b.state)], ys=states.map(s=>Number(balance(s,poolAccount,x))),lo=Math.min(...ys)*.95,hi=Math.max(...ys)*1.05;
 const points=ys.map((y,i)=>`${25+i*580/Math.max(1,ys.length-1)},${110-(y-lo)/(hi-lo||1)*90}`);
 $('#chart').innerHTML=`<path d="M25 120H615" stroke="#dce4df"/><polyline points="${points.join(' ')}" fill="none" stroke="#087e72" stroke-width="3"/>`+points.map((p,i)=>`<circle cx="${p.split(',')[0]}" cy="${p.split(',')[1]}" r="${i===viewed+1?6:4}" fill="#087e72"/><text x="${p.split(',')[0]}" y="137" text-anchor="middle" fill="#62757a" font-size="10">${i}</text>`).join('');
 $('#balances').innerHTML='<table><thead><tr><th>Счёт</th><th>atoms '+esc(x)+'</th><th>atoms '+esc(y)+'</th></tr></thead><tbody>'+Object.keys(state.accounts).map(a=>`<tr><td>${esc(a)}</td><td>${esc(balance(state,a,x))}</td><td>${esc(balance(state,a,y))}</td></tr>`).join('')+'</tbody></table>';
 $('#positions').innerHTML='<table><tr><th>Пул / владелец</th><th>Shares</th></tr>'+Object.entries(state.lp_balances).flatMap(([id,owners])=>Object.entries(owners).map(([owner,shares])=>`<tr><td>${esc(id)} / ${esc(owner)}</td><td>${esc(shares)}</td></tr>`)).join('')+'</table>';
 renderCredit(state);renderMarket(state,venue);renderOracles(state);renderBooks(state);renderClmm(state);
 $('#system-events').innerHTML=block?.system_events?.length?'<details open><summary>Системное начисление перед транзакциями</summary>'+events(block.system_events)+'</details>':'';
 $('#decisions').innerHTML=(block?.decisions??[]).map(d=>decisionCard(d,run,block)).join('');
 $('#transactions').innerHTML=block?block.receipts.map((r,i)=>`<button class="tx ${selected===i?'active':''}" data-tx="${i}"><span class="badge ${r.status==='Success'?'':'failure'}">${esc(r.status)}</span><span>${esc(r.tx_id)}<small>${esc(r.used)} units · ${esc(r.fee)} ${esc(run.scenario.chain.fee_asset)} комиссия</small></span></button>`).join('')+block.rejected.map(r=>`<p class="notice">Не включена: ${esc(r.tx_id)} · ${esc(r.reason)}</p>`).join(''):'<p class="empty">Выполните первый блок или выберите сохранённый блок.</p>';
 $('#transactions').querySelectorAll('button').forEach(b=>b.onclick=()=>{selected=Number(b.dataset.tx);selectedCall=0;render();});
 const receipt=block?.receipts[selected];$('#tx-title').textContent=receipt?.tx_id??'Выберите транзакцию';
 $('#receipt').innerHTML=receipt?`<table>${[['Статус',receipt.status],[`Резерв ${run.scenario.chain.fee_asset}`,receipt.reserved],[`Комиссия ${run.scenario.chain.fee_asset}`,receipt.fee],[`Возврат ${run.scenario.chain.fee_asset}`,receipt.refund],['Ресурс',receipt.used]].map(([k,v])=>`<tr><td>${esc(k)}</td><td>${esc(v)}</td></tr>`).join('')}</table>${receipt.error?`<p class="notice">${esc(receipt.error)}</p>`:''}`:'';
 if(receipt?.charges)$('#receipt').innerHTML+='<details><summary>Расшифровка ресурса</summary><table>'+receipt.charges.map(c=>`<tr><td>${esc(c.phase)}</td><td>${esc(c.charged)} / ${esc(c.requested)}</td></tr>`).join('')+'</table><p class="hint">Списано / требовалось. При исчерпании расходуется весь оставшийся лимит.</p></details>';
 $('#canonical').classList.toggle('active',mode==='canonical');$('#attempt').classList.toggle('active',mode==='attempt');$('#calls').innerHTML='';$('#detail').innerHTML='';
 if(receipt){
   if(mode==='attempt'){
     $('#calls').innerHTML=receipt.trace.map((t,i)=>`<button class="call ${i===selectedCall?'active':''}" data-call="${i}">${esc(t.path)} · ${esc(t.operation)}<br><small>${esc(t.outcome)} · ${esc(t.used)} units суммарно</small></button>`).join('');
     $('#calls').querySelectorAll('button').forEach(b=>b.onclick=()=>{selectedCall=Number(b.dataset.call);render();});
     const t=receipt.trace[selectedCall];if(t)$('#detail').innerHTML=`<p class="hint">${esc(t.caller)} → ${esc(t.callee)}</p>${receipt.status!=='Success'?'<p class="notice">Попытка отменена. Показанные переводы не вошли в состояние.</p>':''}`+(t.oracle?oracleCard(t.oracle):'')+routeCard(t.route)+bookCard(t.book)+clmmCard(t.clmm)+solverCard(t.solver??[])+twapCard(t.twap)+sourceChanges(t.source_updates??[])+events(t.events);
   }else $('#detail').innerHTML=(receipt.status==='Success'?receipt.trace.map(t=>routeCard(t.route)+bookCard(t.book)+clmmCard(t.clmm)).join(''):'')+events(receipt.events)+(receipt.status!=='Success'?'<p class="notice">Переводы отменены. Комиссия и израсходованный nonce сохранены.</p>':'');
 }
 if(previous&&previous.scenario.name===run.scenario.name){const a=previous.blocks[viewed]?.state??(viewed===-1?previous.initial:null);$('#comparison').innerHTML='<div class="section-label">СРАВНЕНИЕ A/B · ОДИНАКОВЫЙ БЛОК</div>'+ (a?`<p class="hint">A: предыдущий запуск; B: текущий. Факторы: input ${esc(previous.scenario.blocks[0]?.[0]?.program[0]?.amount??'—')} → ${esc(run.scenario.blocks[0]?.[0]?.program[0]?.amount??'—')}, fee ${Object.values(previous.scenario.protocols)[0]?.fee_bps??'—'} → ${pool?.fee_bps??'—'} bps; min_out ${esc(previous.scenario.blocks[0]?.[0]?.program[0]?.min_out??'—')} → ${esc(run.scenario.blocks[0]?.[0]?.program[0]?.min_out??'—')}.</p><table><tr><th>Резерв</th><th>A</th><th>B</th></tr>${[x,y].map(asset=>`<tr><td>${esc(asset)}</td><td>${balance(a,poolAccount,asset)}</td><td>${balance(state,poolAccount,asset)}</td></tr>`).join('')}</table>`:'<p class="muted">В предыдущем запуске этот блок ещё не исполнен.</p>');}else{$('#comparison').innerHTML='<div class="section-label">СРАВНЕНИЕ A/B</div><p class="muted">Для сравнения измените параметр и повторите тот же preset.</p>';}
}
let clmmRender=0,clmmQuoteToken=0;
function clmmCard(t){
 if(!t)return '';
 const c=run.scenario.clmm_pools[t.pool],out=t.asset_in===c.x?c.q:c.x;
 return `<details open class="clmm-trace"><summary>${esc(t.action)}${t.position_id?' · #'+esc(t.position_id):''}</summary><p>Tick ${t.tick_before} → ${t.tick_after}<br>Активная L: ${esc(t.liquidity_before)} → ${esc(t.liquidity_after)}</p>`+
  (t.asset_in?`<p>Запрошено: ${esc(t.requested)} ${esc(t.asset_in)}<br>Потрачено: ${esc(t.spent)} · остаток: ${esc(t.remainder)}<br>Выход: ${esc(t.output)} ${esc(out)} · fee: ${esc(t.fee)} ${esc(t.asset_in)}</p>`:`<p>Расчёт: ${esc(t.amounts[0])} ${esc(c.x)} + ${esc(t.amounts[1])} ${esc(c.q)}<br>Начисленные fees позиции: ${esc(t.position_owed[0])} ${esc(c.x)} / ${esc(t.position_owed[1])} ${esc(c.q)}</p>`)+
  `<p class="hint">S до: <code>${esc(t.sqrt_before)}</code><br>S после: <code>${esc(t.sqrt_after)}</code> · Q96</p>`+
  t.steps.map((s,i)=>`<details open><summary>Шаг ${i+1}${s.crossed_tick!==null?' · crossing '+s.crossed_tick:''}</summary><p>Tick ${s.tick_before} → ${s.tick_after}<br>L ${esc(s.liquidity_before)} → ${esc(s.liquidity_after)}<br>Gross ${esc(s.gross)} · net ${esc(s.net)} · fee ${esc(s.fee)}<br>Output ${esc(s.output)} ${esc(out)}</p><p class="hint"><code>${esc(s.sqrt_before)} → ${esc(s.sqrt_after)}</code></p></details>`).join('')+'</details>';
}
async function renderClmm(state){
 const token=++clmmRender,runId=run.run_id;clmmQuoteToken++;$('#clmm-quote-result').innerHTML='';
 const ids=Object.keys(run.scenario.clmm_pools??{});$('#clmm-panel').hidden=!ids.length;if(!ids.length)return;
 let selected=$('#clmm-select').value;if(!ids.includes(selected))selected=ids[0];
 $('#clmm-select').innerHTML=ids.map(id=>`<option ${id===selected?'selected':''}>${esc(id)}</option>`).join('');
 const cfg=run.scenario.clmm_pools[selected],owners=[...new Set(Object.values(run.scenario.agents).flatMap(a=>a.controlled_accounts))];
 let owner=$('#clmm-owner').value;if(!owners.includes(owner))owner=owners.includes('trader')?'trader':owners[0];
 $('#clmm-owner').innerHTML=owners.map(id=>`<option ${id===owner?'selected':''}>${esc(id)}</option>`).join('');
 const asset=$('#clmm-input').value;$('#clmm-input').innerHTML=[cfg.x,cfg.q].map(a=>`<option ${a===asset?'selected':''}>${esc(a)}</option>`).join('');
 $('#clmm-view').innerHTML='<p class="muted">Чтение диапазонов…</p>';
 try{
  const v=await api('clmm/'+state.block);if(token!==clmmRender||run.run_id!==runId||v.run_id!==runId||v.block!==state.block)return;
  const p=v.pools.find(p=>p.pool===selected),c=p.config;
  const lo=Math.min(p.current_tick,...p.ticks.map(t=>t.tick))-10,hi=Math.max(p.current_tick,...p.ticks.map(t=>t.tick))+10,maxL=Math.max(1,...p.ticks.map(t=>Number(t.liquidity_above)));
  const x=t=>25+(t-lo)/(hi-lo)*590,y=l=>140-Number(l)/maxL*110;
  const path=p.ticks.map(t=>`H${x(t.tick)}V${y(t.liquidity_above)}`).join('');
  const chart=`<svg class="liquidity-chart" viewBox="0 0 640 170" role="img" aria-label="Активная ликвидность по диапазонам ticks"><path d="M25 140${path}H615" stroke="#087e72" stroke-width="3" fill="none"/><path d="M${x(p.current_tick)} 15V145" stroke="#bd522d" stroke-dasharray="4 4"/><text x="${x(p.current_tick)}" y="162" text-anchor="middle" fill="#bd522d" font-size="12">tick ${p.current_tick}</text></svg>`;
  $('#clmm-view').innerHTML=`<h2>${esc(p.pool)} · ${esc(c.x)}/${esc(c.q)}</h2><p>Swap fee ${c.fee_bps} bps · ticks ±32768</p><div class="stats"><div class="stat"><span>Активная L</span><strong>${esc(p.active_liquidity)}</strong></div><div class="stat"><span>Текущий tick</span><strong>${p.current_tick}</strong></div></div>${chart}<p class="hint">Линия показывает L по ticks; маркер — текущий tick. Это не reserves и не объём, доступный по неизменной цене.</p>`+
   `<table><tr><th>В пуле</th><th>${esc(c.x)}</th><th>${esc(c.q)}</th></tr>${[['Резервы',p.reserves],['Требования LP',p.claims],['Остатки округления',p.rounding_surplus]].map(([label,n])=>`<tr><td>${label}</td><td>${esc(n[0])}</td><td>${esc(n[1])}</td></tr>`).join('')}</table><p class="hint">Требования обеспечены reserves. Остатки округления не выводятся и не создают ликвидность.</p>`+
   p.positions.map(v=>`<article class="clmm-position"><h3>#${esc(v.id)} ${esc(v.owner)} · ${v.active?'Active':'Inactive'}</h3><p>[${v.lower}, ${v.upper}) · L ${esc(v.liquidity)}</p><table><tr><th>Позиция</th><th>${esc(c.x)}</th><th>${esc(c.q)}</th></tr><tr><td>Principal при burn</td><td>${esc(v.principal[0])}</td><td>${esc(v.principal[1])}</td></tr><tr><td>Fees при collect</td><td>${esc(v.collectable[0])}</td><td>${esc(v.collectable[1])}</td></tr></table></article>`).join('')+
   (!p.positions.length?'<p class="empty">Открытых позиций нет.</p>':'')+
   `<details><summary>Точные price и ticks</summary><p>S: <code>${esc(p.sqrt_price)}</code> · Q96<br>Цена: <code>${esc(p.price.numerator)} / ${esc(p.price.denominator)}</code> atoms ${esc(c.q)} за atom ${esc(c.x)}</p><table><tr><th>Tick</th><th>L выше</th><th>Gross L</th><th>Net L</th></tr>${p.ticks.map(t=>`<tr><td>${t.tick}</td><td>${esc(t.liquidity_above)}</td><td>${esc(t.gross)}</td><td>${t.net.negative?'−':'+'}${esc(t.net.magnitude)}</td></tr>`).join('')}</table></details>`;
 }catch(e){if(token===clmmRender)$('#clmm-view').textContent=e.message;}
}
$('#clmm-select').onchange=()=>render();
$('#clmm-input').onchange=()=>{const c=run.scenario.clmm_pools[$('#clmm-select').value];$('#clmm-limit').value=$('#clmm-input').value===c.x?'-100':'100';};
$('#clmm-quote-form').oninput=()=>{clmmQuoteToken++;$('#clmm-quote-result').innerHTML='';};
$('#clmm-quote-form').onsubmit=async e=>{
 e.preventDefault();const token=++clmmQuoteToken,runId=run.run_id;
 try{
  const sender=$('#clmm-owner').value,agent=Object.entries(run.scenario.agents).find(([,a])=>a.controlled_accounts.includes(sender))?.[0],limit=Number($('#clmm-limit').value);
  if(!Number.isInteger(limit)||limit < -32768||limit > 32768)throw Error('Tick должен быть целым в пределах ±32768');
  const q=await api('clmm-quote',{block:viewed+1,agent,sender,pool:$('#clmm-select').value,asset_in:$('#clmm-input').value,amount:atoms($('#clmm-amount').value),limit_tick:limit,min_out:atoms($('#clmm-minimum').value),allow_partial:$('#clmm-partial').value==='true'});
  if(token!==clmmQuoteToken||run.run_id!==runId)return;
  $('#clmm-quote-result').innerHTML=`<p><strong>${esc(q.receipt.status)}</strong> · ${esc(q.receipt.used)} units${q.receipt.error?' · '+esc(q.receipt.error):''}</p>`+q.receipt.trace.map(t=>clmmCard(t.clmm)).join('')+`<p class="hint">Только пробное исполнение · блок ${esc(q.block)}<br>State ref: <code>${esc(q.state_ref)}</code></p>`;
 }catch(e){if(token===clmmQuoteToken)$('#clmm-quote-result').textContent=e.message;}
};
let bookRender=0,bookQuoteToken=0;
function bookCard(t){
 if(!t)return '';
 const cfg=run.scenario.books[t.book];
 return `<details open class="book-trace"><summary>${esc(t.book)} · #${esc(t.order_id)} · ${esc(t.status)}</summary><p>${t.side==='Buy'?'Покупка':'Продажа'} · limit ${esc(t.price_ticks)} ticks<br>Запрошено ${esc(t.requested_lots)} lots · исполнено ${esc(t.filled_lots)} · остаток ${esc(t.remaining_lots)}<br>Стоящая часть: ${esc(t.resting_lots)} lots</p><p>Резерв: ${esc(t.reserved)} ${esc(t.reserve_asset)} · возврат: ${esc(t.refund)}<br>${t.side==='Buy'?'Получено':'Продано'}: ${esc(t.base_filled)} ${esc(cfg.base_asset)} · ${t.side==='Buy'?'Уплачено':'Выручено'}: ${esc(t.taker_quote)} ${esc(cfg.quote_asset)}<br>Gross: ${esc(t.gross_quote)} ${esc(cfg.quote_asset)} · taker fee: ${esc(t.fee)} ${esc(cfg.quote_asset)}</p>${t.fills.length?`<table><tr><th>Maker ID</th><th>Lots</th><th>Ticks</th><th>Gross ${esc(cfg.quote_asset)}</th><th>Fee ${esc(cfg.quote_asset)}</th></tr>`+t.fills.map(f=>`<tr><td>#${esc(f.maker_id)}<br>${esc(f.maker)}</td><td>${esc(f.lots)}</td><td>${esc(f.price_ticks)}</td><td>${esc(f.gross_quote)}</td><td>${esc(f.fee)}</td></tr>`).join('')+'</table>':''}<p class="hint">Комиссия округляется вверх для каждого match. Maker fee = 0. Переводы перечислены в трассе исполнения.</p></details>`;
}
async function renderBooks(state){
 const token=++bookRender,runId=run.run_id;bookQuoteToken++;
 $('#book-quote-result').innerHTML='';
 const ids=Object.keys(run.scenario.books??{});$('#book-panel').hidden=!ids.length;
 if(!ids.length)return;
 let selected=$('#book-select').value;if(!ids.includes(selected))selected=ids[0];
 $('#book-select').innerHTML=ids.map(id=>`<option ${id===selected?'selected':''}>${esc(id)}</option>`).join('');
 const owners=[...new Set(Object.values(run.scenario.agents).flatMap(a=>a.controlled_accounts))];
 let owner=$('#book-owner').value;if(!owners.includes(owner))owner=owners.includes('carol')?'carol':owners[0];
 $('#book-owner').innerHTML=owners.map(id=>`<option ${id===owner?'selected':''}>${esc(id)}</option>`).join('');
 $('#book-view').innerHTML='<p class="muted">Чтение очереди…</p>';
 try{
  const v=await api('books/'+state.block);
  if(token!==bookRender||run.run_id!==runId||v.run_id!==runId||v.block!==state.block)return;
  const b=v.books.find(b=>b.book===selected),c=b.config;
  const levels=(levels,label,cls,asset)=>`<div class="${cls}"><h3>${label}</h3>${levels.length?'<table><tr><th>Ticks</th><th>'+esc(c.quote_asset)+'/lot</th><th>Lots</th></tr>'+levels.map(l=>`<tr><td>${esc(l.price_ticks)}</td><td>${esc(l.quote_per_lot)}</td><td>${esc(l.lots)}</td></tr><tr><td colspan="3">${l.orders.map(o=>`<small>#${esc(o.id)} ${esc(o.owner)} · ${esc(o.remaining_lots)} lots · locked ${esc(o.locked)} ${esc(asset)}</small>`).join('')}</td></tr>`).join('')+'</table>':'<p class="empty">Пусто</p>'}</div>`;
  $('#book-view').innerHTML=`<p>Lot: ${esc(c.lot_atoms)} atoms ${esc(c.base_asset)} · tick: ${esc(c.quote_tick_atoms)} atoms ${esc(c.quote_asset)}/lot<br>Maker fee 0 · taker fee ${c.taker_fee_bps} bps → ${esc(c.fee_account)}</p><p class="hint">Лучший уровень первым; очередь внутри уровня — по ID принятия. Цена gross, до taker fee.</p><div class="stats"><div class="stat"><span>Escrow ${esc(c.base_asset)}</span><strong>${esc(b.escrow_base)}</strong></div><div class="stat"><span>Escrow ${esc(c.quote_asset)}</span><strong>${esc(b.escrow_quote)}</strong></div><div class="stat"><span>Следующий ID</span><strong>${esc(b.next_id)}</strong></div></div><div class="book-sides">${levels(b.bids,'Bids · покупка','bids',c.quote_asset)}${levels(b.asks,'Asks · продажа','asks',c.base_asset)}</div>`;
 }catch(e){if(token===bookRender)$('#book-view').textContent=e.message;}
}
$('#book-select').onchange=()=>render();
$('#book-quote-form').oninput=()=>{bookQuoteToken++;$('#book-quote-result').innerHTML='';};
$('#book-quote-form').onsubmit=async e=>{
 e.preventDefault();const token=++bookQuoteToken,runId=run.run_id,block=run.blocks[viewed],state=block?.state??run.initial;
 try{
  const sender=$('#book-owner').value,agent=Object.entries(run.scenario.agents).find(([,a])=>a.controlled_accounts.includes(sender))?.[0];
  const transaction={id:'readonly-book-quote',agent,sender,chain:run.scenario.chain.id,nonce:state.accounts[sender].nonce,limit:'1000',price:'0',deadline:state.timestamp,program:[{type:'PlaceOrder',book:$('#book-select').value,side:$('#book-side').value,price_ticks:atoms($('#book-price').value),lots:atoms($('#book-lots').value),tif:$('#book-tif').value}]};
  const q=await api('book-quote',{block:viewed+1,transaction});
  if(token!==bookQuoteToken||run.run_id!==runId)return;
  $('#book-quote-result').innerHTML=`<p><strong>${esc(q.receipt.status)}</strong> · ${esc(q.receipt.used)} units${q.receipt.error?' · '+esc(q.receipt.error):''}</p>`+q.receipt.trace.map(t=>routeCard(t.route)+bookCard(t.book)+clmmCard(t.clmm)).join('')+`<p class="hint">Только пробное исполнение · блок ${esc(q.block)}<br>State ref: <code>${esc(q.state_ref)}</code></p>`;
 }catch(e){if(token===bookQuoteToken)$('#book-quote-result').textContent=e.message;}
};
let oracleRender=0;
function ratioText(r){return r?`${esc(r.numerator)} / ${esc(r.denominator)}`:'Unavailable';}
function twapCard(q){
 if(!q)return '';
 const units=run.scenario.price_sources[q.source];
 return `<details open><summary>TWAP · ${esc(q.outcome)} · окно ${esc(q.window)} s</summary><p class="hint">Источник ${esc(q.source)} · конец ${esc(q.end)} s<br>Точное среднее: ${q.exact_average?ratioText(q.exact_average):'—'}</p><table><tr><th>Интервал, s</th><th>Цена ${esc(units.quote_asset)}/${esc(units.base_asset)} в atoms</th></tr>${q.segments.map(s=>`<tr><td>${esc(s.start)} → ${esc(s.end)}</td><td>${ratioText(s.price)}</td></tr>`).join('')}</table><p class="hint">При публикации округление вниз к scale 10¹⁸ выполняется один раз.</p></details>`;
}
function sourceChanges(updates){return updates.map(u=>`<p class="hint">Изменение источника ${esc(u.source)} в ${esc(u.timestamp)} s:<br>${ratioText(u.price)} quote atoms / base atom</p>`).join('');}
async function renderOracles(state){
 const token=++oracleRender,runId=run.run_id;
 $('#oracle-panel').hidden=!Object.keys(run.scenario.price_sources??{}).length;
 if($('#oracle-panel').hidden)return;
 $('#oracle-view').innerHTML='<p class="muted">Чтение окна…</p>';
 try{
  const v=await api('oracles/'+state.block);
  if(token!==oracleRender||run.run_id!==runId||v.run_id!==runId||v.block!==state.block)return;
  $('#oracle-view').innerHTML=v.feeds.map(f=>{
   const src=f.source,r=f.record;
   return `<div class="oracle"><h2>${esc(f.feed)} · ${esc(f.kind.type)}</h2>`+
    (f.kind.type==='NativeSpot'?'<p class="notice">Небезопасный spot: отношение reserves, а не исполнимая quote.</p>':'')+
    (src?`<p>Spot сейчас: ${ratioText(src.spot)}<br>Источник: ${esc(src.source)} · ${esc(src.pool)}</p><p class="hint">Доступная история: ${esc(src.anchor)} → ${esc(src.last_time)} s<br>Сегментов: ${src.segments} / ${src.capacity}</p>`:'')+
    (f.query?twapCard(f.query)+`<p>Проекция публикации на текущий момент: ${ratioText(f.projected_price)}</p>`:'')+
    (r?`<p><strong>Опубликованная цена: ${ratioText(r.price)}</strong><br>Наблюдение: ${esc(r.observed_at)} s · публикация: ${esc(r.published_at)} s<br>Возраст: ${esc(f.observation_age)} s · sequence ${esc(r.sequence)}</p>`:'<p>Опубликованная цена отсутствует.</p>')+
    '<p class="hint">Чтение окна не публикует цену. Lending использует последнее принятое наблюдение с собственным max_age.</p></div>';
  }).join('');
 }catch(e){if(token===oracleRender)$('#oracle-view').textContent=e.message;}
}
let marketRender=0;
function solverCard(steps){
 return steps.length?`<details class="solver" open><summary>Solver: ${steps.length} итераций · ${steps.length} units</summary><p class="hint">Каждая итерация списывает 1 unit до вычисления. Допуск: 1 нормализованная единица.</p><table><tr><th>Шаг</th><th>До → после</th></tr>${steps.map(s=>`<tr><td>${esc(s.solver)} #${s.iteration}${s.converged?' ✓':''}</td><td>${esc(s.before)} → ${esc(s.after)}</td></tr>`).join('')}</table></details>`:'';
}
async function renderMarket(state,venue){
 const token=++marketRender,runId=run.run_id;
 $('#market-panel').hidden=!venue||!Object.keys(run.scenario.stable_pools??{}).length;
 if($('#market-panel').hidden)return;
 $('#market').innerHTML='<p class="muted">Расчёт котировок…</p>';
 try{
  const v=await api('market/'+state.block+'/'+encodeURIComponent(venue));
  if(token!==marketRender||run.run_id!==runId||v.run_id!==runId||v.block!==state.block)return;
  const m=v.market,p=m.parameters,colors=['#087e72','#bd522d','#5555aa'];
  const points=m.curves.flatMap(c=>c.points.filter(p=>p.output!==null));
  const maxX=Math.max(1,...points.map(p=>Number(p.input))),maxY=Math.max(1,...points.map(p=>Number(p.output)));
  const chart='<svg viewBox="0 0 640 185" role="img" aria-label="Котировки площадок: вход и выход"><path d="M25 10V155H620" stroke="#dce4df" fill="none"/>'+m.curves.map((c,i)=>'<polyline fill="none" stroke="'+colors[i%colors.length]+'" stroke-width="3" points="'+c.points.filter(p=>p.output!==null).map(p=>`${25+Number(p.input)/maxX*590},${155-Number(p.output)/maxY*140}`).join(' ')+'"/>').join('')+'<text x="310" y="180" fill="#62757a" font-size="12">Вход, atoms</text></svg>';
  $('#market').innerHTML=`<h2>${esc(m.venue)} · ${esc(m.model)}</h2>`+(p?`<p>A=${p.amplification} · fee=${p.fee_bps} bps · максимум ${p.iteration_limit} итераций</p><p class="hint">Нормализованные reserves: ${m.normalized.map(esc).join(' / ')}<br>D: ${esc(m.invariant??m.invariant_error)}</p>`:'')+chart+
   '<p class="hint">Котировки по сохранённому состоянию. При исполнении цена и min_out проверяются заново. Оси приблизительные; суммы в таблице точные.</p>'+
   m.curves.map((c,i)=>`<h3 style="color:${colors[i%colors.length]}">${esc(c.venue)} · ${esc(c.model)}</h3><p class="hint">Резервы: ${esc(formatUnits(c.reserve_in,run.scenario.assets[c.asset_in].decimals))} ${esc(c.asset_in)} / ${esc(formatUnits(c.reserve_out,run.scenario.assets[c.asset_out].decimals))} ${esc(c.asset_out)}</p><table><tr><th>Вход ${esc(c.asset_in)}</th><th>Выход ${esc(c.asset_out)}</th></tr>${c.points.map(p=>`<tr><td>${esc(formatUnits(p.input,run.scenario.assets[c.asset_in].decimals))}</td><td>${p.output===null?esc(p.error):esc(formatUnits(p.output,run.scenario.assets[c.asset_out].decimals))}</td></tr>`).join('')}</table>`).join('');
 }catch(e){if(token===marketRender)$('#market').textContent=e.message;}
}
let creditRender=0;
function oracleCard(o){
 const r=o.record,p=r?.price;
 return `<div class="oracle"><strong>Oracle · ${esc(o.feed)} · ${esc(o.status)}</strong>${o.kind?.type==='NativeSpot'?'<p class="notice">Небезопасный spot из reserves CPMM</p>':o.kind?.type==='NativeTwap'?`<p class="hint">TWAP · окно ${esc(o.kind.window)} s</p>`:''}${r?`<p>${esc(p.numerator)} / ${esc(p.denominator)} atoms ${esc(p.quote_asset)} за atom ${esc(p.base_asset)}</p><p class="hint">Источник: ${esc(r.source_id)} · sequence ${esc(r.sequence)}<br>Наблюдение: ${esc(r.observed_at)} s · публикация: ${esc(r.published_at)} s<br>Проверка: ${esc(o.evaluated_at)} s · возраст: ${esc(o.age??'—')} / ${esc(o.max_age)} s</p>`:'<p>Наблюдение отсутствует</p>'}</div>`;
}
async function renderCredit(state){
 const token=++creditRender,runId=run.run_id;
 const ids=Object.keys(run.scenario.lending??{});$('#credit-panel').hidden=!ids.length;
 if(!ids.length)return;
 $('#credit').innerHTML='<p class="muted">Чтение сохранённого состояния…</p>';
 try{
  const v=await api('credit/'+state.block);
  if(token!==creditRender||runId!==run.run_id||v.run_id!==runId||v.block!==state.block)return;
  $('#credit').innerHTML=v.markets.map(m=>{
   const l=state.lending[m.market],cfg=run.scenario.lending[m.market];
   return `<h2>${esc(m.market)} · ${esc(m.status)}</h2><p class="hint">C + D = S + E · залог хранится отдельно от cash.<br>Всего deposit shares: ${esc(l.total_shares)} · начислено до ${esc(l.last_accrual)} s</p>`+
    `<div class="stats">${[['Cash C',m.cash],['Долг D',l.debt],['Требования S',l.claims],['Equity E',l.equity]].map(([k,n])=>`<div class="stat"><span>${esc(k)} · ${esc(cfg.debt_asset)}</span><strong>${esc(n)}</strong></div>`).join('')}</div>`+
    oracleCard(m.oracle)+`<table><tr><th>Вкладчик</th><th>Shares</th></tr>${Object.entries(l.deposit_shares).map(([a,n])=>`<tr><td>${esc(a)}</td><td>${esc(n)}</td></tr>`).join('')}</table>`+
    m.positions.map(p=>`<div class="position"><h3>${esc(p.owner)} · ${esc(p.health)}</h3><p>Залог: ${esc(p.collateral)} atoms ${esc(cfg.collateral_asset)} · ${esc(p.escrow)}<br>Долг: ${esc(p.debt)} atoms ${esc(cfg.debt_asset)} · дробный остаток: ${esc(p.remainder)} / 10¹⁸<br>HF: ${p.health==='NoDebt'?'NoDebt':p.health_numerator===null?'цена недоступна':`${esc(p.health_numerator)} / ${esc(p.debt)}`} · используется: ${p.collateral_enabled?'да':'нет'}<br>Лимит по LTV: ${esc(p.borrow_capacity??'—')} atoms ${esc(cfg.debt_asset)}</p></div>`).join('')+
    (l.losses.length?'<h3>Признанные убытки</h3>'+l.losses.map(x=>`<p>Блок ${esc(x.block)} · ${esc(x.borrower)}: списано ${esc(x.amount)} ${esc(cfg.debt_asset)}; equity ${esc(x.equity_loss)}, вкладчики ${esc(x.depositor_loss)}.</p>`).join(''):'');
  }).join('');
 }catch(e){if(token===creditRender)$('#credit').textContent=e.message;}
}
function events(es){return es.length?'<ul class="events">'+es.map(e=>`<li><span class="muted">${esc(e.kind)}</span><br>${esc(e.from)} → ${esc(e.to)}<br><strong>${esc(e.amount)} ${e.asset.startsWith('L:')?'liquidity':e.asset.startsWith('LOTS:')?'lots':e.kind.includes('Shares')?'shares':e.kind==='InterestRemainderCleared'?'долей 10⁻¹⁸':e.kind==='OraclePublished'?'':e.kind==='PriceTimeAccrued'?'s':'atoms'} ${e.kind==='PriceTimeAccrued'?'':esc(e.asset)}</strong></li>`).join('')+'</ul>':'<p class="empty">Подтверждённых переводов нет.</p>';}
async function newRun(){
 stop();const id=$('#preset').value;const s=await api('preset/'+id);
 if(id==='sc-01'){
   const amount=atoms($('#amount').value),minimum=atoms($('#minimum').value),fee=Number($('#fee').value);
   if(!Number.isInteger(fee)||fee<0||fee>=10000)throw Error('Комиссия должна быть от 0 до 9999 bps');
   for(const block of s.blocks)for(const tx of block)for(const c of tx.program)if(c.type==='SwapExactIn'){c.amount=amount;c.min_out=minimum;}
   s.protocols.cpmm.fee_bps=fee;
 }
 const created=await api('run',s);previous=run;run=created;viewed=-1;selected=0;render();
}
async function next(){run=await api('step',{});viewed=run.blocks.length-1;selected=0;selectedCall=0;render();}
async function tick(){if(!playing)return;await guard(next);if(run.blocks.length>=(run.scenario.scheduler?.rounds??run.scenario.blocks.length))stop();if(playing)timer=setTimeout(tick,600);}
$('#venue').onchange=()=>render();
function presetControls(){
 const id=$('#preset').value;$('#sc01-params').hidden=id!=='sc-01';
 $('#preset-summary').textContent=({'sc-bundle-allowed':'Три members: Transfer20 Q, разрешённый late revert, bid50 Q последним переводом. Bob20, Alice575, builder405 Q = fees355 + bid50.', 'sc-bundle-forbidden':'Та же группа без разрешённого revert. Все пробные receipts остаются диагностикой: канонические fee/nonce и балансы не меняются.', 'sc-bundle-greedy':'Одни входы, разные builders. Greedy выбирает bundle со score405 Q; ordinary с fee345 затем не помещается в остаток блока500.', 'sc-bundle-fee':'Те же входы: FeePriority выбирает ordinary price3, fee345 Q. Bundle имеет minimum price1 и больше не помещается целиком.', 'sc-bundle-visible':'Victim видна в 8 s; bundle отправлен в 9 s и включает её точную копию, ответ и bid5 Q. Victim исполняется один раз; Alice получает net11 Q.', 'sc-bundle-private':'Victim не доставлена submitter. VisibleTxId отклоняется, группа не получает доступ к глобальному private registry. Исполняется только исходная victim.', 'sc-scheduler-public':'Victim отправлена в 7 s, видна в 8 s. Реакция в 9 s прибывает к cutoff 10 s: 100 Q → 116 Q. Явная программа, без поиска; native fee = 0.', 'sc-scheduler-private':'Та же victim доступна только builder. Searcher не получает сообщение, его капитал не меняется.', 'sc-scheduler-delayed':'Builder delay ответа увеличен до 2 s: arrival 11 s пропускает cutoff 10 s. Ответ остаётся в очереди и включается в следующий блок.', 'sc-scheduler-nonce':'Nonce 1 приходит раньше nonce 0. FIFO сначала включает доступный nonce 0, затем повторно проверяет nonce 1. Sender 740 Q, получатель 30 Q, builder 230 Q.', 'sc-scheduler-fifo':'Два кандидата с nonce 0. FIFO выбирает ранний price=1: fee 115 Q; конкурент становится stale и не платит.', 'sc-scheduler-fee':'Те же кандидаты. FeePriority выбирает price=2: fee 230 Q; конкурент становится stale и не платит.', 'sc-lp-policies':'Alice вносит L=1000000 и удерживает позицию вне диапазона. Bob переставляет свой диапазон в блоке 3 и выходит в блоке 5. Общий пул, реальные mint/burn/collect, execution fee = 0.', 'sc-mm-oracle':'Oracle 10 → 12 Q/X: MM меняет bid/ask 45/55 → 55/65 на следующем наблюдении. Carol связывает стакан и CPMM: 10 X → 108 Q → 11 X. При возрасте 3 s (max 2) MM отменяет заявки. Execution fee = 0.', 'sc-03-heterogeneous':'Стакан: 35 X → 198 Q → 165 R, остаток 15 X. CLMM: из 10000 X потрачено 5029; выход через CPMM 3327 R, остаток 4971 X. RequireFull и поздний отказ откатывают все ноги. Execution fee = 0.', 'sc-02-clmm':'Q96, диапазон [−100,100), L=1000000, fee=30 bps. Взнос 4988 X + 4988 Q. Swap 1000 X → 996 Q; burn 5984 X + 3991 Q, collect 2 X. Остатки 2 X + 1 Q не выводятся. Execution fee = 0.', 'sc-clmm-crossing':'Три соседних диапазона с L=1/2/3 млн. Swap 16000 X → 15755 Q пересекает ticks −100 и −200. Обратный swap показывает отказ и явное разрешение partial fill, затем collect/burn всех LP.', 'sc-08-book':'Lot = 10 X, tick = 2 Q/lot. Maker fee = 0, taker = 100 bps. Покупка 4 lots: 40 X за 420 Q + 5 Q fees. FOK/late rollback, частичное GTC, cancel, self-trade и пустой IOC. Execution fee = 0.', 'sc-07-flash-liquidation':'TWAP 20 → 11 Q/X. Flash 60 Q → liquidation 6 X → 84 Q → 83 R → 76 Q. Возврат 61 Q, остаток 15 Q. Первая попытка продаёт залог дешевле и целиком отменяется. Ставка 12.5%/s, execution fee = 0 — учебные параметры.', 'sc-twap':'CPMM 4 X / 400 Q: spot 100 → 200 в t=30. TWAP 150 в t=60. Borrow по TWAP, rollback цены и повтор старого окна: возраст 60/90 s. Execution fee = 0.', 'sc-twap-120':'Тот же источник: 100 за первые 30 s и 200 за следующие 90 s. При W=120 s результат в t=120 равен 175; до этого NotReady.', 'sc-stable':'Q/R, 6 decimals. StableSwap: A=100, fee=4 bps; CPMM: fee=30 bps. Swap 100 Q → 99.860150 R, proportional mint/burn, цикл через обе площадки и rollback. Execution fee = 0.', 'sc-stable-depeg':'StableSwap: 100 Q / 10000 R, A=100, fee=4 bps. Вход 100 Q даёт 554.752748 R. Затем обратный swap и отменённая попытка. Execution fee = 0.', 'sc-05-credit':'100 Q → 100 shares; 6 X обеспечивают заём 80 Q. Ставка 12.5% за модельную секунду, reserve factor 20%. Цена 20 → 11 Q/X. Ликвидация и вывод после убытка. Учебные параметры, execution fee = 0.', 'sc-lp':'Mint: max 100 X / 120 Q, затем burn 25 shares. Последний mint отменяется. Execution fee = 0.', 'sc-03-route':'Политика ищет цикл на своих 100 Q. Не менее 1 Q после execution fee. Fee price = 0.', 'sc-04-flash':'Заём 100 Q, flash fee 5 bps (1 Q), две площадки. Execution fee = 0. Затем недоплата и поздний отказ.'})[id]??'';
}
async function syncControls(){
 const options=Array.from($('#preset').options);
 const presets=await Promise.all(options.map(async o=>[o.value,await api('preset/'+o.value)]));
 const match=presets.find(([,s])=>s.name===run.scenario.name);
 if(match){
  $('#preset').value=match[0];
  if(match[0]==='sc-01'){
   const swap=run.scenario.blocks[0]?.[0]?.program.find(c=>c.type==='SwapExactIn');
   if(swap){$('#amount').value=swap.amount;$('#minimum').value=swap.min_out;}
   $('#fee').value=run.scenario.protocols.cpmm.fee_bps;
  }
 }
 presetControls();
}
$('#preset').onchange=presetControls;
$('#new').onclick=()=>guard(newRun);$('#step').onclick=()=>{stop();guard(next);};$('#play').onclick=()=>{if(playing){stop();return;}playing=true;$('#play').textContent='Ⅱ Пауза';tick();};
$('#canonical').onclick=()=>{mode='canonical';render();};$('#attempt').onclick=()=>{mode='attempt';render();};
$('#save').onclick=()=>guard(async()=>download(`checkpoint-${run.state.block}.json`,await api('checkpoint')));
$('#export').onclick=()=>download('chainarium-run.json',run);
$('#fork').onclick=()=>guard(async()=>{stop();previous=run;run=await api('fork',{label:crypto.randomUUID()});render();});
$('#restore').onchange=e=>guard(async()=>{stop();const file=e.target.files[0];if(!file)return;if(file.size>64*1024*1024)throw Error('Checkpoint больше 64 MiB');const value=JSON.parse(await file.text());const restored=await api('restore',value);previous=run;run=restored;viewed=run.blocks.length-1;await syncControls();render();});
await guard(async()=>{try{run=await api('run');viewed=run.blocks.length-1;await syncControls();render();}catch{await newRun();}});
