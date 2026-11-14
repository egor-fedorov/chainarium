import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

ROOT=Path('/tmp/chainarium-dec29-evidence')
BASE='http://127.0.0.1:8765'
events=[]
with sync_playwright() as p:
    browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
    page=browser.new_page(viewport={'width':1440,'height':1100})
    errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
    page.goto(BASE)
    expect(page.locator('#position')).to_contain_text('Исполнено')
    before=page.request.get(BASE+'/api/run').json()
    for kind in ['builder','policy','asymmetric-missing','rational-cap']:
        report=json.loads((ROOT/kind/'manifest.json').read_text())
        page.locator('#experiment-import').set_input_files(ROOT/kind/'manifest.json')
        expect(page.locator('#experiment-name')).to_have_text(report['name'])
        expect(page.locator('#experiment-runs tbody tr')).to_have_count(len(report['rows']))
        assert page.request.get(BASE+'/api/run').json()==before
        if kind=='builder':
            expect(page.locator('#experiment-summary')).to_contain_text('-575/2')
            expect(page.locator('#experiment-summary')).to_contain_text('66125/3')
        elif kind=='policy':
            expect(page.locator('#experiment-summary')).to_contain_text('-1629/4')
            expect(page.locator('#experiment-summary')).to_contain_text('2093695/76')
            expect(page.locator('#experiment-summary')).to_contain_text('Отрицательных: 20')
        elif kind=='asymmetric-missing':
            expect(page.locator('#experiment-summary')).to_contain_text('Исключения (2)')
            expect(page.locator('#experiment-summary')).to_contain_text('0, 3, 4')
            expect(page.locator('#experiment-summary')).to_contain_text('-248/3')
            expect(page.locator('#experiment-summary')).to_contain_text('нулевых: 1')
        else:
            expect(page.locator('#experiment-summary')).to_contain_text('ArithmeticLimit')
            expect(page.locator('#experiment-summary')).to_contain_text('доступно 64')
        page.set_viewport_size({'width':390,'height':844})
        assert page.evaluate('document.documentElement.scrollWidth<=innerWidth')
        page.locator('#experiment-summary').screenshot(path=str(ROOT/f'{kind}-independent-mobile.png'))
        page.set_viewport_size({'width':1440,'height':1100})
        events.append(kind+': exact statistics/exclusions, read-only import and narrow layout pass')
    # Verify a legal exported report actually gets blocked by the import cap.
    page.locator('#experiment-import').set_input_files(ROOT/'large-legal-series/manifest.json')
    expect(page.locator('#error')).to_contain_text('Отчёт больше 64 MiB')
    assert page.request.get(BASE+'/api/run').json()==before
    page.locator('#error').screenshot(path=str(ROOT/'large-report-rejected.png'))
    events.append('CONFIRMED: legal completed 69,232,220-byte report rejected by UI as >64 MiB')
    # Show the accepted corrupt report through the same UI, not only raw API.
    page.locator('#experiment-import').set_input_files(ROOT/'accepted-inconsistent-horizon.json')
    expect(page.locator('#error')).to_be_hidden()
    expect(page.locator('#experiment-outcome')).to_contain_text('Completed')
    expect(page.locator('#experiment-runs tbody tr').first).to_contain_text('0 / 0')
    page.locator('#experiment-runs').locator('..').locator('summary').click()
    page.locator('#experiment-runs').screenshot(path=str(ROOT/'accepted-zero-horizon.png'))
    events.append('CONFIRMED: UI displays Completed/0-of-0 rows for retained three-block Plan and nonzero PnL')
    # Selected native checkpoint identity, read-only failures and exact restore.
    report=json.loads((ROOT/'builder/manifest.json').read_text())
    checkpoint=json.loads((ROOT/'builder/runs/0000/checkpoint.json').read_text())
    result=page.request.post(BASE+'/api/experiment-checkpoint',data={'report':report,'row':1,'checkpoint':checkpoint})
    assert not result.ok
    assert page.request.get(BASE+'/api/run').json()==before
    page.locator('#experiment-import').set_input_files(ROOT/'builder/manifest.json')
    expect(page.locator('#experiment-name')).to_have_text(report['name'])
    expect(page.locator('#experiment-runs tbody tr').first).to_contain_text('3 / 3')
    page.locator('#experiment-checkpoint').set_input_files(ROOT/'builder/runs/0000/checkpoint.json')
    expect(page.locator('#position')).to_contain_text('Исполнено 3')
    assert page.request.get(BASE+'/api/run').json()==checkpoint['run']
    with page.expect_download() as d: page.locator('#experiment-download').click()
    assert json.loads(Path(d.value.path()).read_text())==report
    events.append('Selected checkpoint: mismatched row rejected without mutation; matching row native replay and report download pass')
    assert not errors,errors
    browser.close()
(ROOT/'ui-independent-result.json').write_text(json.dumps(events,indent=2))
print('\n'.join(events))
