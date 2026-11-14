"""Check the actual UI download serializer at the report-size boundary."""
import json
from pathlib import Path
import subprocess
import sys

SRC=Path(sys.argv[1]);OUT=Path(sys.argv[2]);OUT.mkdir(exist_ok=True)
plan=json.loads((SRC/'scenarios/experiments/builder.json').read_text())
plan['cases']=plan['cases'][:1]
plan['hypothesis']='x'*33540306
file=OUT/'near-limit-plan.json';file.write_text(json.dumps(plan,separators=(',',':')))
dest=OUT/'near-limit'
p=subprocess.run([str(SRC/'target/debug/chainarium'),'experiment',str(file),str(dest)],capture_output=True,text=True)
assert p.returncode==0,p.stderr
manifest=dest/'manifest.json'
report=json.loads(manifest.read_text())
# JavaScript pretty serialization is evaluated with Node, using the same exact
# JSON.stringify(value,null,2) expression as web/app.js's shared download function.
script=OUT/'size.js'
script.write_text("const fs=require('fs');const r=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));const b=JSON.stringify(r,null,2);fs.writeFileSync(process.argv[3],b);console.log(Buffer.byteLength(b));")
downloaded=OUT/'ui-downloaded-report.json'
q=subprocess.run(['node',str(script),str(manifest),str(downloaded)],capture_output=True,text=True,check=True)
result={'cli_exit':p.returncode,'outcome':report['outcome'],'cli_manifest_bytes':manifest.stat().st_size,
        'ui_download_bytes':downloaded.stat().st_size,'import_limit':64*1024*1024}
assert result['cli_manifest_bytes']<=result['import_limit']<result['ui_download_bytes'],result
(OUT/'download-size-result.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result,indent=2))
