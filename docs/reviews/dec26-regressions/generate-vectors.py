import importlib.util, json, pathlib
root=pathlib.Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('reference', root/'docs/reviews/baseline-agents-reference.py')
ref=importlib.util.module_from_spec(spec); spec.loader.exec_module(ref)
cases=[]
for seed in [0,1,42,(1<<64)-1,(1<<256)-1]:
 for agent in ['trader','z','name:/β']:
  for count in [1,2,3,7,16]:
   stream=ref.Stream(seed,'agent/'+agent)
   draws=[]
   for k in range(8):
    draws.append({'gate':str(stream.uniform(1000000)), 'choice':str(stream.uniform(count)), 'position':str(stream.position)})
   cases.append({'seed':str(seed),'agent':agent,'count':count,'rounds':draws})
(root/'crates/chainarium-core/tests/dec26-independent-vectors.json').write_text(json.dumps(cases))
print(len(cases),'seed/name/template combinations; 8 draws pairs each')
