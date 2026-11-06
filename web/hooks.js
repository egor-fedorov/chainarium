const $=s=>document.querySelector(s);
const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
function instruction(i){
 switch(i.type){
 case 'FeeByInput':return `${esc(i.asset_in)}: вход < ${esc(i.threshold)} → fee ${i.below_bps} bps; иначе ${i.at_or_above_bps} bps`;
 case 'MaxInput':return `Предел входа: ${esc(i.maximum)} ${esc(i.asset_in)}`;
 case 'IncrementCounter':return 'Увеличить собственный счётчик на 1';
 case 'PaySender':return `Перевести ${esc(i.amount)} ${esc(i.asset)} из hook account отправителю`;
 case 'Reject':return `Отклонить: ${esc(i.reason)}`;
 default:return esc(i.type);
 }
}
export function hookCard(h){
 if(!h)return '';
 const o=h.observation,a=o?.action;
 return `<article class="hook-trace"><h3>Hook ${esc(h.hook)} · ${esc(h.point)}</h3>${h.instruction?`<p>${instruction(h.instruction)}</p>`:''}<p>Counter: ${esc(h.counter_before)} → ${esc(h.counter_after)}${h.effective_fee!==null?`<br>Выбранная fee: ${h.effective_fee} bps`:''}</p>${o?`<p>Pool ${esc(o.pool)} · sender ${esc(o.sender)}<br>Блок ${esc(o.block)} / ${esc(o.timestamp)} s · reserves ${esc(o.reserves[0])} ${esc(o.x)} / ${esc(o.reserves[1])} ${esc(o.y)}</p>`:''}${a?.type==='Swap'?`<p>Вход ${esc(a.input)} ${esc(a.asset_in)} · min_out ${esc(a.min_out)}<br>Base fee ${a.base_fee} bps${a.output!==null?` · effective fee ${a.effective_fee} bps<br>Фактический swap output ${esc(a.output)}`:''}</p>`:a?`<p>${esc(a.type)} liquidity${a.actual?` · ${esc(a.actual.x)} ${esc(o.x)} / ${esc(a.actual.y)} ${esc(o.y)} · shares ${esc(a.actual.shares)}`:''}</p>`:''}</article>`;
}
export function renderHooks(run,state,block){
 const hooks=Object.entries(run.scenario.hooks??{}),panel=$('#hook-panel');panel.hidden=!hooks.length;if(!hooks.length)return;
 $('#hook-config').innerHTML='<h2>Какие правила добавляет hook?</h2><p>Программа выполняется внутри swap или LP-вызова. Выплаты обеспечены отдельным балансом; отказ откатывает их вместе со счётчиком и операцией пула.</p>'+hooks.map(([id,c])=>{
 const caps=c.capabilities;
 return `<article><h3>${esc(id)} · ${esc(c.pool)}</h3><p>Собственный счёт: ${esc(c.account)}<br>Counter: <strong>${esc(state.hooks[id].counter)}</strong><br>Баланс: ${Object.entries(state.accounts[c.account].balances).map(([a,n])=>`${esc(n)} ${esc(a)}`).join(' / ')}</p><p class="hint">Права: ${[caps.swap_fee?'изменять swap fee':'',caps.reject?'отклонять операцию':'',caps.write_local?'менять свой counter':'',...Object.entries(caps.spend_own).map(([a,n])=>`платить до ${esc(n)} ${esc(a)} на одну точку`)].filter(Boolean).join('; ')}</p>${Object.entries(c.programs).map(([point,program])=>`<details><summary>${esc(point)} · ${program.length} инструкций</summary><ol>${program.map(i=>`<li>${instruction(i)}</li>`).join('')}</ol></details>`).join('')}</article>`;
 }).join('')+'<p class="hint">Чистая кривая по base fee не учитывает hook. Используйте «Проверить маршрут» с владельцем, лимитом ресурса и min_out. Выплата hook показана отдельно от swap output и не передаётся следующему hop.</p>';
 $('#hook-result').innerHTML=block?block.receipts.filter(r=>r.trace.some(t=>t.hook)).map(r=>`<article><h3>${esc(r.tx_id)} · ${esc(r.status)}</h3><p>${esc(r.used)} units · native fee ${esc(r.fee)} ${esc(run.scenario.chain.fee_asset)}${r.error?'<br>'+esc(r.error):''}</p>${r.status!=='Success'?'<p class="notice">Hook effects этой попытки отменены. Включённые fee и nonce сохранены.</p>':''}<details><summary>Точки и инструкции · trace</summary><table><tr><th>Точка / инструкция</th><th>Исход / counter</th></tr>${r.trace.filter(t=>t.hook).map(t=>`<tr><td>${esc(t.path)}<br>${t.hook.instruction?instruction(t.hook.instruction):esc(t.hook.point)}</td><td>${esc(t.outcome)}<br>${esc(t.hook.counter_before)} → ${esc(t.hook.counter_after)}${t.hook.effective_fee!==null?'<br>fee '+t.hook.effective_fee+' bps':''}</td></tr>`).join('')}</table></details></article>`).join('')||'<p>В этом блоке hook не вызывался.</p>':'<p>Запустите блок для просмотра сохранённой trace. Просмотр не исполняет hook.</p>';
}
