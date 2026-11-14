"""DEC-30 browser acceptance against the current release on localhost:8080."""
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
    for name in ['lp', 'capital', 'ordered', 'infeasible', 'unpriced', 'fractional', 'credit']:
        page.locator('#preset').select_option('sc-hodl-' + name)
        page.locator('#new').click()
        expect(page.locator('#hodl-panel')).to_be_visible()
        expect(page.locator('#hodl-body')).to_contain_text('Блок 0')
        genesis = page.request.get(BASE + '/api/checkpoint').json()
        scenario = genesis['run']['scenario']
        rounds = scenario.get('scheduler', {}).get('rounds', len(scenario['blocks']))
        boundaries = [page.request.get(BASE + '/api/hodl/0').json()]
        for block in range(1, rounds + 1):
            page.locator('#step').click()
            expect(page.locator('#position')).to_contain_text(f'Исполнено {block} / {rounds}')
            expect(page.locator('#hodl-body')).to_contain_text(f'Блок {block}')
            boundaries.append(page.request.get(BASE + f'/api/hodl/{block}').json())
        full = page.request.get(BASE + '/api/run').json()
        final = boundaries[-1]
        if name == 'lp':
            expect(page.locator('#hodl-excess')).to_have_text('-100')
            expect(page.locator('#hodl-pnl')).to_have_text('300')
            expect(page.locator('#hodl-actual-pnl')).to_have_text('200')
        if name == 'capital':
            expect(page.locator('#hodl-excess')).to_have_text('-30')
            expect(page.locator('#hodl-pnl')).to_have_text('60')
        if name == 'ordered':
            assert page.locator('#hodl-flows code').all_text_contents() == ['z-in', 'a-out']
            expect(page.locator('#hodl-excess')).to_have_text('0')
        if name == 'infeasible':
            expect(page.locator('#hodl-status')).to_contain_text('InfeasibleOutflow')
            expect(page.locator('#hodl-excess')).to_contain_text('Unpriced')
            expect(page.locator('#hodl-holdings')).to_contain_text('граница блока 0')
            expect(page.locator('#hodl-body')).to_contain_text('Последняя допустимая корзина · только история')
        if name == 'unpriced':
            expect(page.locator('#hodl-value')).to_have_text('19')
            expect(page.locator('#hodl-gap')).to_have_text('-1')
            expect(page.locator('#hodl-excess')).to_contain_text('HistoricalFlowUnpriced')
        if name == 'fractional':
            expect(page.locator('#hodl-excess')).to_have_text('-1 / 2')
        if name == 'credit':
            page.locator('#hodl-portfolio').select_option('depositor')
            expect(page.locator('#hodl-excess')).to_have_text('-20')
        with page.expect_download() as downloaded:
            page.locator('#hodl-download').click()
        payload = json.loads(Path(downloaded.value.path()).read_text())
        assert payload == dict(policy=scenario['hodl_policy'], valuation_policy=scenario['valuation_policy'], boundary=final)
        # Every history query is deterministic and leaves all native state intact.
        for block in [0, rounds, 0, rounds]:
            page.locator(f'#blocks button[data-block="{block - 1}"]').click()
            expect(page.locator('#hodl-body')).to_contain_text(f'Блок {block}')
            assert page.request.get(BASE + f'/api/hodl/{block}').json() == boundaries[block]
        assert not page.request.get(BASE + f'/api/hodl/{rounds + 1}').ok
        assert page.request.get(BASE + '/api/run').json() == full
        # Checkpoints contain native history; restoration rebuilds HODL exactly.
        assert page.request.post(BASE + '/api/restore', data=genesis).ok
        page.reload()
        for block in range(1, rounds + 1):
            page.locator('#step').click()
            expect(page.locator('#hodl-body')).to_contain_text(f'Блок {block}')
            assert page.request.get(BASE + f'/api/hodl/{block}').json() == boundaries[block]
        assert page.request.get(BASE + '/api/run').json() == full
        page.set_viewport_size({'width': 390, 'height': 844})
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), name
        if name in ['lp', 'infeasible', 'unpriced']:
            page.locator('#hodl-panel').screenshot(path=f'output/m5c-{name}-mobile.png')
        page.set_viewport_size({'width': 1440, 'height': 1100})
        print('PASS browser', name, flush=True)
    # Portfolio/source/transaction identifiers are displayed as text, not HTML.
    s = page.request.get(BASE + '/api/preset/sc-hodl-capital').json()
    malicious = '<img src=x onerror="window.hodlInjected=1">'
    p = s['valuation_policy']
    p['portfolios'][malicious] = p['portfolios'].pop('owner')
    for binding in p['capital_transfers']:
        binding['portfolio'] = malicious
    s['hodl_policy']['portfolios'] = [malicious]
    p['reference_feeds']['X']['source_id'] = malicious
    assert page.request.post(BASE + '/api/run', data=s).ok
    page.reload()
    expect(page.locator('#hodl-body')).to_contain_text('Блок 0')
    expect(page.locator('#hodl-portfolio option')).to_have_text(malicious)
    assert page.locator('#hodl-panel img').count() == 0
    assert page.evaluate('window.hodlInjected') is None
    page.locator('#preset').select_option('sc-01')
    page.locator('#new').click()
    expect(page.locator('#hodl-panel')).to_be_hidden()
    assert not page.request.get(BASE + '/api/hodl/0').ok
    assert not errors, errors
    browser.close()
print('PASS: seven HODL demos, exact fractions, canonical flow order, Unpriced/infeasible diagnostics, read-only history, checkpoint replay, downloads, mobile and escaped identifiers')
