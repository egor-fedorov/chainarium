const $ = s => document.querySelector(s);
const esc = s => String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fraction = q => q ? (q.denominator==='1'?esc(q.numerator):`${esc(q.numerator)} / ${esc(q.denominator)}`) : '—';
const marked = m => !m ? '—' : m.value ? fraction(m.value) : `<span class="unavailable">Недоступно<small>${m.reasons.map(esc).join(', ')}</small></span>`;
const json = v => `<pre>${esc(JSON.stringify(v,null,2))}</pre>`;
const names = {value:'Стоимость',pnl:'PnL',hodl_value:'Стоимость HODL',hodl_pnl:'PnL HODL',excess_pnl:'Excess PnL',value_gap:'Разница с HODL (не PnL)'};
const kinds = {Account:'Счёт',Asset:'Актив',Agent:'Агент',Amm:'AMM',Clmm:'CLMM',Book:'Стакан',Lending:'Кредитный рынок',Oracle:'Oracle',PriceSource:'Источник цены',Flash:'Flash provider',Hook:'Hook'};
const table = (heads,rows) => `<div class="table-scroll"><table><thead><tr>${heads.map(h=>`<th>${h}</th>`).join('')}</tr></thead><tbody>${rows.map(r=>`<tr>${r.map(c=>`<td>${c}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`;
const events = p => table(['Событие','Откуда → куда','Количество'],p.items.map(e=>[esc(e.kind),`${esc(e.from)} → ${esc(e.to)}`,`${esc(e.amount)} ${esc(e.asset)}`]));
function pager(id,p,load) {
 const root=$(id);root.innerHTML=`<button data-prev ${p.offset===0?'disabled':''}>←</button><span>${p.total ? p.offset+1 : 0}–${p.offset+p.items.length} / ${p.total}</span><button data-next ${p.next===null?'disabled':''}>→</button>`;
 root.querySelector('[data-prev]').onclick=()=>load(Math.max(0,p.offset-64));
 root.querySelector('[data-next]').onclick=()=>load(p.next);
}

export function bindExplorer(api,guard,download) {
 let slots={a:null,b:null}, revision=0, counters={}, index=null, comparison=null, primary='A', graph=null, entity=null, receipt=null;
 const keys=()=>({a:slots.a?.key??null,b:slots.b?.key??null});
 const side=()=>$('#explorer-side').value;
 const point=()=>side()===primary ? comparison?.a : comparison?.b;
 const fail=e=>{if(e.message!=='stale'){$('#explorer-error').hidden=false;$('#explorer-error').textContent=e.message;}};
 async function query(view,channel) {
  const generation=revision,ticket=(counters[channel]??0)+1;counters[channel]=ticket;
  const response=await api('explorer/query',{keys:keys(),view});
  if(generation!==revision || counters[channel]!==ticket)throw Error('stale');
  return response;
 }
 const job=fn=>async(...args)=>{try{await fn(...args);}catch(e){fail(e);}};
 function describe() {
  $('#explorer-status').innerHTML=['A','B'].map(id=>{
   const h=slots[id.toLowerCase()];
   return `<article><span class="section-label">${id}</span><h3>${h?esc(h.name):'История не загружена'}</h3>${h?`<p>${h.blocks} блоков · до ${esc(h.through)} s</p><details><summary>Идентичность снимка</summary><p>Run <code>${esc(h.run_id)}</code><br>Checkpoint <code>${esc(h.key)}</code><br>Build <code>${esc(h.build_id)}</code></p></details>`:'<p>Зафиксируйте текущий запуск или откройте checkpoint.</p>'}</article>`;
  }).join('');
 }
 async function refresh(next) {
  revision++;slots=next??await api('explorer');index=null;comparison=null;receipt=null;entity=null;
  $('#explorer-error').hidden=true;$('#explorer-download').disabled=true;describe();
  const available=['A','B'].filter(s=>slots[s.toLowerCase()]);
  $('#explorer-content').hidden=!available.length;if(!available.length)return;
  $('#explorer-side').innerHTML=available.map(s=>`<option>${s}</option>`).join('');
  const result=await query({type:'Index'},'index');index=result.data;primary=result.primary;
  $('#explorer-time').innerHTML=index.times.map(t=>`<option value="${esc(t)}">${esc(t)} s</option>`).join('');
  $('#explorer-slider').max=String(index.times.length-1);$('#explorer-slider').value='0';
  await render();
 }
 async function render() {
  revision++;receipt=null;entity=null;graph=null;comparison=null;
  $('#explorer-download').disabled=true;$('#explorer-error').hidden=true;
  $('#explorer-body').textContent='Читается сохранённая история…';
  $('#explorer-world').hidden=true;$('#explorer-agent').hidden=true;
  const result=await query({type:'Compare',time:$('#explorer-time').value},'compare');comparison=result.data;
  const c=comparison;
  $('#explorer-body').innerHTML=`<div class="explorer-points">${[[primary,c.a],['B',c.b]].filter(([,p])=>p).map(([slot,p])=>`<article><strong>${slot} · блок ${p.block}</strong><p>Snapshot ${esc(p.timestamp)} s · возраст ${esc(p.age)} s<br>${p.receipts} receipts · ${p.rejections} отклонений<br>Ресурс ${esc(p.used)} · оплачено ${fraction(p.native_fees)} ${esc(p.fee_asset)} в этом блоке</p></article>`).join('')}</div>
   <p class="hint">Общее время ${esc(c.time)} s. На каждой стороне показана последняя подтверждённая граница до этого времени. Возраст снимка указан отдельно.</p>
   ${c.reasons.length ? `<p class="notice">Сравнение метрик ограничено: ${c.reasons.map(esc).join(', ')}.</p>` : ''}
   <h3>Стоимость и результат · atoms Q</h3>${c.metrics.length?table(['Портфель / метрика',esc(primary),c.b?'B':'—','B − A'],c.metrics.map(m=>[`${esc(m.portfolio)}<br>${names[m.metric]??esc(m.metric)}`,marked(m.a),marked(m.b),marked(m.difference)])):'<p>Для этих историй reference valuation не настроена.</p>'}
   <p class="hint">Разность PnL требует общей границы времени и одинаковой политики оценки. Это сравнение двух случаев. Статистические выводы доступны отдельно в парных сериях.</p>`;
  const d=c.configuration;
  $('#explorer-diff').innerHTML=d?`<p>${d.entries.length} различий · ${d.checked} проверенных узлов${d.truncated?' · показана часть diff (достигнут лимит)':''}</p>`+table(['Путь','A','B'],d.entries.map(e=>[esc(e.path),diffValue(e.before),diffValue(e.after)])):'<p>Загрузите обе стороны для diff сценариев.</p>';
  $('#explorer-download').disabled=false;
  $('#explorer-download').onclick=()=>download(`comparison-${c.time}.json`,c,0);
  $('#explorer-slider').value=String(index.times.indexOf(c.time));
  $('#explorer-prev').disabled=index.times.indexOf(c.time)===0;$('#explorer-next').disabled=index.times.indexOf(c.time)===index.times.length-1;
  await inspect();
 }
 function diffValue(v) {return v ? v.preview===null ? json(v.value) : `<p>Значение ${v.bytes} bytes<br>SHA-256 <code>${esc(v.sha256)}</code></p><pre>${esc(v.preview??'null')}</pre>` : '<em>Отсутствует</em>';}
 async function inspect() {
  if(!comparison)return;
  const agent=$('#explorer-mode').value==='agent';
  $('#explorer-summary').hidden=agent;
  receipt=null;entity=null;graph=null;
  counters.call=(counters.call??0)+1;counters.observation=(counters.observation??0)+1;
  $('#explorer-world').hidden=agent;$('#explorer-agent').hidden=!agent;
  if(agent) {await loadAgents(0);return;}
  for(const id of ['graph','entities','receipts','rejections','system','balances','balance-pages','entity-pages','receipt-pages','rejection-pages','system-pages'])$('#explorer-'+id).replaceChildren();
  $('#explorer-entity-balance-pages').hidden=true;
  $('#explorer-entity-view').textContent='Выберите объект.';$('#explorer-receipt').textContent='Выберите receipt.';$('#explorer-call').textContent='';
  await Promise.all([loadGraph(),loadEntities(0),loadReceipts(0),loadRejections(0),loadSystem(0)]);
  if($('#explorer-account').value)await loadBalances(0);
 }
 async function loadBalances(offset) {
  const account=$('#explorer-account').value;if(!account)return;
  $('#explorer-balances').textContent='Читаются балансы…';$('#explorer-balance-pages').replaceChildren();
  const {data:p}=await query({type:'Balances',time:comparison.time,account,offset},'balances');
  $('#explorer-balances').innerHTML=table(['Актив',primary,'B','B − A'],p.items.map(r=>[esc(r.asset),esc(r.a??'—'),esc(r.b??'—'),marked(r.difference)]));
  pager('#explorer-balance-pages',p,job(loadBalances));
 }
 async function loadEntities(offset) {
  const {data:p}=await query({type:'Entities',slot:side(),offset},'entities');
  $('#explorer-entities').innerHTML='<option value="">Выберите объект</option>'+p.items.map((e,i)=>`<option value="${i}">${esc(kinds[e.kind])} · ${esc(e.id)}</option>`).join('');
  $('#explorer-entities').onchange=job(async()=>{const e=p.items[Number($('#explorer-entities').value)];if($('#explorer-entities').value!=='' && e)await showEntity(e,0);});
  pager('#explorer-entity-pages',p,job(loadEntities));
 }
 async function showEntity(e,offset) {
  entity=e;$('#explorer-entity-view').textContent='Читается объект…';$('#explorer-entity-balance-pages').hidden=true;
  const {data:v}=await query({type:'Entity',slot:side(),block:point().block,entity:e,offset},'entity');
  $('#explorer-entity-view').innerHTML=`<h3>${esc(kinds[e.kind])} · ${esc(e.id)}</h3>`+
   (v.nonce!==undefined?`<p>Nonce ${esc(v.nonce)}</p>`:'')+
   (v.balances?table(['Актив','Баланс, atoms'],v.balances.items.map(r=>[esc(r.asset),esc(r.amount)])):'')+
   (v.reserves?table(['Актив','Резерв, atoms'],v.reserves.map(r=>[esc(r.asset),esc(r.amount)])):'')+
   (v.cash!==undefined?`<p>Cash ${esc(v.cash)}</p>`:'')+
   `<details><summary>Точные параметры и сохранённое состояние</summary>${json(v)}</details>`;
  $('#explorer-entity-balance-pages').hidden=!v.balances;
  if(v.balances)pager('#explorer-entity-balance-pages',v.balances,job(offset=>showEntity(e,offset)));
  if(e.kind==='Account') {$('#explorer-account').value=e.id;await loadBalances(0);}
  drawGraph();
 }
 async function loadGraph() {const {data:g}=await query({type:'Graph',slot:side()},'graph');graph=g;drawGraph();}
 function drawGraph(highlights=[]) {
  if(!graph)return;
  const nodes=graph.nodes,assets=nodes.filter(n=>n.kind==='Asset'),others=nodes.filter(n=>n.kind!=='Asset');
  const positions=new Map();const key=e=>JSON.stringify(e);
  for(const [x,list] of [[110,assets],[490,others]])list.forEach((e,i)=>positions.set(key(e),[x,36+i*62]));
  const height=Math.max(90,Math.max(assets.length,others.length)*62+20);
  const edge=graph.edges.map(e=>{const a=positions.get(key(e.from)),b=positions.get(key(e.to));return a&&b?`<path d="M${a[0]} ${a[1]}L${b[0]} ${b[1]}"/><title>${esc(e.relation)}</title>`:'';}).join('');
  $('#explorer-graph').innerHTML=`<p class="hint">Связи конфигурации; доступную ликвидность проверяет исполнение. ${graph.truncated?'Граф сокращён по лимиту. ':''}${graph.nodes.length}/${graph.total_nodes} объектов · ${graph.edges.length}/${graph.total_edges} связей.</p><div class="graph-scroll"><svg viewBox="0 0 650 ${height}" role="img" aria-label="Карта рынка выбранной истории"><g class="graph-edges">${edge}</g>${nodes.map((e,i)=>{const [x,y]=positions.get(key(e));return `<g role="button" tabindex="0" data-node="${i}" class="graph-node ${key(e)===key(entity)||highlights.includes(e.id)?'selected':''}" aria-label="${esc(kinds[e.kind])} ${esc(e.id)}"><rect x="${x-90}" y="${y-23}" width="180" height="46" rx="8"/><text x="${x}" y="${y-4}" text-anchor="middle">${esc(kinds[e.kind])}</text><text x="${x}" y="${y+13}" text-anchor="middle">${esc(e.id.length>22?e.id.slice(0,20)+'…':e.id)}</text><title>${esc(e.id)}</title></g>`;}).join('')}</svg></div>`;
  $('#explorer-graph').querySelectorAll('[data-node]').forEach(el=>{const click=job(()=>showEntity(nodes[Number(el.dataset.node)],0));el.onclick=click;el.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();click();}};});
 }
 async function loadReceipts(offset) {
  const {data:p}=await query({type:'Receipts',slot:side(),block:point().block,offset},'receipts');
  $('#explorer-receipts').innerHTML=p.items.map(r=>`<button class="tx" data-receipt="${r.index}"><span>#${r.index} · ${esc(r.tx_id)}<small>${esc(r.sender)} · ${esc(r.status)} · fee ${esc(r.fee)} · ${r.calls} calls</small></span></button>`).join('')||'<p>В этом блоке нет receipts.</p>';
  $('#explorer-receipts').querySelectorAll('[data-receipt]').forEach(el=>el.onclick=job(()=>loadReceipt(Number(el.dataset.receipt),0,0)));
  pager('#explorer-receipt-pages',p,job(loadReceipts));
 }
 async function loadRejections(offset) {const {data:p}=await query({type:'Rejections',slot:side(),block:point().block,offset},'rejections');$('#explorer-rejections').innerHTML=table(['TxId','Причина отклонения'],p.items.map(r=>[esc(r.tx_id),esc(r.reason)]));pager('#explorer-rejection-pages',p,job(loadRejections));}
 async function loadSystem(offset) {const {data:p}=await query({type:'SystemEvents',slot:side(),block:point().block,offset},'system');$('#explorer-system').innerHTML=events(p);pager('#explorer-system-pages',p,job(loadSystem));}
 async function loadReceipt(i,event_offset,call_offset) {
  $('#explorer-receipt').textContent='Читается receipt…';
  receipt=null;counters.call=(counters.call??0)+1;
  $('#explorer-call').textContent='';
  const {data:r}=await query({type:'Receipt',slot:side(),block:point().block,index:i,event_offset,call_offset},'receipt');receipt=r;
  $('#explorer-call').textContent='Выберите вызов для просмотра попытки.';
  $('#explorer-receipt').innerHTML=`<h3>${esc(r.receipt.tx_id)} · ${esc(r.receipt.status)}</h3>${r.receipt.error?`<p class="notice">${esc(r.receipt.error)}</p>`:''}<p>Ресурс ${esc(r.receipt.used)} · резерв ${esc(r.reserved)} · fee ${esc(r.receipt.fee)} · возврат ${esc(r.refund)} ${esc(r.fee_asset)}</p><h4>Подтверждённые события</h4>${events(r.canonical_events)}<div id="explorer-event-pages" class="pager"></div><details><summary>Расход ресурса</summary>${json(r.charges)}</details><h4>Сохранённая трасса попытки</h4><p class="hint">Каждый вызов показан по собственному path. Вложенные события могут входить в трассу родителя; эти строки не суммируются.</p><div id="explorer-calls">${r.calls.items.map(t=>`<button class="call" data-call="${t.index}">${esc(t.path)} · ${esc(t.operation)} · ${esc(t.outcome)}</button>`).join('')}</div><div id="explorer-call-pages" class="pager"></div>`;
  pager('#explorer-event-pages',r.canonical_events,job(offset=>loadReceipt(i,offset,call_offset)));
  pager('#explorer-call-pages',r.calls,job(offset=>loadReceipt(i,event_offset,offset)));
  $('#explorer-calls').querySelectorAll('[data-call]').forEach(el=>el.onclick=job(()=>loadCall(Number(el.dataset.call),0)));
 }
 async function loadCall(call,event_offset) {
  if(!receipt)return;
  const i=receipt.receipt.index;
  const {data:t}=await query({type:'Call',slot:side(),block:point().block,index:i,call,event_offset},'call');
  $('#explorer-call').innerHTML=`<h3>${esc(t.diagnostic.path)} · ${esc(t.diagnostic.operation)}</h3><p>${esc(t.diagnostic.caller)} → ${esc(t.diagnostic.callee)} · ${esc(t.diagnostic.outcome)}</p><p class="${t.committed?'hint':'notice'}">${t.committed?'Попытка вошла в успешную транзакцию.':'Транзакция отменена. Эти попытки переводов не вошли в состояние; fee/nonce сохранены в canonical результате.'}</p>${events(t.attempt_events)}<div id="explorer-attempt-pages" class="pager"></div><details><summary>Диагностика вызова</summary>${json(t.diagnostic)}</details>`;
  pager('#explorer-attempt-pages',t.attempt_events,job(offset=>loadCall(call,offset)));
  drawGraph([t.diagnostic.callee,...t.attempt_events.items.map(e=>e.asset)]);
 }
 async function loadAgents(offset) {
  counters.observation=(counters.observation??0)+1;$('#explorer-observation').textContent='Читается наблюдение…';
  const {data:p}=await query({type:'Agents',slot:side(),offset},'agents');
  $('#explorer-agent-select').innerHTML=p.items.map((a,i)=>`<option value="${i}">${esc(a.id)}</option>`).join('');
  pager('#explorer-agent-pages',p,job(loadAgents));
  const choose=job(async()=>{counters.observation=(counters.observation??0)+1;$('#explorer-observation').textContent='Читается наблюдение…';const a=p.items[Number($('#explorer-agent-select').value)];if(!a){$('#explorer-observation').textContent='Агенты отсутствуют.';return;}
   $('#explorer-agent-account').innerHTML=a.accounts.map(id=>`<option value="${esc(id)}">${esc(id)}</option>`).join('');
   const show=job(async()=>{const {data:o}=await query({type:'Observation',slot:side(),block:point().block,agent:a.id,account:$('#explorer-agent-account').value},'observation');
    $('#explorer-observation').innerHTML=`<h3>${esc(a.id)} · ${esc(o.market.account)}</h3><p>Блок ${esc(o.market.block)} · ${esc(o.market.timestamp)} s · nonce ${esc(o.market.nonce)}</p>${table(['Собственный актив','Баланс, atoms'],Object.entries(o.market.balances).map(([a,n])=>[esc(a),esc(n)]))}<h4>Доставленные сообщения</h4>${table(['TxId','Отправитель','Доставлено'],o.messages.map(m=>[esc(m.transaction.id),esc(m.transaction.sender),`${esc(m.delivered_at)} s`]))}<details><summary>Полное доступное native-наблюдение</summary>${json(o)}</details>`;
   });$('#explorer-agent-account').onchange=show;await show();
  });$('#explorer-agent-select').onchange=choose;await choose();
 }
 for(const slot of ['A','B']) {
  $(`#explorer-pin-${slot}`).onclick=()=>guard(async()=>refresh(await api(`explorer/${slot}/pin`,{})));
  $(`#explorer-import-${slot}`).onchange=e=>guard(async()=>{
   const file=e.target.files[0];e.target.value='';if(!file)return;if(file.size>64*1024*1024)throw Error('Checkpoint больше 64 MiB');
   await refresh(await api(`explorer/${slot}/checkpoint`,JSON.parse(await file.text())));e.target.value='';
  });
 }
 $('#explorer-time').onchange=job(render);
 $('#explorer-slider').oninput=()=>{$('#explorer-time').value=index.times[Number($('#explorer-slider').value)];};
 $('#explorer-slider').onchange=job(render);
 const move=job(async delta=>{const i=index.times.indexOf($('#explorer-time').value)+delta;if(i>=0&&i<index.times.length){$('#explorer-time').value=index.times[i];await render();}});
 $('#explorer-prev').onclick=()=>move(-1);$('#explorer-next').onclick=()=>move(1);
 $('#explorer-side').onchange=job(async()=>{revision++;await inspect();});$('#explorer-mode').onchange=job(async()=>{revision++;await inspect();});
 $('#explorer-balance-form').onsubmit=e=>{e.preventDefault();job(()=>loadBalances(0))();};
 job(()=>refresh())();
}
