import json,pathlib,subprocess,copy
R=pathlib.Path('/tmp/dec27-dec28-independent-review');binary=R/'fix-target/debug/chainarium'
s=json.loads((R/'unregistered-capital-sender.json').read_text())
def run(raw,name):
 f=R/(name+'.json');f.write_text(json.dumps(raw,indent=2));out=R/(name+'-result')
 p=subprocess.run([str(binary),'run',str(f),str(out)],text=True,capture_output=True)
 return p,out
p,_=run(s,'fix-labelled-unknown')
assert p.returncode!=0 and 'capital Transfer must cross ordinary portfolio boundary' in p.stderr,p
control=copy.deepcopy(s);control['valuation_policy']['capital_transfers']=[]
p,out=run(control,'fix-unlabelled-unknown');assert p.returncode==0,p.stderr
r=json.loads((out/'run.json').read_text());assert not r['blocks'][0]['receipts'];assert r['scheduler']['known']['victim']['status']['type']=='Rejected'
control=copy.deepcopy(s);control['scheduler']['emissions'][0]['transaction']['sender']='bob';control['scheduler']['emissions'][0]['transaction']['program'][0]['asset']='Q'
p,out=run(control,'fix-labelled-registered');assert p.returncode==0,p.stderr
flows=[json.loads(l) for l in (out/'capital-flows.jsonl').read_text().splitlines()];assert len(flows)==1 and flows[0]['amount']=='1' and flows[0]['direction']=='Inflow',flows
print('PASS fix 22b1be27f02ab80320ba1d587ff997ea74d74c5b: original reproducer rejects at scenario validation; unlabelled invalid emission still observable at admission; registered labelled transfer commits one real inflow.')
