"""M1–M4f browser/API acceptance. Run against `chainarium serve 8080`. Requires Playwright + Chromium."""
import json
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path='/usr/bin/chromium', headless=True)
    page = browser.new_page(viewport={'width': 1440, 'height': 1100})
    errors = []
    page.on('pageerror', lambda e: errors.append(str(e)))
    page.goto('http://127.0.0.1:8080')
    expect(page.locator('#stats')).to_be_visible()
    page.locator('#preset').select_option('sc-01')
    page.locator('#amount').fill('100')
    page.locator('#minimum').fill('1')
    page.locator('#fee').fill('30')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 3')
    page.locator('#play').click()
    expect(page.locator('#transactions')).to_contain_text('Success')
    page.locator('#play').click()
    page.wait_for_timeout(800)
    expect(page.locator('#position')).to_contain_text('Исполнено 1 / 3')
    expect(page.locator('#stats')).to_contain_text('1100')
    page.locator('#save').click()
    checkpoint = page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('260')
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    page.locator('#attempt').click()
    expect(page.locator('#calls')).to_contain_text('RolledBack')
    expect(page.locator('#detail')).to_contain_text('не вошли в состояние')
    page.locator('#blocks button[data-block="0"]').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    canonical = page.request.get('http://127.0.0.1:8080/api/run').json()
    assert canonical['state']['block'] == '2', 'viewing must not change execution'
    page.locator('#blocks button[data-block="1"]').click()
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ResourceExhausted')
    expect(page.locator('#receipt')).to_contain_text('240')
    full = page.request.get('http://127.0.0.1:8080/api/run').json()
    assert full['state']['accounts']['pool']['balances'] == {'Q':'910','X':'1100'}
    restore = page.request.post('http://127.0.0.1:8080/api/restore', data=checkpoint)
    assert restore.ok, restore.text()
    page.reload()
    expect(page.locator('#position')).to_contain_text('Исполнено 1 / 3')
    page.locator('#play').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 3 / 3', timeout=10000)
    assert page.request.get('http://127.0.0.1:8080/api/run').json() == full
    # The browser's exact BigInt conversion uses the same module as its input controls.
    for value in ['9007199254740991','9007199254740993',str(2**256-1)]:
        result = page.evaluate('''async v => {
          const {atoms} = await import('/wire.mjs');
          const response = await fetch('/api/wire', {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({amount:atoms(v)})});
          return response.json();
        }''', value)
        assert result['amount'] == value
    exact_price = {'base_asset':'X','quote_asset':'Q','numerator':str(2**256-1),'denominator':'9007199254740993'}
    wire = page.evaluate("""async price => {
      const {atoms} = await import('/wire.mjs');
      const body = {amount:atoms('9007199254740993'), price:{...price,numerator:atoms(price.numerator),denominator:atoms(price.denominator)}};
      return (await fetch('/api/wire',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)})).json();
    }""",exact_price)
    assert wire['price'] == exact_price
    for value in ['01','-1',str(2**256)]:
        response = page.request.post('http://127.0.0.1:8080/api/wire',data={'amount':value})
        assert not response.ok
    page.locator('#amount').fill('200')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 3')
    page.locator('#step').click()
    expect(page.locator('#stats')).to_contain_text('1200')
    expect(page.locator('#comparison')).to_contain_text('100 → 200')
    page.locator('#fork').click()
    expect(page.locator('#error')).to_be_hidden()
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    page.locator('#attempt').click()
    Path('output').mkdir(exist_ok=True)
    page.screenshot(path='output/m1-inspector.png',full_page=True)
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'mobile horizontal overflow'
    page.screenshot(path='output/m1-mobile.png',full_page=True)
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#preset').select_option('sc-lp')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 3')
    page.locator('#step').click()
    expect(page.locator('#positions')).to_contain_text('1100')
    page.locator('#step').click()
    expect(page.locator('#positions')).to_contain_text('1075')
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    expect(page.locator('#positions')).to_contain_text('1075')
    page.locator('#preset').select_option('sc-03-route')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 3')
    page.locator('#step').click()
    expect(page.locator('#decisions')).to_contain_text('+23')
    expect(page.locator('#receipt')).to_contain_text('Success')
    page.locator('#venue').select_option('second')
    expect(page.locator('#stats')).to_contain_text('1090')
    route_state=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert route_state['state']['accounts']['alice']['balances']['Q']=='10023'
    page.locator('#preset').select_option('sc-04-flash')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 3')
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    page.locator('#attempt').click()
    expect(page.locator('#calls')).to_contain_text('FlashCallback')
    flash_state=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert flash_state['state']['accounts']['alice']['balances']['Q']=='22'
    assert flash_state['state']['accounts']['flash']['balances']['Q']=='10001'
    page.screenshot(path='output/m2-flash-success.png',full_page=True)
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    after=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert after['state']['accounts']['alice']['balances']==flash_state['state']['accounts']['alice']['balances']
    assert after['state']['accounts']['flash']==flash_state['state']['accounts']['flash']
    page.screenshot(path='output/m2-flash-rollback.png',full_page=True)
    page.locator('#preset').select_option('sc-05-credit')
    page.locator('#new').click()
    expect(page.locator('#credit')).to_contain_text('NoDebt')
    page.locator('#step').click()
    expect(page.locator('#stats')).to_contain_text('80')
    expect(page.locator('#credit')).to_contain_text('Healthy')
    expect(page.locator('#credit')).to_contain_text('96 / 80')
    page.locator('#transactions button[data-tx="1"]').click()
    page.locator('#attempt').click()
    page.locator('#calls button[data-call="1"]').click()
    expect(page.locator('#detail')).to_contain_text('20 / 1')
    credit_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#step').click()
    expect(page.locator('#credit')).to_contain_text('списано 30 Q; equity 2, вкладчики 28')
    expect(page.locator('#system-events')).to_contain_text('10 atoms Q')
    expect(page.locator('#credit')).to_contain_text('NoDebt')
    after_credit=page.request.get('http://127.0.0.1:8080/api/run').json()
    credit=after_credit['state']['lending']['credit']
    assert (credit['debt'],credit['claims'],credit['equity'],credit['total_shares'])==('0','60','0','75')
    assert after_credit['state']['accounts']['alice']['balances']['Q']=='20'
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    page.locator('#attempt').click()
    expect(page.locator('#detail')).to_contain_text('11 / 1')
    expect(page.locator('#detail')).to_contain_text('BadDebtWrittenOff')
    expect(page.locator('#detail')).to_contain_text('не вошли в состояние')
    page.screenshot(path='output/m2-credit-rollback.png',full_page=True)
    page.locator('#transactions button[data-tx="3"]').click()
    page.locator('#canonical').click()
    expect(page.locator('#detail')).to_contain_text('20 atoms Q')
    page.screenshot(path='output/m2-credit-loss.png',full_page=True)
    page.locator('#blocks button[data-block="0"]').click()
    expect(page.locator('#credit')).to_contain_text('96 / 80')
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==after_credit
    page.locator('#step').click()
    expect(page.locator('#credit')).to_contain_text('Всего deposit shares: 0')
    credit_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert credit_full['state']['accounts']['alice']['balances']['Q']=='80'
    response=page.request.post('http://127.0.0.1:8080/api/restore',data=credit_checkpoint)
    assert response.ok, response.text()
    page.request.post('http://127.0.0.1:8080/api/step',data={})
    page.request.post('http://127.0.0.1:8080/api/step',data={})
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==credit_full
    page.reload()
    expect(page.locator('#credit')).to_contain_text('NoDebt')
    expect(page.locator('#preset')).to_have_value('sc-05-credit')
    expect(page.locator('#sc01-params')).to_be_hidden()
    expect(page.locator('#venue-control')).to_be_hidden()
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'credit mobile horizontal overflow'
    page.screenshot(path='output/m2-credit-mobile.png',full_page=True)
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#preset').select_option('sc-stable')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 3')
    page.locator('#venue').select_option('stable')
    expect(page.locator('#market')).to_contain_text('99.860150')
    expect(page.locator('#market')).to_contain_text('2000000000000000000000')
    initial_stable=page.request.get('http://127.0.0.1:8080/api/run').json()
    for _ in range(3):
        assert page.request.get('http://127.0.0.1:8080/api/market/0/stable').ok
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==initial_stable
    page.screenshot(path='output/m2-stable-curves.png',full_page=True)
    page.locator('#step').click()
    expect(page.locator('#stats')).to_contain_text('900139850')
    expect(page.locator('#receipt')).to_contain_text('Success')
    page.locator('#attempt').click()
    expect(page.locator('#detail')).to_contain_text('Solver:')
    expect(page.locator('#detail')).to_contain_text('D #1')
    expect(page.locator('#detail')).to_contain_text('y #')
    page.screenshot(path='output/m2-stable-solver.png',full_page=True)
    stable_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    after_stable=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert after_stable['state']['accounts']['alice']['balances']['R']=='2099860150'
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('ResourceExhausted')
    stable_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    for name in ['alice','stable-account','cpmm-account']:
        assert stable_full['state']['accounts'][name]['balances']==after_stable['state']['accounts'][name]['balances']
    response=page.request.post('http://127.0.0.1:8080/api/restore',data=stable_checkpoint)
    assert response.ok, response.text()
    page.request.post('http://127.0.0.1:8080/api/step',data={})
    page.request.post('http://127.0.0.1:8080/api/step',data={})
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==stable_full
    page.locator('#preset').select_option('sc-stable-depeg')
    page.locator('#new').click()
    page.locator('#venue').select_option('stable')
    expect(page.locator('#market')).to_contain_text('9179141268556708391244')
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    depeg=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert depeg['state']['accounts']['alice']['balances']['R']=='2554752748'
    page.locator('#attempt').click()
    expect(page.locator('#detail')).to_contain_text('Solver:')
    page.screenshot(path='output/m2-stable-depeg.png',full_page=True)
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'stable mobile horizontal overflow'
    page.screenshot(path='output/m2-stable-mobile.png',full_page=True)
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#preset').select_option('sc-twap')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 5')
    expect(page.locator('#oracle-view')).to_contain_text('NotReady')
    page.locator('#step').click()
    expect(page.locator('#oracle-view')).to_contain_text('Spot сейчас: 200 / 1')
    page.locator('#transactions button[data-tx="2"]').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    page.locator('#step').click()
    expect(page.locator('#oracle-view')).to_contain_text('Точное среднее: 150 / 1')
    expect(page.locator('#oracle-view')).to_contain_text('Опубликованная цена: 150 / 1')
    expect(page.locator('#credit')).to_contain_text('120 / 100')
    page.locator('#transactions button[data-tx="2"]').click()
    page.locator('#attempt').click()
    page.locator('#calls button[data-call="1"]').click()
    expect(page.locator('#detail')).to_contain_text('TWAP · окно 60 s')
    expect(page.locator('#detail')).to_contain_text('150 / 1')
    twap_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    before_queries=page.request.get('http://127.0.0.1:8080/api/run').json()
    for _ in range(3):
        assert page.request.get('http://127.0.0.1:8080/api/oracles/2').ok
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==before_queries
    page.screenshot(path='output/m2-twap-150.png',full_page=True)
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    expect(page.locator('#detail')).to_contain_text('600 / 1')
    expect(page.locator('#oracle-view')).to_contain_text('Spot сейчас: 200 / 1')
    page.locator('#step').click()
    expect(page.locator('#oracle-view')).to_contain_text('Возраст: 60 s')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    page.locator('#step').click()
    expect(page.locator('#oracle-view')).to_contain_text('Возраст: 90 s')
    expect(page.locator('#oracle-view')).to_contain_text('Проекция публикации на текущий момент: 200 / 1')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('oracle stale')
    expect(page.locator('#detail')).to_contain_text('Stale')
    page.screenshot(path='output/m2-twap-stale.png',full_page=True)
    twap_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert twap_full['state']['lending']['credit']['debt']=='100'
    response=page.request.post('http://127.0.0.1:8080/api/restore',data=twap_checkpoint)
    assert response.ok, response.text()
    for _ in range(3):
        page.request.post('http://127.0.0.1:8080/api/step',data={})
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==twap_full
    page.locator('#preset').select_option('sc-twap-120')
    page.locator('#new').click()
    for block in range(1,5):
        page.locator('#step').click()
        expect(page.locator('#position')).to_contain_text(f'Исполнено {block} / 4')
    expect(page.locator('#oracle-view')).to_contain_text('Точное среднее: 175 / 1')
    expect(page.locator('#oracle-view')).to_contain_text('Опубликованная цена: 175 / 1')
    page.screenshot(path='output/m2-twap-175.png',full_page=True)
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'TWAP mobile horizontal overflow'
    page.screenshot(path='output/m2-twap-mobile.png',full_page=True)
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#preset').select_option('sc-07-flash-liquidation')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 3')
    page.locator('#step').click()
    expect(page.locator('#credit')).to_contain_text('Долг: 80 atoms Q')
    composite_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#step').click()
    expect(page.locator('#credit')).to_contain_text('списано 30 Q; equity 2, вкладчики 28')
    expect(page.locator('#credit .stats')).to_contain_text('Cash C')
    expect(page.locator('#credit .stats')).to_contain_text('80')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('insufficient balance')
    page.locator('#attempt').click()
    expect(page.locator('#calls')).to_contain_text('RolledBack')
    page.locator('#calls button[data-call="2"]').click()
    expect(page.locator('#detail')).to_contain_text('11 / 1')
    expect(page.locator('#detail')).to_contain_text('BadDebtWrittenOff')
    page.screenshot(path='output/m2-composite-rollback.png',full_page=True)
    page.locator('#transactions button[data-tx="2"]').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    expect(page.locator('#calls')).to_contain_text('FlashCallback')
    page.locator('#calls button[data-call="0"]').click()
    expect(page.locator('#detail')).to_contain_text('61 atoms Q')
    composite=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert composite['state']['accounts']['carol']['balances']=={'X':'0','Q':'15','R':'0'}
    assert composite['state']['accounts']['flash']['balances']['Q']=='10001'
    assert len(composite['state']['lending']['credit']['losses'])==1
    assert len(composite['state']['price_history']['oracle-price']['segments'])==2
    page.screenshot(path='output/m2-composite-success.png',full_page=True)
    page.locator('#step').click()
    expect(page.locator('#credit')).to_contain_text('Всего deposit shares: 75')
    composite_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert composite_full['state']['accounts']['reserve']['balances']['Q']=='60'
    response=page.request.post('http://127.0.0.1:8080/api/restore',data=composite_checkpoint)
    assert response.ok, response.text()
    for _ in range(2):
        page.request.post('http://127.0.0.1:8080/api/step',data={})
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==composite_full
    page.reload()
    expect(page.locator('#preset')).to_have_value('sc-07-flash-liquidation')
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'composite mobile horizontal overflow'
    page.screenshot(path='output/m2-composite-mobile.png',full_page=True)
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#preset').select_option('sc-08-book')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 6')
    expect(page.locator('#book-view')).to_contain_text('Пусто')
    page.locator('#step').click()
    expect(page.locator('#book-view')).to_contain_text('#1 alice · 2 lots · locked 20 X')
    expect(page.locator('#book-view')).to_contain_text('#2 bob · 3 lots · locked 30 X')
    before_quote=page.request.get('http://127.0.0.1:8080/api/run').json()
    page.locator('#book-panel details > summary').click()
    page.locator('#book-quote-form button').click()
    expect(page.locator('#book-quote-result')).to_contain_text('Уплачено: 425 Q')
    expect(page.locator('#book-quote-result')).to_contain_text('возврат: 23')
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==before_quote
    book_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#book-lots').fill('6')
    page.locator('#book-tif').select_option('FOK')
    page.locator('#book-quote-form button').click()
    expect(page.locator('#book-quote-result')).to_contain_text('FOK not fully fillable')
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==before_quote
    page.screenshot(path='output/m3-book-quote.png',full_page=True)
    page.locator('#step').click()
    page.locator('#transactions button[data-tx="0"]').click()
    expect(page.locator('#receipt')).to_contain_text('FOK not fully fillable')
    page.locator('#attempt').click()
    expect(page.locator('#detail')).to_contain_text('Показанные переводы не вошли')
    expect(page.locator('#detail')).to_contain_text('Gross: 530 Q')
    page.locator('#transactions button[data-tx="2"]').click()
    expect(page.locator('#detail')).to_contain_text('Уплачено: 425 Q')
    expect(page.locator('#book-view')).to_contain_text('#2 bob · 1 lots · locked 10 X')
    page.screenshot(path='output/m3-book-fills.png',full_page=True)
    page.locator('#step').click()
    expect(page.locator('#book-view')).to_contain_text('#4 alice · 4 lots · locked 440 Q')
    page.locator('#transactions button[data-tx="3"]').click()
    expect(page.locator('#detail')).to_contain_text('Выручено: 217 Q')
    page.locator('#step').click()
    expect(page.locator('#book-view')).to_contain_text('#7 carol · 1 lots · locked 120 Q')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#detail')).to_contain_text('Partial')
    expect(page.locator('#detail')).to_contain_text('Стоящая часть: 1 lots')
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'book mobile horizontal overflow'
    page.locator('#book-panel').scroll_into_view_if_needed()
    page.screenshot(path='output/m3-book-mobile.png')
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('SelfTrade')
    page.locator('#step').click()
    page.locator('#transactions button[data-tx="2"]').click()
    expect(page.locator('#receipt')).to_contain_text('ResourceExhausted')
    book_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert book_full['state']['books']['book']['orders']=={}
    assert book_full['state']['books']['book']['next_id']=='10'
    assert book_full['state']['accounts']['treasury']['balances']['Q']=='16'
    response=page.request.post('http://127.0.0.1:8080/api/restore',data=book_checkpoint)
    assert response.ok, response.text()
    for _ in range(5):
        assert page.request.post('http://127.0.0.1:8080/api/step',data={}).ok
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==book_full
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#preset').select_option('sc-02-clmm')
    page.locator('#new').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 0 / 5')
    page.locator('#step').click()
    expect(page.locator('#clmm-view')).to_contain_text('#1 alice · Active')
    expect(page.locator('#clmm-view')).to_contain_text('1000000')
    if not page.locator('#clmm-input').is_visible():
        page.locator('#clmm-panel > details > summary').click()
    before_clmm_quote=page.request.get('http://127.0.0.1:8080/api/run').json()
    page.locator('#clmm-quote-form button').click()
    expect(page.locator('#clmm-quote-result')).to_contain_text('Выход: 996 Q · fee: 3 X')
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==before_clmm_quote
    clmm_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    page.locator('#attempt').click()
    expect(page.locator('#detail')).to_contain_text('не вошли в состояние')
    expect(page.locator('#detail')).to_contain_text('Выход: 996 Q')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    view=page.request.get('http://127.0.0.1:8080/api/clmm/2').json()['pools'][0]
    assert view['positions'][0]['collectable']==['2','0']
    assert view['rounding_surplus']==['2','1']
    page.screenshot(path='output/m3-clmm-position.png',full_page=True)
    page.locator('#step').click()
    expect(page.locator('#clmm-view')).to_contain_text('Открытых позиций нет')
    page.locator('#calls button[data-call="1"]').click()
    expect(page.locator('#detail')).to_contain_text('Расчёт: 2 X + 0 Q')
    state=page.request.get('http://127.0.0.1:8080/api/run').json()['state']
    assert state['accounts']['clmm-account']['balances']=={'X':'2','Q':'1'}
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('NoExecutableOutput')
    expect(page.locator('#clmm-view')).to_contain_text('#2 alice · Active')
    after_bootstrap=page.request.get('http://127.0.0.1:8080/api/run').json()['state']
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ResourceExhausted')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    clmm_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert clmm_full['state']['clmm']==after_bootstrap['clmm']
    assert clmm_full['state']['accounts']['clmm-account']==after_bootstrap['accounts']['clmm-account']
    response=page.request.post('http://127.0.0.1:8080/api/restore',data=clmm_checkpoint)
    assert response.ok, response.text()
    for _ in range(4):
        assert page.request.post('http://127.0.0.1:8080/api/step',data={}).ok
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==clmm_full
    page.locator('#preset').select_option('sc-clmm-crossing')
    page.locator('#new').click()
    page.locator('#step').click()
    page.locator('#step').click()
    expect(page.locator('#detail')).to_contain_text('Выход: 15755 Q')
    expect(page.locator('#detail')).to_contain_text('crossing -100')
    expect(page.locator('#detail')).to_contain_text('crossing -200')
    expect(page.locator('#clmm-view')).to_contain_text('#3 carol · Active')
    expect(page.locator('#clmm-view')).to_contain_text('#1 alice · Inactive')
    if not page.locator('#clmm-input').is_visible():
        page.locator('#clmm-panel > details > summary').click()
    page.locator('#clmm-input').select_option('Q')
    page.locator('#clmm-amount').fill('50000')
    page.locator('#clmm-limit').fill('100')
    page.locator('#clmm-quote-form button').click()
    expect(page.locator('#clmm-quote-result')).to_contain_text('CLMM partial fill disabled')
    before_partial_quote=page.request.get('http://127.0.0.1:8080/api/run').json()
    page.locator('#clmm-partial').select_option('true')
    page.locator('#clmm-quote-form button').click()
    expect(page.locator('#clmm-quote-result')).to_contain_text('Success')
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==before_partial_quote
    page.screenshot(path='output/m3-clmm-crossing.png',full_page=True)
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'CLMM mobile horizontal overflow'
    page.locator('#clmm-panel').evaluate("el=>el.scrollIntoView({block:'start'})")
    page.screenshot(path='output/m3-clmm-mobile.png')
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('CLMM partial fill disabled')
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#receipt')).to_contain_text('Success')
    partial=page.request.get('http://127.0.0.1:8080/api/run').json()['blocks'][2]['receipts'][1]['trace'][0]['clmm']
    assert int(partial['remainder'])>0
    assert int(partial['spent'])+int(partial['remainder'])==50000
    page.locator('#step').click()
    expect(page.locator('#clmm-view')).to_contain_text('Открытых позиций нет')
    page.reload()
    expect(page.locator('#preset')).to_have_value('sc-clmm-crossing')
    page.locator('#preset').select_option('sc-03-heterogeneous')
    page.locator('#new').click()
    expect(page.locator('#route-panel')).to_be_visible()
    page.locator('#step').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 1 / 4')
    page.locator('#route-input').select_option('X')
    page.locator('#route-output').select_option('R')
    page.locator('#route-amount').fill('35')
    page.locator('#route-residual').select_option('KeepResiduals')
    page.locator('#route-limits input[data-include="0"]').check()
    before_route_query=page.request.get('http://127.0.0.1:8080/api/run').json()
    page.locator('#route-quote-form button').click()
    expect(page.locator('#route-quote-result')).to_contain_text('Проверено кандидатов: 2 / 64')
    expect(page.locator('#route-quote-result')).to_contain_text('194 R')
    expect(page.locator('#route-quote-result')).to_contain_text('15 X')
    expect(page.locator('#route-quote-result')).to_contain_text('165 R')
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==before_route_query
    page.locator('#route-panel').screenshot(path='output/m3-route-search.png')
    page.locator('#route-residual').select_option('RequireFull')
    page.locator('#route-quote-form button').click()
    expect(page.locator('#route-quote-result')).to_contain_text('route requires full input consumption')
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==before_route_query
    page.locator('#step').click()
    expect(page.locator('#receipt')).to_contain_text('ProtocolRevert')
    page.locator('#transactions button[data-tx="2"]').click()
    page.locator('#canonical').click()
    expect(page.locator('#detail')).to_contain_text('165 R')
    expect(page.locator('#detail')).to_contain_text('Остатки в кошельке: 15 X')
    route_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#step').click()
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#detail')).to_contain_text('3327 R')
    expect(page.locator('#detail')).to_contain_text('4971 X')
    page.screenshot(path='output/m3-route-partial.png',full_page=True)
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'route mobile horizontal overflow'
    page.locator('#detail').evaluate("el=>el.scrollIntoView({block:'start'})")
    page.screenshot(path='output/m3-route-mobile.png')
    page.set_viewport_size({'width':1440,'height':1100})
    page.locator('#step').click()
    page.locator('#transactions button[data-tx="1"]').click()
    expect(page.locator('#detail')).to_contain_text('stable')
    route_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert len(route_full['blocks'][3]['receipts'][1]['route_results']['0']['hops'])==3
    assert page.request.post('http://127.0.0.1:8080/api/restore',data=route_checkpoint).ok
    page.reload()
    expect(page.locator('#preset')).to_have_value('sc-03-heterogeneous')
    page.locator('#play').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 4 / 4',timeout=10000)
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==route_full
    page.locator('#preset').select_option('sc-lp-policies')
    page.locator('#new').click()
    expect(page.locator('#policy-panel')).to_be_visible()
    expect(page.locator('#policy-config')).to_contain_text('Блок 3: Диапазон [-200, -50)')
    page.locator('#step').click()
    expect(page.locator('#decisions')).to_contain_text('alice · Submit')
    expect(page.locator('#clmm-view')).to_contain_text('2000000')
    page.locator('#step').click()
    expect(page.locator('#clmm-view')).to_contain_text('Inactive')
    page.locator('#step').click()
    expect(page.locator('#decisions')).to_contain_text('alice · Hold')
    expect(page.locator('#decisions')).to_contain_text('bob · Submit')
    expect(page.locator('#clmm-view')).to_contain_text('#3 bob')
    page.screenshot(path='output/m3-lp-policies.png',full_page=True)
    lp_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#play').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 5 / 5',timeout=10000)
    lp_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert all(v['owner']=='alice' for v in lp_full['state']['clmm']['clmm']['positions'].values())
    assert page.request.post('http://127.0.0.1:8080/api/restore',data=lp_checkpoint).ok
    page.reload()
    expect(page.locator('#preset')).to_have_value('sc-lp-policies')
    page.locator('#play').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 5 / 5',timeout=10000)
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==lp_full
    page.locator('#preset').select_option('sc-mm-oracle')
    page.locator('#new').click()
    expect(page.locator('#policy-config')).to_contain_text('Публичный oracle public-price')
    page.locator('#step').click()
    expect(page.locator('#book-view')).to_contain_text('180')
    expect(page.locator('#decisions')).to_contain_text('Anchor: 50 ticks')
    page.locator('#step').click()
    expect(page.locator('#decisions')).to_contain_text('alice · Hold')
    expect(page.locator('#decisions')).to_contain_text('Anchor: 50 ticks')
    page.locator('#step').click()
    expect(page.locator('#book-view')).to_contain_text('220')
    expect(page.locator('#decisions')).to_contain_text('Anchor: 60 ticks')
    page.locator('#step').click()
    expect(page.locator('#decisions')).to_contain_text('carol · Submit')
    expect(page.locator('#decisions')).to_contain_text('+1 atoms X')
    expect(page.locator('#decisions')).to_contain_text('Остатки в кошельке: нет')
    page.locator('#decisions').screenshot(path='output/m3-mm-decisions.png')
    mm_checkpoint=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
    page.locator('#step').click()
    expect(page.locator('#decisions')).to_contain_text('возраст 2 / 2 s')
    page.locator('#step').click()
    expect(page.locator('#decisions')).to_contain_text('MM missing/stale oracle: cancel quotes')
    expect(page.locator('#decisions')).to_contain_text('возраст 3 / 2 s')
    mm_full=page.request.get('http://127.0.0.1:8080/api/run').json()
    assert mm_full['state']['books']['book']['orders']=={}
    page.set_viewport_size({'width':390,'height':844})
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'policy mobile horizontal overflow'
    page.locator('#decisions').evaluate("el=>el.scrollIntoView({block:'start'})")
    page.screenshot(path='output/m3-mm-mobile.png')
    page.set_viewport_size({'width':1440,'height':1100})
    assert page.request.post('http://127.0.0.1:8080/api/restore',data=mm_checkpoint).ok
    page.reload()
    expect(page.locator('#preset')).to_have_value('sc-mm-oracle')
    page.locator('#play').click()
    expect(page.locator('#position')).to_contain_text('Исполнено 6 / 6',timeout=10000)
    assert page.request.get('http://127.0.0.1:8080/api/run').json()==mm_full
    # DEC-21: real delivery, visibility, canonical execution and pending checkpoint state.
    for name in ['public','private','delayed','nonce','fifo','fee']:
        page.locator('#preset').select_option('sc-scheduler-'+name)
        page.locator('#new').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 0 / 3')
        expect(page.locator('#scheduler-panel')).to_be_visible()
        page.locator('#step').click()
        expect(page.locator('#scheduler-summary')).to_contain_text('cutoff 10 s')
        first=page.request.get('http://127.0.0.1:8080/api/run').json()
        if name=='public':
            expect(page.locator('#scheduler-result')).to_contain_text('10 s · Admitted')
            expect(page.locator('#scheduler-result')).to_contain_text('8 s · Visible')
            assert first['state']['accounts']['alice']['balances']['Q']=='10016'
            page.locator('#scheduler-panel').screenshot(path='output/m4-public-delivery.png')
        elif name=='private':
            assert not first['blocks'][0].get('decisions')
            expect(page.locator('#scheduler-result')).to_contain_text('Private → только builder')
            assert first['state']['accounts']['alice']['balances']['Q']=='10000'
        elif name=='delayed':
            expect(page.locator('#scheduler-result')).to_contain_text('due 11 s')
            expect(page.locator('#scheduler-result')).to_contain_text('InFlight')
            assert len(first['scheduler']['queue'])==1
        else:
            expect(page.locator('#scheduler-result')).to_contain_text('Included')
            assert first['state']['accounts']['alice']['balances']['Q']=={'nonce':'740','fifo':'875','fee':'760'}[name]
        saved=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
        # Viewing the old snapshot must not deliver messages or advance the scheduler.
        page.locator('#blocks button[data-block="-1"]').click()
        expect(page.locator('#scheduler-result')).to_contain_text('Начальное состояние')
        assert page.request.get('http://127.0.0.1:8080/api/run').json()==first
        page.locator('#blocks button[data-block="0"]').click()
        page.locator('#play').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 3 / 3',timeout=10000)
        full=page.request.get('http://127.0.0.1:8080/api/run').json()
        assert page.request.post('http://127.0.0.1:8080/api/restore',data=saved).ok
        page.reload()
        expect(page.locator('#preset')).to_have_value('sc-scheduler-'+name)
        if name=='delayed':
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'scheduler mobile overflow'
            page.locator('#scheduler-panel').evaluate("el=>el.scrollIntoView({block:'start'})")
            page.screenshot(path='output/m4-delayed-mobile.png')
            page.set_viewport_size({'width':1440,'height':1100})
            page.locator('#scheduler-panel').screenshot(path='output/m4-delayed-queue.png')
        page.locator('#play').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 3 / 3',timeout=10000)
        assert page.request.get('http://127.0.0.1:8080/api/run').json()==full
    # DEC-22: actual selected groups versus unpaid diagnostic probes.
    for name in ['allowed','forbidden','greedy','fee','visible','private']:
        page.locator('#preset').select_option('sc-bundle-'+name)
        page.locator('#new').click()
        expect(page.locator('#bundle-panel')).to_be_visible()
        expect(page.locator('#bundle-config')).to_contain_text('Score = native fees')
        page.locator('#step').click()
        first=page.request.get('http://127.0.0.1:8080/api/run').json()
        groups=first['blocks'][0]['scheduler']['probes']
        if name in ['allowed','greedy']:
            expect(page.locator('#bundle-result')).to_contain_text('355 + 50')
            expect(page.locator('#bundle-result')).to_contain_text('ProtocolRevert')
            assert first['state']['accounts']['builder']['balances']['Q']=='405'
            assert len(first['blocks'][0]['receipts'])==3
            if name=='greedy':
                page.locator('#bundle-panel').screenshot(path='output/m4b-greedy.png')
        elif name=='forbidden':
            assert first['blocks'][0]['receipts']==[]
            assert first['state']['accounts']['builder']['balances']['Q']=='0'
            assert not any(p['selected'] for p in groups)
            expect(page.locator('#bundle-result')).to_contain_text('DisallowedRevert')
            expect(page.locator('#bundle-result')).to_contain_text('не вошли в канонический блок')
        elif name=='fee':
            assert first['state']['accounts']['builder']['balances']['Q']=='345'
            assert first['blocks'][0]['receipts'][0]['tx_id']=='ordinary'
            expect(page.locator('#bundle-result')).to_contain_text('345 + 0')
        elif name=='visible':
            assert first['state']['accounts']['alice']['balances']['Q']=='10011'
            assert first['state']['accounts']['builder']['balances']['Q']=='5'
            assert [r['tx_id'] for r in first['blocks'][0]['receipts']]==['victim','response','payment']
            expect(page.locator('#bundle-result')).to_contain_text('0 + 5')
        else:
            expect(page.locator('#bundle-result')).to_contain_text('UnknownVisibleTxId')
            assert len(first['blocks'][0]['receipts'])==1
            assert first['state']['accounts']['alice']['balances']['Q']=='10000'
        saved=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
        page.locator('#blocks button[data-block="-1"]').click()
        expect(page.locator('#bundle-result')).to_contain_text('Запустите блок')
        assert page.request.get('http://127.0.0.1:8080/api/checkpoint').json()==saved
        page.locator('#blocks button[data-block="0"]').click()
        if name=='forbidden':
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'bundle mobile overflow'
            page.locator('#bundle-panel').evaluate("el=>el.scrollIntoView({block:'start'})")
            page.screenshot(path='output/m4b-forbidden-mobile.png')
            page.set_viewport_size({'width':1440,'height':1100})
            page.locator('#bundle-panel').screenshot(path='output/m4b-forbidden.png')
        rounds=first['scenario']['scheduler']['rounds']
        if rounds>1:
            page.locator('#play').click()
            expect(page.locator('#position')).to_contain_text(f'Исполнено {rounds} / {rounds}',timeout=10000)
        full=page.request.get('http://127.0.0.1:8080/api/run').json()
        assert page.request.post('http://127.0.0.1:8080/api/restore',data=saved).ok
        page.reload()
        expect(page.locator('#preset')).to_have_value('sc-bundle-'+name)
        if rounds>1:
            page.locator('#play').click()
            expect(page.locator('#position')).to_contain_text(f'Исполнено {rounds} / {rounds}',timeout=10000)
        assert page.request.get('http://127.0.0.1:8080/api/run').json()==full
    # DEC-23: conditional search is separate from canonical builder inclusion.
    for name in ['public','private','delayed','fifo','competition','fees']:
        page.locator('#preset').select_option('sc-backrun-'+name)
        page.locator('#new').click()
        expect(page.locator('#backrun-panel')).to_be_visible()
        page.locator('#step').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 1 / 2')
        first=page.request.get('http://127.0.0.1:8080/api/run').json()
        report=first['blocks'][0]['scheduler']['search']
        expected_alice={'public':'10011','private':'10000','delayed':'10000','fifo':'10000','competition':'10000','fees':'1017752'}[name]
        expected_builder={'public':'5','private':'0','delayed':'0','fifo':'0','competition':'7','fees':'280'}[name]
        assert first['state']['accounts']['alice']['balances']['Q']==expected_alice
        assert first['state']['accounts']['builder']['balances']['Q']==expected_builder
        expect(page.locator('#backrun-result')).to_contain_text('Расходы поиска не списаны')
        if name=='private':
            assert report['metrics']['candidates']==0 and not report['decisions']
            expect(page.locator('#backrun-result')).to_contain_text('Поисковых реакций в этом блоке нет')
        else:
            expect(page.locator('#backrun-result')).to_contain_text('Условный net:')
            for d in report['decisions']:
                best=d['candidates'][d['selected']]
                assert best['input']==('100000' if name=='fees' else '100')
        if name=='competition':
            assert first['state']['accounts']['carol']['balances']['Q']=='10009'
            assert report['metrics']['candidates']==20
            page.locator('#backrun-panel').screenshot(path='output/m4c-competition.png')
        saved=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
        page.locator('#blocks button[data-block="-1"]').click()
        expect(page.locator('#backrun-result')).to_contain_text('Запустите блок')
        assert page.request.get('http://127.0.0.1:8080/api/checkpoint').json()==saved
        page.locator('#blocks button[data-block="0"]').click()
        if name=='fees':
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'backrun mobile overflow'
            page.locator('#backrun-panel').evaluate("el=>el.scrollIntoView({block:'start'})")
            page.screenshot(path='output/m4c-fees-mobile.png')
            page.set_viewport_size({'width':1440,'height':1100})
            page.locator('#backrun-panel').screenshot(path='output/m4c-fees.png')
        page.locator('#play').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 2 / 2',timeout=10000)
        full=page.request.get('http://127.0.0.1:8080/api/run').json()
        assert page.request.post('http://127.0.0.1:8080/api/restore',data=saved).ok
        page.reload()
        expect(page.locator('#preset')).to_have_value('sc-backrun-'+name)
        page.locator('#play').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 2 / 2',timeout=10000)
        assert page.request.get('http://127.0.0.1:8080/api/run').json()==full
    # DEC-24: actual seizure, Own/Flash, paid losing attempt, preview age and delayed inclusion.
    for name in ['own','flash','fifo','fee','stale','delayed']:
        page.locator('#preset').select_option('sc-liquidator-'+name)
        page.locator('#new').click()
        expect(page.locator('#liquidator-panel')).to_be_visible()
        for n in range(1,4):
            page.locator('#step').click()
            expect(page.locator('#position')).to_contain_text(f'Исполнено {n} / 4')
        first=page.request.get('http://127.0.0.1:8080/api/run').json()
        report=first['blocks'][2]['scheduler']['liquidators']
        expect(page.locator('#liquidator-result')).to_contain_text('Расходы поиска не списаны')
        if name=='stale':
            expect(page.locator('#liquidator-result')).to_contain_text('StaleOracle')
            assert report['metrics']['trials']==0
            assert first['state']['lending']['credit']['debt']=='80'
        else:
            expect(page.locator('#liquidator-result')).to_contain_text('Условный net:')
            expect(page.locator('#liquidator-result')).to_contain_text('20 s → preview 3 / 30 s')
            if name!='delayed':
                expect(page.locator('#liquidator-result')).to_contain_text('Фактически получено залога:')
                page.locator('#transactions button[data-tx="0"]').click()
                page.locator('#attempt').click()
                index=1 if name=='flash' else 0
                if name=='flash':
                    traces=first['blocks'][2]['receipts'][0]['trace']
                    index=next(i for i,t in enumerate(traces) if t['operation']=='LiquidateAndSell')
                page.locator(f'#calls button[data-call="{index}"]').click()
                expect(page.locator('#detail')).to_contain_text('Фактически погашено:')
                expect(page.locator('#detail')).to_contain_text('Observed at 20')
            if name in ['fifo','fee']:
                expect(page.locator('#liquidator-result')).to_contain_text('NoDebt')
                assert first['state']['accounts']['builder']['balances']['Q']==('410' if name=='fifo' else '460')
                if name=='fee':
                    page.locator('#liquidator-panel').screenshot(path='output/m4d-competition.png')
            if name=='flash':
                expect(page.locator('#liquidator-result')).to_contain_text('24675')
                page.set_viewport_size({'width':390,'height':844})
                assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'liquidator mobile overflow'
                page.locator('#liquidator-panel').evaluate("el=>el.scrollIntoView({block:'start'})")
                page.screenshot(path='output/m4d-flash-mobile.png')
                page.set_viewport_size({'width':1440,'height':1100})
        saved=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
        page.locator('#blocks button[data-block="-1"]').click()
        expect(page.locator('#liquidator-result')).to_contain_text('Запустите блок')
        assert page.request.get('http://127.0.0.1:8080/api/checkpoint').json()==saved
        page.locator('#step').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 4 / 4')
        full=page.request.get('http://127.0.0.1:8080/api/run').json()
        expected={'own':'1024','flash':'25675','fifo':'99760','fee':'124565','stale':'1000','delayed':'1024'}[name]
        assert full['state']['accounts']['carol']['balances']['Q']==expected
        if name=='delayed':
            expect(page.locator('#liquidator-result')).to_contain_text('BusyNonce')
            expect(page.locator('#liquidator-result')).to_contain_text('Фактически погашено:')
        assert page.request.post('http://127.0.0.1:8080/api/restore',data=saved).ok
        page.reload()
        expect(page.locator('#preset')).to_have_value('sc-liquidator-'+name)
        page.locator('#step').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 4 / 4')
        assert page.request.get('http://127.0.0.1:8080/api/run').json()==full
    # DEC-25: hook capabilities/state, actual payout, rollback and route output separation.
    for name in ['fee','limit','rollback','shortfall','lp','route']:
        page.locator('#preset').select_option('sc-hook-'+name)
        page.locator('#new').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 0 / 2')
        expect(page.locator('#hook-panel')).to_be_visible()
        expect(page.locator('#hook-config')).to_contain_text('Counter: 0')
        if name=='fee':
            before_quote=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
            page.locator('#route-owner').select_option('alice')
            page.locator('#route-input').select_option('X')
            page.locator('#route-output').select_option('Q')
            page.locator('#route-amount').fill('100')
            page.locator('#route-minimum').fill('1')
            page.locator('#route-resource').fill('500')
            page.locator('#route-price').fill('1')
            page.locator('#route-quote-form button').click()
            expect(page.locator('#route-quote-result')).to_contain_text('205 units')
            expect(page.locator('#route-quote-result')).to_contain_text('82 Q')
            expect(page.locator('#route-quote-result')).to_contain_text('Hook guard')
            assert page.request.get('http://127.0.0.1:8080/api/checkpoint').json()==before_quote
        page.locator('#step').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 1 / 2')
        first=page.request.get('http://127.0.0.1:8080/api/run').json()
        q=first['blocks'][0]['receipts'][0]
        expected={'fee':('Success','195','890','2','17'),'limit':('ProtocolRevert','140','860','0','20'),'rollback':('ProtocolRevert','205','795','0','20'),'shortfall':('ProtocolRevert','195','805','0','2'),'lp':('Success','175','728','2','17'),'route':('Success','225','778','2','17')}[name]
        assert (q['status'],q['used'],first['state']['accounts']['alice']['balances']['Q'],first['state']['hooks']['guard']['counter'],first['state']['accounts']['hook']['balances']['Q'])==expected
        expect(page.locator('#hook-result')).to_contain_text(expected[0])
        expect(page.locator('#hook-result')).to_contain_text(expected[1]+' units')
        if expected[0]!='Success':
            expect(page.locator('#hook-result')).to_contain_text('Hook effects этой попытки отменены')
            assert not any(e['kind'].startswith('Hook') for e in q['events'])
        else:
            page.locator('#attempt').click()
            index=next(i for i,t in enumerate(q['trace']) if t.get('hook'))
            page.locator(f'#calls button[data-call="{index}"]').click()
            expect(page.locator('#detail')).to_contain_text('Hook guard')
            if name=='route':
                assert q['route_results']['0']['hops'][1]['budget']=='82'
                assert first['state']['accounts']['alice']['balances']['R']=='75'
        if name=='fee':
            page.locator('#hook-panel').screenshot(path='output/m4e-hook-fee.png')
        if name=='rollback':
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'hook mobile overflow'
            page.locator('#hook-panel').evaluate("el=>el.scrollIntoView({block:'start'})")
            page.screenshot(path='output/m4e-hook-rollback-mobile.png')
            page.set_viewport_size({'width':1440,'height':1100})
        saved=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
        page.locator('#blocks button[data-block="-1"]').click()
        expect(page.locator('#hook-config')).to_contain_text('Counter: 0')
        assert page.request.get('http://127.0.0.1:8080/api/checkpoint').json()==saved
        page.locator('#step').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 2 / 2')
        full=page.request.get('http://127.0.0.1:8080/api/run').json()
        if name=='lp':
            assert full['state']['accounts']['alice']['balances']['Q']=='656'
            expect(page.locator('#hook-config')).to_contain_text('Counter: 4')
        assert page.request.post('http://127.0.0.1:8080/api/restore',data=saved).ok
        page.reload()
        expect(page.locator('#preset')).to_have_value('sc-hook-'+name)
        page.locator('#step').click()
        expect(page.locator('#position')).to_contain_text('Исполнено 2 / 2')
        assert page.request.get('http://127.0.0.1:8080/api/run').json()==full
    # DEC-26: sampled requests, preflight vs paid receipt, borrower targets and native publisher.
    for name in ['random','scripted','preflight','borrower','spot','twap']:
        rounds={'random':6,'scripted':3,'preflight':3,'borrower':6,'spot':2,'twap':2}[name]
        page.locator('#preset').select_option('sc-agent-'+name)
        page.locator('#new').click()
        expect(page.locator('#position')).to_contain_text(f'Исполнено 0 / {rounds}')
        expect(page.locator('#agent-panel')).to_be_visible()
        page.locator('#step').click()
        expect(page.locator('#position')).to_contain_text(f'Исполнено 1 / {rounds}')
        first=page.request.get('http://127.0.0.1:8080/api/run').json()
        d=first['blocks'][0]['scheduler']['baselines'][0]
        expected={'random':('Submit','PreflightSuccess'),'scripted':('Submit','SubmitPredictedFailure'),'preflight':('Skip','Preflight: scripted late failure'),'borrower':('Submit','PreflightSuccess'),'spot':('Submit','PreflightSuccess'),'twap':('Skip','Preflight: NotReady: insufficient history')}[name]
        assert (d['action'],d['reason'])==expected
        expect(page.locator('#agent-result')).to_contain_text(expected[1])
        if name=='random':
            assert d['selection']['gate']['value']=='41542' and d['selection']['position_after']=='4'
            expect(page.locator('#agent-result')).to_contain_text('template 1')
            page.locator('#agent-panel').screenshot(path='output/m4f-random.png')
        if name=='scripted':
            expect(page.locator('#agent-result')).to_contain_text('оплачено 125 Q')
            assert first['state']['accounts']['bob']['balances']['Q']=='0'
        if name in ['preflight','twap']:
            assert not first['blocks'][0]['receipts']
            expect(page.locator('#agent-result')).to_contain_text('канонические комиссии не списаны')
        saved=page.request.get('http://127.0.0.1:8080/api/checkpoint').json()
        for _ in range(2):
            page.locator('#blocks button[data-block="-1"]').click()
            expect(page.locator('#agent-result')).to_contain_text('Выполните раунд')
            page.locator('#blocks button[data-block="0"]').click()
            expect(page.locator('#agent-result')).to_contain_text(expected[1])
        assert page.request.get('http://127.0.0.1:8080/api/checkpoint').json()==saved
        for i in range(2,rounds+1):
            page.locator('#step').click()
            expect(page.locator('#position')).to_contain_text(f'Исполнено {i} / {rounds}')
        full=page.request.get('http://127.0.0.1:8080/api/run').json()
        account='bob' if name=='borrower' else 'oracle' if name in ['spot','twap'] else 'alice'
        balance={'random':'730','scripted':'740','preflight':'865','borrower':'540','spot':'780','twap':'889'}[name]
        assert full['state']['accounts'][account]['balances']['Q']==balance
        if name=='borrower':
            page.locator('#blocks button[data-block="3"]').click()
            expect(page.locator('#agent-result')).to_contain_text('Repay')
            expect(page.locator('#agent-result')).to_contain_text('debt 80')
            page.set_viewport_size({'width':390,'height':844})
            assert page.evaluate('document.documentElement.scrollWidth <= innerWidth'), 'agent mobile overflow'
            page.locator('#agent-panel').evaluate("el=>el.scrollIntoView({block:'start'})")
            page.screenshot(path='output/m4f-borrower-mobile.png')
            page.set_viewport_size({'width':1440,'height':1100})
        assert page.request.post('http://127.0.0.1:8080/api/restore',data=saved).ok
        page.reload()
        expect(page.locator('#preset')).to_have_value('sc-agent-'+name)
        for i in range(2,rounds+1):
            page.locator('#step').click()
            expect(page.locator('#position')).to_contain_text(f'Исполнено {i} / {rounds}')
        assert page.request.get('http://127.0.0.1:8080/api/run').json()==full
    assert not errors, errors
    browser.close()
print('PASS: browser run, trace, read-only viewing, checkpoint/resume, A/B, fork, exact U256 HTTP round-trip, mobile layout, LP, route policy, flash success/shortfall/rollback, credit interest/loss/redemption, exact oracle use, credit checkpoint replay, StableSwap curves/solver/LP/route/replay/depeg/mobile, TWAP 150/175/rollback/stale/readonly/checkpoint/mobile, flash liquidation real legs/loss/shortfall/rollback/checkpoint/mobile, order book levels/quote/fills/escrow/FOK/IOC/GTC/cancel/self-trade/rollback/replay/mobile, CLMM ranges/fees/surplus/crossings/partial/quote/rollback/replay/mobile, heterogeneous routes/limits/search/strict/residuals/readonly/rollback/replay/mobile, LP/MM policies/schedules/observation-delay/anchor-age/closed-cycle/stale-cancel/replay/mobile; M4 delivery/cutoff/Public/Private/late-response/nonce-gap/FIFO/FeePriority/readonly/checkpoint/mobile; bundles/greedy/paid-bid/allowed-and-forbidden-revert/visible-reference/private-isolation/diagnostic-probes/replay/mobile; backrun/public/private/grid/fees/bid/competition/fifo/delay/readonly/checkpoint/mobile; liquidator/Own/Flash/actual-seizure/paid-loser/FIFO/FeePriority/stale/delayed/readonly/checkpoint/mobile; hooks/dynamic-fee/input-limit/cash-shortfall/LP/route-output/payout/rollback/resource/readonly-quote/checkpoint/mobile; baseline-agents/scripted/random/RNG/paid-failure/skip/borrower-target/native-spot-twap/readonly/checkpoint/mobile')
