const $=s=>document.querySelector(s);
const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const role={ScriptedUser:'Пользователь по расписанию',RandomUser:'Случайный пользователь',BorrowerTarget:'Заёмщик с заданной целью',NativeOraclePublisher:'Публикатор native oracle'};
const command=c=>`${esc(c.type)}${c.amount!==undefined?' · '+esc(c.amount)+' atoms':''}${c.feed?' · '+esc(c.feed):''}`;
const cadence=c=>`Блоки ${esc(c.first_block)}–${esc(c.last_block)}, каждые ${esc(c.every)}`;
function config(id,p){
 const k=p.kind;
 let text=k.cadence?`<p>${cadence(k.cadence)}</p>`:'';
 if(k.type==='ScriptedUser')text+=`<p>Режим: ${esc(k.preflight)}</p><ul>${k.schedule.map(e=>`<li>Блок ${esc(e.block)}: ${e.program.map(command).join(' → ')}</li>`).join('')}</ul>`;
 if(k.type==='RandomUser')text+=`<p>Вероятность отправки: ${esc(k.probability_ppm)} / 1000000<br>Режим: ${esc(k.preflight)}</p><ol start="0">${k.programs.map(program=>`<li>${program.map(command).join(' → ')}</li>`).join('')}</ol><p class="hint">На каждом запланированном раунде выбор выполняется и при занятом nonce. Просмотр сохранённых решений не расходует случайность.</p>`;
 if(k.type==='BorrowerTarget')text+=`<p>Reserve: ${esc(k.market)}<br>Шаг долга ≤ ${esc(k.max_debt_step)}, залога ≤ ${esc(k.max_collateral_step)} atoms</p><table><tr><th>С блока</th><th>Цель долга / залога</th></tr>${k.targets.map(t=>`<tr><td>${esc(t.block)}</td><td>${esc(t.debt)} / ${esc(t.collateral)}</td></tr>`).join('')}</table><p class="hint">Сначала погашение избыточного долга, затем внесение залога, заём и вывод. За раунд выполняется не более одного шага.</p>`;
 if(k.type==='NativeOraclePublisher')text+=`<p>Feeds: ${k.feeds.map(esc).join(', ')}</p><p class="hint">Spot определяется при фактическом исполнении. Для TWAP конец окна сохраняется при отправке; NotReady пропускает публикацию.</p>`;
 return `<article><h3>${esc(id)} · ${esc(role[k.type]??k.type)}</h3><p>Счёт: ${esc(p.account)} · limit ${esc(p.resource_limit)} · price ${esc(p.resource_price)} · lifetime ${esc(p.lifetime)} s</p>${text}</article>`;
}
function decision(d,block){
 const o=d.observation,s=d.selection,t=d.trial;
 let body=`<p><strong>${esc(d.action)}</strong> · ${esc(d.reason)}</p>`;
 if(s)body+=`<p>Выбор: gate ${esc(s.gate.value??'RngBudget')} / 1000000 · template ${esc(s.template?.value??'—')}<br>Поток ${esc(s.stream)} · позиция ${esc(s.position_before)} → ${esc(s.position_after)}</p><details><summary>Использованные случайные значения</summary><p>${[...s.gate.draws,...(s.template?.draws??[])].map(esc).join('<br>')}</p></details>`;
 if(o)body+=`<p>Наблюдение: parent ${esc(o.parent_block)} / ${esc(o.parent_timestamp)} s → системная проекция ${esc(o.market.block)} / ${esc(o.market.timestamp)} s</p>`;
 if(d.target)body+=`<p>Действующая цель: debt ${esc(d.target.debt)}, collateral ${esc(d.target.collateral)} atoms</p>`;
 if(o?.credit)body+=`<p>После начисления: debt ${esc(o.credit.debt)}, collateral ${esc(o.credit.collateral)} · oracle ${esc(o.credit.oracle.status)}</p>`;
 if(d.program.length)body+=`<p>Намерение: ${d.program.map(command).join(' → ')}</p>`;
 if(t)body+=`<details open><summary>Пробное исполнение · ${esc(t.status)}</summary><p>${esc(t.used)} units · расчётная fee ${esc(t.fee)}${t.error?'<br>'+esc(t.error):''}</p><p class="hint">Проба не списывает средства. Реальные расходы возникают при включении транзакции.</p></details>`;
 if(d.submission)body+=`<p>Отправка: ${d.submission.type==='Queued'?'поставлена в очередь':'отклонена · '+esc(d.submission.reason)}</p>`;
 if(d.transaction){const status=block.scheduler.state.known[d.transaction.id]?.committed_status;body+=`<p>TxId: <code>${esc(d.transaction.id)}</code><br>Доставка / включение на этой границе: ${esc(status?.type??'не зарегистрирована')}${status?.reason?' · '+esc(status.reason):''}</p>`;}
 return `<article><h3>${esc(d.agent)} · раунд ${esc(d.round)}</h3>${body}</article>`;
}
export function renderAgents(run,block){
 const entries=Object.entries(run.scenario.baseline_policies??{}),panel=$('#agent-panel');panel.hidden=!entries.length;if(!entries.length)return;
 $('#agent-config').innerHTML='<h2>Цель, случайный выбор и действие</h2>'+entries.map(([id,p])=>config(id,p)).join('');
 if(!block){$('#agent-result').innerHTML='<p>Выполните раунд для просмотра решений и фактического результата.</p>';return;}
 const receipts=block.receipts.filter(r=>r.tx_id.startsWith('policy:baseline:'));
 $('#agent-result').innerHTML=(block.scheduler?.baselines??[]).map(d=>decision(d,block)).join('')+'<h3>Фактически включено в этот блок</h3>'+(receipts.length?receipts.map(r=>`<article><p><code>${esc(r.tx_id)}</code><br><strong>${esc(r.status)}</strong> · ${esc(r.used)} units · оплачено ${esc(r.fee)} ${esc(run.scenario.chain.fee_asset)}${r.error?'<br>'+esc(r.error):''}</p></article>`).join(''):'<p>Нет включённых транзакций этих политик; канонические комиссии не списаны.</p>');
}
