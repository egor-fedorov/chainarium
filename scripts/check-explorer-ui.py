"""DEC-31 browser acceptance; run check-explorer.py first. Usage: SCRIPT CLI_OUT."""
from pathlib import Path
import json
import sys
from playwright.sync_api import sync_playwright, expect

root = Path(sys.argv[1])
BASE = 'http://127.0.0.1:8080'
with sync_playwright() as pw:
    browser = pw.chromium.launch(executable_path='/usr/bin/chromium', headless=True)
    page = browser.new_page(viewport={'width': 1440, 'height': 1100})
    errors = []
    page.on('pageerror', lambda e: errors.append(str(e)))

    def get(path):
        r = page.request.get(BASE + '/api/' + path)
        assert r.ok, r.text()
        return r.json()

    def post(path, data):
        r = page.request.post(BASE + '/api/' + path, data=data)
        assert r.ok, r.text()
        return r.json()

    def keys():
        return {k: v['key'] if v else None for k, v in get('explorer').items()}

    def query(view):
        return post('explorer/query', dict(keys=keys(), view=view))['data']

    def reload():
        page.goto(BASE)
        expect(page.locator('#explorer-download')).to_be_enabled()
        expect(page.locator('#explorer-entities option').first).to_have_text('Выберите объект')

    def select_time(t):
        page.locator('#explorer-time').select_option(str(t))
        expect(page.locator('#explorer-body')).to_contain_text(f'Общее время {t} s.')
        expect(page.locator('#explorer-download')).to_be_enabled()
        expect(page.locator('#explorer-error')).to_be_hidden()

    def load_pair(name):
        for side in ['a', 'b']:
            post(f'explorer/{side.upper()}/checkpoint', json.loads((root / f'{name}-{side}.checkpoint.json').read_text()))
        reload()

    # Explicit live capture is immutable, and inspecting it leaves native state intact.
    s = json.loads(Path('scenarios/explorer/synchronous-a.json').read_text())
    post('run', s)
    post('explorer/A/pin', {})
    genesis_key = keys()['a']
    post('step', {})
    post('explorer/B/pin', {})
    assert keys()['a'] == genesis_key and keys()['b'] != genesis_key
    assert get('explorer')['a']['blocks'] == 0
    live = get('checkpoint')
    reload()
    assert page.locator('#explorer-time option').all_text_contents() == ['0 s']
    page.locator('#explorer-pin-A').click()
    expect(page.locator('#explorer-time option')).to_have_count(2)
    assert get('checkpoint') == live

    for name in ['synchronous', 'as_of', 'unknown_history', 'wide_fraction', 'different_decimals', 'short_prefix']:
        load_pair(name)
        timeline = query({'type': 'Index'})['times']
        for t in timeline:
            select_time(t)
            report = query(dict(type='Compare', time=t))
            assert report == json.loads((root / f'{name}-{t}.comparison.json').read_text())
            page.locator('#explorer-account').fill('alice')
            page.locator('#explorer-balance-form button').click()
            expect(page.locator('#explorer-balances')).to_contain_text('Q')
            balances = query(dict(type='Balances', time=t, account='alice'))
            for row in balances['items']:
                for amount in [row['a'], row['b']]:
                    expect(page.locator('#explorer-balances')).to_contain_text(amount)
            if name == 'as_of' and t != '0':
                expect(page.locator('#explorer-body')).to_contain_text('BoundaryTimeMismatch')
            if name == 'different_decimals':
                expect(page.locator('#explorer-balances')).to_contain_text('AssetUnitsMismatch')
        if name == 'wide_fraction':
            expect(page.locator('#explorer-body')).to_contain_text('-1 / 3')
            expect(page.locator('#explorer-body')).to_contain_text('27021597764222980 / 3')
        if name == 'unknown_history':
            expect(page.locator('#explorer-body')).to_contain_text('UnpricedLeft')
        with page.expect_download() as dl:
            page.locator('#explorer-download').click()
        assert json.loads(Path(dl.value.path()).read_text()) == report
        for slot in ['A', 'B']:
            page.locator('#explorer-side').select_option(slot)
            expect(page.locator('#explorer-entities option').first).to_have_text('Выберите объект')
            e = next(o for o in page.locator('#explorer-entities option').all_text_contents() if 'alice' in o)
            page.locator('#explorer-entities').select_option(label=e)
            expect(page.locator('#explorer-entity-view')).to_contain_text('alice')
        page.set_viewport_size({'width': 390, 'height': 844})
        assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), name
        if name in ['as_of', 'wide_fraction']:
            page.locator('#explorer-panel').screenshot(path=str(root / f'{name}-mobile.png'))
        page.set_viewport_size({'width': 1440, 'height': 1100})
        assert get('checkpoint') == live
        print('PASS browser Explorer', name, flush=True)

    # Deliberately release an old response after a newer time was rendered.
    load_pair('as_of')
    held = []

    def delay_old_time(route):
        body = route.request.post_data_json
        if body['view'] == dict(type='Compare', time='10'):
            held.append((route, route.fetch()))
        else:
            route.continue_()

    page.route('**/api/explorer/query', delay_old_time)
    page.locator('#explorer-time').select_option('10')
    page.wait_for_timeout(100)
    assert len(held) == 1
    select_time(20)
    held[0][0].fulfill(response=held[0][1])
    page.wait_for_timeout(100)
    expect(page.locator('#explorer-body')).to_contain_text('Общее время 20 s.')
    expect(page.locator('#explorer-error')).to_be_hidden()
    page.unroute('**/api/explorer/query', delay_old_time)

    # UI file import, failed import and stale query are atomic and independent of live Run.
    page.locator('#explorer-import-A').set_input_files(root / 'as_of-a.checkpoint.json')
    expect(page.locator('#explorer-status')).to_contain_text('Explorer as_of A')
    before = get('explorer')
    stale = keys()
    page.locator('#explorer-import-A').set_input_files(root / 'as_of-invalid.checkpoint.json')
    expect(page.locator('#error')).to_be_visible()
    assert get('explorer') == before and get('checkpoint') == live
    post('explorer/A/checkpoint', json.loads((root / 'synchronous-a.checkpoint.json').read_text()))
    assert not page.request.post(BASE + '/api/explorer/query', data=dict(keys=stale, view={'type': 'Index'})).ok
    assert not page.request.post(BASE + '/api/explorer/query', data=dict(keys=keys(), view={'type': 'Index', 'unknown': 1})).ok

    # Paid revert: inspect receipt -> individual attempt -> graph, no invented state.
    s['accounts']['alice']['balances']['Q'] = '1000'
    s['blocks'][0][0]['price'] = '1'
    s['blocks'][0][0]['program'].append(dict(type='Transfer', to='bob', asset='X', amount='1'))
    post('run', s)
    post('step', {})
    for slot in ['A', 'B']:
        post(f'explorer/{slot}/pin', {})
    live = get('checkpoint')
    reload()
    select_time(12)
    expect(page.locator('#explorer-receipts')).to_contain_text('ProtocolRevert')
    page.locator('#explorer-receipts [data-receipt="0"]').click()
    expect(page.locator('#explorer-receipt')).to_contain_text('Подтверждённые события')
    page.locator('#explorer-calls [data-call="0"]').click()
    expect(page.locator('#explorer-call')).to_contain_text('Транзакция отменена')
    assert get('checkpoint') == live

    # A pending child call must not overwrite the next selected receipt.
    post('run', get('preset/sc-hodl-ordered'))
    post('step', {})
    for slot in ['A', 'B']:
        post(f'explorer/{slot}/pin', {})
    reload()
    select_time(1)
    page.locator('#explorer-receipts [data-receipt="0"]').click()
    expect(page.locator('#explorer-receipt')).to_contain_text('z-in')
    held = []

    def delay_call(route):
        if route.request.post_data_json['view']['type'] == 'Call':
            held.append((route, route.fetch()))
        else:
            route.continue_()

    page.route('**/api/explorer/query', delay_call)
    page.locator('#explorer-calls [data-call="0"]').click()
    page.wait_for_timeout(100)
    assert len(held) == 1
    page.locator('#explorer-receipts [data-receipt="1"]').click()
    expect(page.locator('#explorer-receipt')).to_contain_text('a-out')
    held[0][0].fulfill(response=held[0][1])
    page.wait_for_timeout(100)
    expect(page.locator('#explorer-call')).to_have_text('Выберите вызов для просмотра попытки.')
    page.unroute('**/api/explorer/query', delay_call)

    # Saved scheduler observations: own account only, no future/private messages.
    for mode in ['public', 'private', 'delayed']:
        post('run', get('preset/sc-scheduler-' + mode))
        for _ in range(3):
            post('step', {})
        for slot in ['A', 'B']:
            post(f'explorer/{slot}/pin', {})
        live = get('checkpoint')
        reload()
        select_time(10)
        page.locator('#explorer-graph [aria-label="AMM cpmm"]').click()
        expect(page.locator('#explorer-entity-view')).to_contain_text('cpmm')
        page.locator('#explorer-mode').select_option('agent')
        expect(page.locator('#explorer-summary')).to_be_hidden()
        page.locator('#explorer-agent-select').select_option(label='trader')
        expect(page.locator('#explorer-observation')).to_contain_text('trader · alice')
        expect(page.locator('#explorer-observation')).to_contain_text('Блок 1')
        messages = query(dict(type='Observation', slot='A', block=1, agent='trader', account='alice'))['messages']
        assert any(m['transaction']['id'] == 'victim' for m in messages) == (mode != 'private')
        page.locator('#explorer-time').select_option('0')
        expect(page.locator('#explorer-observation')).to_contain_text('Блок 0')
        # Changing boundary repopulates the agent selector, then choose trader again.
        page.locator('#explorer-agent-select').select_option(label='trader')
        expect(page.locator('#explorer-observation')).to_contain_text('trader · alice')
        expect(page.locator('#explorer-observation')).not_to_contain_text('victim')
        assert get('checkpoint') == live
        print('PASS browser historical observation', mode, flush=True)

    # Untrusted names/IDs are escaped, including graph labels and configuration diff.
    malicious = '<img src=x onerror="window.explorerInjected=1">'
    s = json.loads(Path('scenarios/explorer/synchronous-a.json').read_text())
    s['name'] = malicious
    s['accounts'][malicious] = dict(nonce='0', balances={'Q': '11'})
    s['assets'][malicious] = dict(decimals=0)
    post('run', s)
    for slot in ['A', 'B']:
        post(f'explorer/{slot}/pin', {})
    reload()
    expect(page.locator('#explorer-status')).to_contain_text(malicious)
    assert page.locator('#explorer-panel img').count() == 0
    assert page.evaluate('window.explorerInjected') is None
    page.locator('#explorer-account').fill(malicious)
    page.locator('#explorer-balance-form button').click()
    expect(page.locator('#explorer-balances')).to_contain_text('11')
    page.set_viewport_size({'width': 390, 'height': 844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
    assert not errors, errors
    browser.close()
print('PASS Explorer UI: A/B/time, exact downloads, immutable capture, invalid/stale imports, attempt trace, graph, historical agent privacy, escaping and mobile')
