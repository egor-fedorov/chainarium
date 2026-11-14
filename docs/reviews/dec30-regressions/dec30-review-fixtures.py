import sys,importlib.util,json
from pathlib import Path
sys.dont_write_bytecode=True
for name,dec,model in [('hodl-reference','DEC-30','flow-matched-hodl-v1'),('valuation-reference','DEC-28','reference-mtm-v1')]:
    path=Path('/workspace/chainarium/docs/reviews')/(name+'.py')
    spec=importlib.util.spec_from_file_location('independent',path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
    result=dict(status='Proposed',decision=dec,model=model,cases=module.cases())
    if hasattr(module,'check'):module.check(result)
    assert result==json.loads(path.with_suffix('.json').read_text()),name
    print('PASS fixture consistency',name,len(result['cases']))
