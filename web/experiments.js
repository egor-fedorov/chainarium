const $=s=>document.querySelector(s);
const esc=s=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const json=v=>`<pre>${esc(JSON.stringify(v,null,2))}</pre>`;
// Financial values stay as canonical strings from Rust; no floating point math.
function value(v){return v.status==='Available' ? esc(v.value.denominator==='1'?v.value.numerator:`${v.value.numerator}/${v.value.denominator}`) : `<span class="muted">Недоступно: ${esc(v.reasons.join(', '))}</span>`;}
function table(headers,rows){return `<div class="table-scroll"><table><thead><tr>${headers.map(h=>`<th>${esc(h)}</th>`).join('')}</tr></thead><tbody>${rows.map(r=>`<tr>${r.map(c=>`<td>${c}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`;}
function summary(label,c){
 const s=c.statistics;
 return `<h3>${esc(label)}</h3><p>Запрошено ${esc(c.requested_n)} · завершено ${esc(c.completed_n)} · доступно ${esc(c.eligible_n)}. Единица: ${esc(c.unit)}.</p>`+
 table(['min','p05','median','mean','p95','max','Выборочная дисперсия'],[[s.minimum,s.p05,s.median,s.mean,s.p95,s.maximum,s.sample_variance].map(value)])+
 `<p class="hint">Дисперсия: ${esc(c.variance_unit)}. Отрицательных: ${esc(s.negative)} · нулевых: ${esc(s.zero)} · положительных: ${esc(s.positive)}.</p><p class="hint">Доступные seeds: ${esc(c.eligible_seeds.join(', ')||'нет')}.</p>`+
 (c.excluded.length?`<details open><summary>Исключения (${c.excluded.length})</summary>${table(['Seed','Причины по сторонам'],c.excluded.map(e=>[esc(e.seed),json(e.reasons)]))}</details>`:'<p class="hint">Исключений нет.</p>');
}
export function bindExperiments(api,guard,{beforeRestore,restored,download}){
 let report=null;
 function render(){
  if(!report)return;
  const metric=$('#experiment-metric').value,variant=$('#experiment-variant').value;
  const baseline=report.baseline_variant;
  $('#experiment-summary').innerHTML=summary(baseline,report.analysis.variants[baseline][metric])+summary(variant,report.analysis.variants[variant][metric])+summary(`${variant} − ${baseline} · пары по seed`,report.analysis.paired[variant][metric]);
  const pairs=report.analysis.pairs.filter(p=>p.variant===variant);
  $('#experiment-pairs').innerHTML=table(['Seed',baseline,variant,'Парная разность'],pairs.map(p=>[
   esc(p.seed),value(report.rows.find(r=>r.seed===p.seed&&r.variant===baseline).metrics[metric]),value(report.rows.find(r=>r.seed===p.seed&&r.variant===variant).metrics[metric]),value(p.metrics[metric])
  ]));
  $('#experiment-runs').innerHTML=table(['Seed','Variant','Статус','Блоки','Метрика','Success / Revert / Exhausted','Доля included failures','Отклонённые попытки'],report.rows.map(r=>[
   esc(r.seed),esc(r.variant),esc(r.status.type),`${esc(r.committed_blocks)} / ${esc(r.horizon)}`,value(r.metrics[metric]),esc(['Success','ProtocolRevert','ResourceExhausted'].map(k=>r.outcomes.included[k]).join(' / ')),value(r.outcomes.included_failure_fraction),esc(r.outcomes.inclusion_rejection_attempts)
  ]));
  renderRow();
 }
 function renderRow(){
  if(!report)return;
  const index=Number($('#experiment-row').value),row=report.rows[index];
  const info=report.cases.find(c=>c.seed===row.seed),input=report.plan.cases.find(c=>c.seed===row.seed);
  $('#experiment-row-detail').innerHTML=`<p>Checkpoint: <code>${esc(row.path)}/checkpoint.json</code></p><p class="hint">Открытие заменит текущий запуск в инспекторе. Файл должен соответствовать выбранной строке и пройти native replay.</p>`+
   (row.status.type==='ModelError'?`<p>Ошибка модели: ${esc(row.status.error)}. Сохранённый префикс исключён из конечных распределений.</p><details><summary>Метрики префикса</summary>${json(row.prefix_metrics)}</details>`:'')+
   `<details><summary>Вмешательство: полный diff от baseline</summary>${json(info.variants[row.variant].differences)}</details>`+
   `<details><summary>Общие условия и исходные входы baseline</summary>${json(input.variants[report.baseline_variant])}</details>`+
   `<details><summary>Выбранный Scenario</summary>${json(input.variants[row.variant])}</details>`+
   `<details><summary>Диагностика scheduler и included outcomes</summary>${json(row.outcomes)}</details>`+
   `<details><summary>Хеши входов и сборки</summary>${json({build:report.build_id,plan:report.plan_sha256,common:info.common_input_sha256,external:info.external_inputs_sha256,scenario:info.variants[row.variant].scenario_sha256})}</details>`;
 }
 $('#experiment-import').onchange=e=>guard(async()=>{
  const file=e.target.files[0];if(!file)return;
  if(file.size>64*1024*1024)throw Error('Отчёт больше 64 MiB');
  const imported=await api('experiment-report',JSON.parse(await file.text()));
  report=imported;
  $('#experiment-name').textContent=report.name;
  $('#experiment-hypothesis').textContent=report.hypothesis;
  $('#experiment-outcome').textContent=`${report.outcome} · ${report.rows.length} запусков · ${report.comparison.type} · baseline: ${report.baseline_variant}`;
  $('#experiment-metric').innerHTML=Object.keys(report.metrics).map(m=>`<option value="${esc(m)}">${esc(m)}${m===report.primary_metric?' · primary PnL':''}</option>`).join('');
  $('#experiment-metric').value=report.primary_metric;
  $('#experiment-variant').innerHTML=Object.keys(report.analysis.paired).map(v=>`<option value="${esc(v)}">${esc(v)}</option>`).join('');
  $('#experiment-row').innerHTML=report.rows.map((r,i)=>`<option value="${i}">Seed ${esc(r.seed)} · ${esc(r.variant)} · ${esc(r.status.type)}</option>`).join('');
  $('#experiment-content').hidden=false;render();
 });
 $('#experiment-metric').onchange=render;$('#experiment-variant').onchange=render;$('#experiment-row').onchange=renderRow;
 $('#experiment-checkpoint').onchange=e=>guard(async()=>{
  const file=e.target.files[0];if(!file||!report)return;
  if(file.size>64*1024*1024)throw Error('Checkpoint больше 64 MiB');
  beforeRestore();
  const result=await api('experiment-checkpoint',{report,row:Number($('#experiment-row').value),checkpoint:JSON.parse(await file.text())});
  await restored(result);
 });
 // Preserve the compact report budget on download, including near the 64 MiB cap.
 $('#experiment-download').onclick=()=>{if(report)download('experiment-manifest.json',report,0);};
}
