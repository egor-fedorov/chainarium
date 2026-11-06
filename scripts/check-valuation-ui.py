"""DEC-28: six MTM demos, exact HTTP/UI wire, replay, downloads and readonly views.

Run against `target/release/chainarium serve 8080` after scripts/check-ui.py.
"""
import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

BASE = 'http://127.0.0.1:8080'
Path('output').mkdir(exist_ok=True)
with sync_playwright() as playwright:
    browser = playwright.chromium.launch(executable_path='/usr/bin/chromium', headless=True)
    page = browser.new_page(viewport={'width': 1440, 'height': 1100})
    errors = []
    page.on('pageerror', lambda e: errors.append(str(e)))
    page.goto(BASE)
    for name in ['capital', 'unpriced', 'flash', 'credit', 'clmm', 'book']:
        page.locator('#preset').select_option('sc-mtm-' + name)
        page.locator('#new').click()
        expect(page.locator('#valuation-panel')).to_be_visible()
        expect(page.locator('#valuation-body')).to_contain_text('Блок 0')
        genesis = page.request.get(BASE + '/api/checkpoint').json()
        scenario = genesis['run']['scenario']
        rounds = scenario.get('scheduler', {}).get('rounds', len(scenario['blocks']))
        boundaries = [page.request.get(BASE + '/api/valuation/0').json()]
        for block in range(1, rounds + 1):
            page.locator('#step').click()
            expect(page.locator('#position')).to_contain_text(f'Исполнено {block} / {rounds}')
            expect(page.locator('#valuation-body')).to_contain_text(f'Блок {block}')
            boundaries.append(page.request.get(BASE + f'/api/valuation/{block}').json())
            if name == 'clmm' and block == 2:
                claims = [c for c in boundaries[-1]['portfolios'][0]['current']['components'] if c['key']['kind'] == 'ClmmClaim']
                assert {c['key']['asset']: c['quantity']['numerator'] for c in claims} == {'X': '5986', 'Q': '3991'}
            if name == 'book' and block == 1:
                expect(page.locator('#valuation-components')).to_contain_text('Escrow заявки')
        full = page.request.get(BASE + '/api/run').json()
        final = boundaries[-1]
        p = final['portfolios'][0]
        if name == 'capital':
            assert p['current']['mark']['value']['numerator'] == '115'
            assert p['pnl']['value']['numerator'] == '10'
            assert p['inflows']['value']['numerator'] == '20'
            assert p['outflows']['value']['numerator'] == '15'
            expect(page.locator('#valuation-pnl')).to_have_text('10')
            page.locator('#valuation-panel').screenshot(path='output/m5a-capital.png')
        if name == 'unpriced':
            assert p['current']['mark']['value']['numerator'] == '10'
            assert p['pnl']['value'] is None
            expect(page.locator('#valuation-pnl')).to_contain_text('HistoricalFlowUnpriced')
            expect(page.locator('#valuation-value')).to_have_text('10')
        if name == 'flash':
            expect(page.locator('#valuation-pnl')).to_have_text('-240')
            expect(page.locator('#flash-arbitrage-result')).to_contain_text('24104 Q')
        if name == 'credit':
            page.locator('#valuation-portfolio').select_option('depositor')
            expect(page.locator('#valuation-pnl')).to_have_text('-20')
            assert next(p for p in final['portfolios'] if p['portfolio'] == 'depositor')['pnl']['value']['numerator'] == '-20'
        with page.expect_download() as downloaded:
            page.locator('#valuation-download').click()
        payload = json.loads(Path(downloaded.value.path()).read_text())
        assert payload == {'policy': scenario['valuation_policy'], 'boundary': final}
        # Navigate committed history without changing canonical state, fees or RNG.
        for block in [0, rounds, 0, rounds]:
            page.locator(f'#blocks button[data-block="{block - 1}"]').click()
            expect(page.locator('#valuation-body')).to_contain_text(f'Блок {block}')
            assert page.request.get(BASE + f'/api/valuation/{block}').json() == boundaries[block]
        assert not page.request.get(BASE + f'/api/valuation/{rounds + 1}').ok
        assert page.request.get(BASE + '/api/run').json() == full
        # Rebuild all derived reports after restore; there is no mutable MTM cache.
        assert page.request.post(BASE + '/api/restore', data=genesis).ok
        page.reload()
        expect(page.locator('#preset')).to_have_value('sc-mtm-' + name)
        for block in range(1, rounds + 1):
            page.locator('#step').click()
            expect(page.locator('#valuation-body')).to_contain_text(f'Блок {block}')
            assert page.request.get(BASE + f'/api/valuation/{block}').json() == boundaries[block]
        assert page.request.get(BASE + '/api/run').json() == full
        page.set_viewport_size({'width': 390, 'height': 844})
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), name
        if name == 'unpriced':
            page.locator('#valuation-panel').screenshot(path='output/m5a-unpriced-mobile.png')
        page.set_viewport_size({'width': 1440, 'height': 1100})
    # UI must display a fraction exactly, never the floor as full value.
    scenario = page.request.get(BASE + '/api/preset/sc-mtm-capital').json()
    scenario['blocks'] = []
    scenario['accounts']['alice']['balances'] = {'X': '1', 'Q': '0'}
    policy = scenario['valuation_policy']
    policy['capital_transfers'] = []
    policy['reference_feeds']['X']['records'] = [dict(available_at='0', observed_at='0', sequence='0', price=dict(base_asset='X', quote_asset='Q', numerator='1', denominator='2'))]
    assert page.request.post(BASE + '/api/run', data=scenario).ok
    page.reload()
    expect(page.locator('#valuation-value')).to_have_text('1 / 2')
    report = page.request.get(BASE + '/api/valuation/0').json()
    assert report['portfolios'][0]['current']['mark']['value'] == dict(numerator='1', denominator='2', floor_atoms='0')
    # Valuation remains opt-in for all earlier scenarios.
    page.locator('#preset').select_option('sc-01')
    page.locator('#new').click()
    expect(page.locator('#valuation-panel')).to_be_hidden()
    assert not page.request.get(BASE + '/api/valuation/0').ok
    assert not errors, errors
    browser.close()
print('PASS: six MTM demos, exact fractional wire, PnL/capital/Unpriced, LP/CLMM/book/credit, selected history, readonly requests, replay, download, mobile, opt-in')
