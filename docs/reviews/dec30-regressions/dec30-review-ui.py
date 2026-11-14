import json,os
from pathlib import Path
from playwright.sync_api import sync_playwright,expect

BASE='http://127.0.0.1:8080'
REPO=Path('/workspace/chainarium')
ROOT=Path(os.environ.get('DEC30_REVIEW_ROOT','/tmp/dec30-review-ui'));ROOT.mkdir(exist_ok=True)
def textq(q):return q['numerator'] if q['denominator']=='1' else q['numerator']+' / '+q['denominator']
with sync_playwright() as pw:
    browser=pw.chromium.launch(executable_path='/usr/bin/chromium',headless=True)
    page=browser.new_page(viewport={'width':1440,'height':1100});errors=[]
    page.on('pageerror',lambda e:errors.append(str(e)))
    saved=page.request.get(BASE+'/api/checkpoint').json()
    cases=[]
    for name in ['fractional','infeasible','unpriced','credit','ordered','lp','capital']:
        s=json.loads((REPO/f'scenarios/demos/sc-hodl-{name}.json').read_text())
        if name=='fractional':
            s['accounts']['alice']['balances']['Q']='9007199254740993'
            s['valuation_policy']['reference_feeds']['X']['records'][0]['price']['denominator']='3'
            key='<img src=x onerror="window.hodlBad=1"> & exact'
            s['valuation_policy']['portfolios'][key]=s['valuation_policy']['portfolios'].pop('owner')
            s['hodl_policy']['portfolios']=[key]
            s['valuation_policy']['reference_feeds']['X']['source_id']=key
        cases.append((name,s))
    try:
        for name,s in cases:
            assert page.request.post(BASE+'/api/run',data=s).ok
            rounds=s.get('scheduler',{}).get('rounds',len(s['blocks']))
            for _ in range(rounds):assert page.request.post(BASE+'/api/step').ok
            before=page.request.get(BASE+'/api/checkpoint').body()
            page.goto(BASE)
            for i in [rounds,0,*range(1,rounds+1)]:
                page.locator(f'#blocks button[data-block="{i-1}"]').click()
                expect(page.locator('#hodl-body')).to_contain_text(f'Блок {i}')
                report=page.request.get(BASE+f'/api/hodl/{i}').json()
                for p in report['portfolios']:
                    page.locator('#hodl-portfolio').select_option(p['portfolio'])
                    for selector,mark in [('actual',p['actual']),('actual-pnl',p['actual_pnl']),('value',p['hodl']['mark']),('pnl',p['hodl_pnl']),('excess',p['excess_pnl']),('gap',p['value_gap'])]:
                        element=page.locator('#hodl-'+selector)
                        if mark['value'] is None:
                            expect(element).to_contain_text('Unpriced')
                            for reason in mark['reasons']:expect(element).to_contain_text(reason)
                        else:expect(element).to_have_text(textq(mark['value']))
                    assert page.locator('#hodl-flows code').all_text_contents()==[f['cursor']['tx_id'] for f in p['flows']]
                    if p['ledger']['status']=='Unavailable':expect(page.locator('#hodl-status')).to_contain_text(p['ledger']['failure']['reason'])
                with page.expect_download() as got:page.locator('#hodl-download').click()
                payload=json.loads(Path(got.value.path()).read_text())
                assert payload==dict(policy=s['hodl_policy'],valuation_policy=page.request.get(BASE+'/api/run').json()['scenario']['valuation_policy'],boundary=report)
            assert page.request.get(BASE+'/api/checkpoint').body()==before
            assert page.locator('#hodl-panel img').count()==0 and page.evaluate('window.hodlBad') is None
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth')
            if name in ['fractional','infeasible','unpriced']:page.locator('#hodl-panel').screenshot(path=str(ROOT/(name+'.png')))
            page.set_viewport_size({'width':1440,'height':1100})
            print('PASS independent UI',name,flush=True)
        assert not page.request.get(BASE+'/api/hodl/99999').ok
        assert not errors,errors
    finally:
        assert page.request.post(BASE+'/api/restore',data=saved).ok
        browser.close()
print('PASS exact values >2^53, negative fractions, seven presets, escaped identifiers, all boundaries/portfolios, downloads, readonly checkpoint, mobile layout')
