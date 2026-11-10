import differential as d
import copy,json,subprocess
for name in ['sc-mtm-flash','sc-agent-random']:
 s=json.loads((d.S/f'scenarios/demos/{name}.json').read_text())
 if 'valuation_policy' not in s:
  s['valuation_policy']=copy.deepcopy(json.loads((d.S/'scenarios/demos/sc-mtm-capital.json').read_text())['valuation_policy'])
  s['valuation_policy']['capital_transfers']=[]
  s['model_versions'].append('reference-mtm-v1')
 a,_=d.run(s,'noninterference-original-'+name)
 changed=copy.deepcopy(s);p=changed['valuation_policy'];p['limits']['component_checks_per_portfolio']=1;p['reference_feeds']={};p['max_age_seconds']='0'
 b,_=d.run(changed,'noninterference-changed-'+name)
 for k in ['initial','state','blocks','scheduler','rng']:assert a.get(k)==b.get(k),(name,k)
 assert a['run_id']!=b['run_id']
for name in ['sc-flash-arb-success','sc-flash-arb-delayed','sc-flash-arb-guard','sc-flash-arb-shortfall','sc-mtm-capital','sc-mtm-unpriced','sc-mtm-flash','sc-mtm-credit','sc-mtm-clmm','sc-mtm-book']:
 s=json.loads((d.S/f'scenarios/demos/{name}.json').read_text());complete,vals=d.run(s,'replay-original-'+name);n=len(complete['blocks'])
 for i in sorted(set([0,n//2,n])):
  cp=d.OUT/f'{name}-{i}.checkpoint.json';out=d.OUT/f'{name}-{i}-resumed'
  if not cp.exists():subprocess.run([str(d.BIN),'checkpoint',str(d.S/f'scenarios/demos/{name}.json'),str(i),str(cp)],check=True,capture_output=True)
  if not out.exists():subprocess.run([str(d.BIN),'resume',str(cp),str(out)],check=True,capture_output=True)
  assert json.loads((out/'run.json').read_text())==complete,(name,i)
  if vals is not None:assert [json.loads(l) for l in (out/'valuations.jsonl').read_text().splitlines()]==vals,(name,i)
print('PASS: valuation input/cap noninterference for flash and random-agent scheduler/RNG; CLI checkpoint-resume at genesis/mid/final for 10 demos with exact derived reports.')
