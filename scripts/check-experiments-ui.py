"""DEC-29 exact report import, exclusions, replay matching and readonly browsing.

Usage: python3 scripts/check-experiments-ui.py output/experiment-validation
Against target/release/chainarium serve 8080; run check-experiments.py first.
"""
import copy
import json
from pathlib import Path
import sys
from playwright.sync_api import sync_playwright, expect
root = Path(sys.argv[1])
base = 'http://127.0.0.1:8080'
with sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path='/usr/bin/chromium', headless=True)
    page = browser.new_page(viewport={'width': 1440, 'height': 1100})
    errors = []
    page.on('pageerror', lambda e: errors.append(str(e)))
    page.goto(base)
    expect(page.locator('#position')).to_contain_text('Исполнено')
    for kind in ['builder','policy','unpriced','model-error']:
        report = json.loads((root/kind/'manifest.json').read_text())
        original = page.request.get(base+'/api/run').json()
        page.locator('#experiment-import').set_input_files(root/kind/'manifest.json')
        expect(page.locator('#experiment-name')).to_have_text(report['name'])
        expect(page.locator('#experiment-outcome')).to_contain_text(report['outcome'])
        expect(page.locator('#experiment-pairs tbody tr')).to_have_count(len(report['cases']))
        expect(page.locator('#experiment-runs tbody tr')).to_have_count(len(report['rows']))
        if kind == 'builder':
            expect(page.locator('#experiment-summary')).to_contain_text('-575/2')
            expect(page.locator('#experiment-summary')).to_contain_text('66125/3')
        elif kind == 'policy':
            expect(page.locator('#experiment-summary')).to_contain_text('-1629/4')
            expect(page.locator('#experiment-summary')).to_contain_text('2093695/76')
            expect(page.locator('#experiment-summary')).to_contain_text('Отрицательных: 20')
        elif kind == 'unpriced':
            expect(page.locator('#experiment-summary')).to_contain_text('NoPricedObservations')
            expect(page.locator('#experiment-summary')).to_contain_text('Исключения (1)')
        else:
            expect(page.locator('#experiment-summary')).to_contain_text('ModelError')
            expect(page.locator('#experiment-row-detail')).to_contain_text('префикс исключён')
        for metric in ['fees','success','rejections','pnl']:
            page.locator('#experiment-metric').select_option(metric)
            assert page.request.get(base+'/api/run').json() == original
        page.locator('#experiment-row').select_option('0')
        page.locator('#experiment-row-detail details').first.locator('summary').click()
        assert page.request.get(base+'/api/run').json() == original
        # The server rejects a different row or internally inconsistent report,
        # without replacing the current run.
        checkpoint = json.loads((root/kind/report['rows'][0]['path']/'checkpoint.json').read_text())
        bad = copy.deepcopy(report)
        bad['analysis']['paired']['alternative']['pnl']['eligible_n'] = '127'
        assert not page.request.post(base+'/api/experiment-report', data=bad).ok
        assert not page.request.post(base+'/api/experiment-checkpoint', data={'report':report,'row':1,'checkpoint':checkpoint}).ok
        assert page.request.get(base+'/api/run').json() == original
        page.locator('#experiment-checkpoint').set_input_files(root/kind/report['rows'][0]['path']/'checkpoint.json')
        expect(page.locator('#position')).to_contain_text(f'Исполнено {report["rows"][0]["committed_blocks"]}')
        expect(page.locator('#error')).to_be_hidden()
        assert page.request.get(base+'/api/run').json() == checkpoint['run']
        expect(page.locator('#experiment-name')).to_have_text(report['name'])
        if kind == 'model-error':
            page.locator('#step').click()
            expect(page.locator('#error')).to_contain_text('ModelError')
            assert page.request.get(base+'/api/run').json() == checkpoint['run']
        with page.expect_download() as downloaded:
            page.locator('#experiment-download').click()
        payload=Path(downloaded.value.path()).read_bytes()
        assert len(payload)==(root/kind/'manifest.json').stat().st_size
        assert len(payload)<=64*1024*1024
        assert json.loads(payload)==report
        page.set_viewport_size({'width':390,'height':844})
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), kind
        if kind == 'unpriced':
            assert page.locator('#experiment-summary details .table-scroll').evaluate_all('(elements)=>elements.every(e=>e.scrollWidth<=e.clientWidth)')
            page.locator('#experiment-summary').screenshot(path=str(root/'unpriced-mobile.png'))
        page.set_viewport_size({'width':1440,'height':1100})
        if kind == 'builder':
            page.locator('#experiment-summary').screenshot(path=str(root/'builder-statistics.png'))
        print(f'{kind}: exact UI, readonly import, exclusions, matching checkpoint and download passed',flush=True)
    assert not errors, errors
    browser.close()
