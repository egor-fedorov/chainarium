exec(open('/tmp/dec27-dec28-independent-review/differential.py').read().split('base=json.loads')[0])
base=json.loads((S/'scenarios/demos/sc-flash-arb-success.json').read_text())
for case in range(60):
 s=copy.deepcopy(base); s['protocols']={}
 for name in ['pool','pool-b']:s['accounts'].pop(name)
 for asset in ['A','B']:s['assets'][asset]={'decimals':0}
 venues=[]
 for i in range(rng.randint(4,12)):
  x,y=rng.sample(['Q','X','A','B'],2); ident=f'v{i:02}'; acct='pool-'+ident
  rx,ry=rng.randint(1000,1000000),rng.randint(1000,1000000); fee=rng.choice([0,30,100])
  s['protocols'][ident]=dict(account=acct,x=x,y=y,fee_bps=fee,owner='alice')
  s['accounts'][acct]=dict(nonce='0',balances={x:str(rx),y:str(ry)})
  venues.append((ident,x,y,rx,ry,fee))
 cfg=s['scheduler']['flash_arbitrage']; limits=cfg['limits']
 limits.update(hops=rng.choice([2,3]),trials_per_policy=rng.choice([1,2,8,64]),trials_per_round=rng.choice([1,3,10,64]),venue_checks=rng.choice([1,7,20,80,4096]))
 p=cfg['policies']['trader'];p.update(resource_limit='300',resource_price='0',principal='1000',minimum_net_profit='1')
 if case%2==0:
  s['accounts']['bob']['balances']['Q']='1000'
  cfg['policies']['recipient']=copy.deepcopy(p);cfg['policies']['recipient']['account']='bob'
 all_metrics=dict(trials=0,venue_checks=0,truncated=False)
 expected=[]
 for agent in sorted(cfg['policies']):
  info=dict(trials=0,truncated=False,best=None)
  if all_metrics['trials']>=limits['trials_per_round'] or all_metrics['venue_checks']>=limits['venue_checks']:
   info['truncated']=all_metrics['truncated']=True
  else:
   def stop():info['truncated']=True;all_metrics['truncated']=True
   def dfs(asset,path,amount):
    if len(path)>=limits['hops']:return
    for ident,x,y,rx,ry,fee in venues:
     if all_metrics['venue_checks']>=limits['venue_checks']:stop();break
     all_metrics['venue_checks']+=1
     if ident in path:continue
     if asset==x:other=y;out=quote(rx,ry,amount,fee)
     elif asset==y:other=x;out=quote(ry,rx,amount,fee)
     else:continue
     new=path+[ident]
     if other=='Q':
      if all_metrics['trials']>=limits['trials_per_round'] or info['trials']>=limits['trials_per_policy']:stop();return
      all_metrics['trials']+=1;info['trials']+=1
      net=out-1001
      if amount>0 and out>0 and net>=1:
       candidate=(net,150+20*len(new),new)
       if info['best'] is None or (-net,candidate[1],new)<(-info['best'][0],info['best'][1],info['best'][2]):info['best']=candidate
     else:dfs(other,new,out)
     if info['truncated']:break
   dfs('Q',[],1000)
  expected.append((agent,info))
 rr,_=run(s,f'search-{case:03}')
 report=rr['blocks'][0]['scheduler']['flash_arbitrage']
 for key in ['trials','venue_checks','truncated']:assert report['metrics'][key]==all_metrics[key],(case,key,report['metrics'],all_metrics)
 for decision,(agent,info) in zip(report['decisions'],expected):
  assert decision['agent']==agent and decision['truncated']==info['truncated']
  assert len(decision['candidates'])==info['trials']
  if info['best'] is None:assert decision['selected'] is None,(case,decision,info)
  else:
   chosen=decision['candidates'][decision['selected']]
   assert (int(chosen['net']),int(chosen['receipt']['used']),[h['venue'] for h in chosen['hops']])==info['best'],(case,decision,info)
print('PASS: 60 independent DFS multigraph cases, 2/3-hop cycles, caps, tie order and shared AgentId budgets.')
