import assert from 'node:assert/strict';
import test from 'node:test';
import {atoms,units,formatUnits,MAX} from './wire.mjs';
test('exact atom strings and U256 bounds',()=>{
 for(const v of ['0','9007199254740991','9007199254740993',MAX.toString()]) assert.equal(atoms(v),v);
 for(const v of ['',0,'00','01','+1','-1','1e3','1.0',(MAX+1n).toString()]) assert.throws(()=>atoms(v));
});
test('different decimals without a Number conversion',()=>{
 assert.equal(units('1.001',18),'1001000000000000000');
 assert.equal(units('9007199254740993',0),'9007199254740993');
 assert.equal(units('0.000001',6),'1');
 assert.equal(formatUnits('1',6),'0.000001');
 assert.throws(()=>units('0.0000001',6));
 assert.throws(()=>units('1.0',0));
});
