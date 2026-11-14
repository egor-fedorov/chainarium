"""Actual Chromium import/download/reimport using final 1d4e3a2 JS files.

The HTTP process is 4728abf, whose Rust sources are byte-identical to 1d4e3a2.
Only app.js/experiments.js responses are supplied from the final isolated archive.
"""
import hashlib
import json
from pathlib import Path
from playwright.sync_api import sync_playwright,expect

SRC=Path('/tmp/chainarium-dec29-fix-1d4e3a2')
OLD=Path('/tmp/chainarium-dec29-fix-evidence-4728abf')
OUT=Path('/tmp/chainarium-dec29-fix-evidence-1d4e3a2');OUT.mkdir(exist_ok=True)
BASE='http://127.0.0.1:8765'
def digest(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
for p in (SRC/'crates').rglob('*'):
    if p.is_file():assert p.read_bytes()==(Path('/tmp/chainarium-dec29-fix-4728abf')/p.relative_to(SRC)).read_bytes(),p
results=[]
with sync_playwright() as p:
    browser=p.chromium.launch(executable_path='/usr/bin/chromium',headless=True,args=['--no-sandbox'])
    page=browser.new_page(viewport={'width':1200,'height':900})
    for name in ['app.js','experiments.js']:
        page.route(BASE+'/'+name,lambda route,request,name=name:route.fulfill(path=SRC/'web'/name,content_type='text/javascript'))
    errors=[];page.on('pageerror',lambda e:errors.append(str(e)))
    page.goto(BASE)
    expect(page.locator('#position')).to_contain_text('Исполнено')
    before=page.request.get(BASE+'/api/run').json()
    # Hide layout while testing a 32MiB hypothesis label; all real handlers, DOM
    # updates, financial report rendering, network requests and downloads execute.
    page.add_style_tag(content='body{display:none !important}')
    for kind,file in [('near-limit',OLD/'near-limit/manifest.json'),('normal',OLD/'builder/manifest.json')]:
        with page.expect_response(lambda r:r.url==BASE+'/api/experiment-report' and r.request.method=='POST',timeout=180000) as uploaded:
            page.locator('#experiment-import').set_input_files(file)
        assert uploaded.value.status==200
        page.wait_for_function("document.querySelector('#experiment-name').textContent==='Builder order: four fixed cases'",timeout=180000)
        page.wait_for_timeout(300)
        with page.expect_download(timeout=180000) as event:
            page.evaluate("document.querySelector('#experiment-download').click()")
        downloaded=OUT/(kind+'-download.json');event.value.save_as(downloaded)
        assert downloaded.stat().st_size==file.stat().st_size<=64*1024*1024
        assert digest(downloaded)==digest(file)
        with page.expect_response(lambda r:r.url==BASE+'/api/experiment-report' and r.request.method=='POST',timeout=180000) as imported:
            page.locator('#experiment-import').set_input_files(downloaded)
        assert imported.value.status==200
        page.wait_for_timeout(300)
        assert page.locator('#error').get_attribute('hidden') is not None
        assert page.request.get(BASE+'/api/run').json()==before
        results.append(dict(kind=kind,bytes=downloaded.stat().st_size,sha256=digest(downloaded),
            original_import_http=uploaded.value.status,reimport_http=imported.value.status,
            byte_identical=True,singleton_unchanged=True))
    assert not errors,errors
    browser.close()
(OUT/'download-fix-browser-result.json').write_text(json.dumps(results,indent=2))
print(json.dumps(results,indent=2))
