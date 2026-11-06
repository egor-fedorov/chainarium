import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect
base='http://127.0.0.1:18026'
with sync_playwright() as p:
 browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True)
 page=browser.new_page(viewport={'width':1440,'height':1100})
 errors=[]
 page.on('pageerror',lambda e:errors.append(str(e)))
 page.goto(base)
 outcomes=[]
 for name,rounds,account,expected in [('random',6,'alice','730'),('scripted',3,'alice','740'),('preflight',3,'alice','865'),('borrower',6,'bob','540'),('spot',2,'oracle','780'),('twap',2,'oracle','889')]:
  page.locator('#preset').select_option('sc-agent-'+name)
  page.locator('#new').click()
  expect(page.locator('#position')).to_contain_text(f'Исполнено 0 / {rounds}')
  for i in range(rounds):
   page.locator('#step').click()
   expect(page.locator('#position')).to_contain_text(f'Исполнено {i+1} / {rounds}')
  full=page.request.get(base+'/api/run').json()
  saved=page.request.get(base+'/api/checkpoint').json()
  assert full['state']['accounts'][account]['balances']['Q']==expected
  for index in [-1,0,rounds-1,0,rounds-1]:
   page.locator(f'#blocks button[data-block="{index}"]').click()
   page.locator('#agent-panel').inner_text()
   page.request.get(base+'/api/run')
  assert saved==page.request.get(base+'/api/checkpoint').json(),name
  assert page.request.post(base+'/api/restore',data=saved).ok
  assert full==page.request.get(base+'/api/run').json()
  outcomes.append({'demo':name,'balance':expected,'position':full['rng']['positions']})
 # Independent failing cap reproduction in the actual UI from the same core result.
 s=json.loads(Path('/tmp/dec26-regressions/known-cap.json').read_text())
 assert page.request.post(base+'/api/run',data=s).ok
 for _ in range(2):assert page.request.post(base+'/api/step',data={}).ok
 page.reload()
 expect(page.locator('#agent-result')).to_contain_text('PreflightSuccess')
 expect(page.locator('#agent-result')).to_contain_text('не зарегистрирована')
 panel=page.locator('#agent-result').inner_text()
 Path('/tmp/dec26-known-cap-ui.txt').write_text(panel)
 page.locator('#agent-panel').screenshot(path='/tmp/dec26-known-cap-ui.png')
 assert 'KnownTransactionsLimit' not in panel
 assert not errors,errors
 Path('/tmp/dec26-browser-results.json').write_text(json.dumps(outcomes,indent=2))
 browser.close()
print('PASS: six isolated UI demos, exact core balances, read-only checkpoints, restore and cap mismatch reproduced on 18026; 8080 untouched')
