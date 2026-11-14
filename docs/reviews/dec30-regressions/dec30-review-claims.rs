use chainarium_core::{Run,Scenario,Amount,hodl,valuation};
use serde_json::{json,Value};
use std::{sync::Arc,collections::BTreeMap};
fn fixture(name:&str)->Value {serde_json::from_slice(&std::fs::read(format!("/workspace/chainarium/scenarios/demos/{name}.json")).unwrap()).unwrap()}
fn make(v:Value)->Run {Run::new(Scenario::parse(&serde_json::to_vec(&v).unwrap()).unwrap()).unwrap()}
fn amount(s:&str)->Amount {serde_json::from_value(json!(s)).unwrap()}
fn number(m:&valuation::Mark,n:&str,d:&str) {let x=serde_json::to_value(m.value.as_ref().unwrap()).unwrap();assert_eq!(x["numerator"],n);assert_eq!(x["denominator"],d);}
fn main(){
    println!("DEBUG BUILD {}",chainarium_core::BUILD_ID);
    let mut raw=fixture("sc-hodl-lp");raw["blocks"]=json!([]);raw["protocols"]=json!({});
    for a in raw["accounts"].as_object_mut().unwrap().values_mut(){a["balances"]=json!({"Q":"0","X":"0"});}
    raw["valuation_policy"]["reference_feeds"]["X"]["records"]=json!([{"available_at":"0","observed_at":"0","sequence":"0","price":{"base_asset":"X","quote_asset":"Q","numerator":"3","denominator":"1"}}]);
    for i in 0..3 {
        raw["accounts"][format!("reserve{i}")]=json!({"balances":{"X":"1","Q":"9"},"nonce":"0"});
        raw["protocols"][format!("pool{i}")]=json!({"account":format!("reserve{i}"),"x":"X","y":"Q","fee_bps":0,"owner":"bob"});
    }
    let mut r=make(raw);
    for balances in Arc::make_mut(&mut r.initial.lp_balances).values_mut(){*balances=BTreeMap::from([("alice".into(),1.into()),("bob".into(),2.into())]);}
    r.initial.invariant(&r.scenario).unwrap();r.state=r.initial.clone();
    let report=hodl::at(&r,0).unwrap();let p=&report.portfolios[0];
    match &p.ledger {hodl::Ledger::Ready{current}=>{
        let q=serde_json::to_value(&current.quantities).unwrap();assert_eq!(q["X"]["numerator"],"1");assert_eq!(q["X"]["denominator"],"1");assert_eq!(q["Q"]["numerator"],"9");},_=>panic!("fractions unavailable")}
    number(&p.hodl.mark,"12","1");number(&p.hodl_pnl,"0","1");
    assert_eq!(p.initial_components.iter().filter(|c|c.key.kind=="CpmmLP").count(),6);
    let mut missing=r.clone();missing.scenario.valuation_policy.as_mut().unwrap().reference_feeds.clear();
    let p=hodl::at(&missing,0).unwrap().portfolios.remove(0);assert!(matches!(p.ledger,hodl::Ledger::Ready{..})&&p.hodl.mark.value.is_none());
    let mut capped=r.clone();capped.scenario.valuation_policy.as_mut().unwrap().limits.component_checks_per_portfolio=1;
    let p=hodl::at(&capped,0).unwrap().portfolios.remove(0);assert!(matches!(p.ledger,hodl::Ledger::Unavailable{failure:hodl::Failure{ref reason,..},last_feasible:None} if reason=="ComponentLimit"));
    println!("PASS synthetic invariant-valid LP claims: 1/3 + 1/3 + 1/3 = 1, full holdings preserved without prices, no partial holdings under ComponentLimit");
    // The current native genesis grants sole LP ownership. A valid synthetic
    // exposure snapshot is needed to exercise initial fractional quantities.
    let mut raw=fixture("sc-hodl-credit");raw["blocks"]=json!([]);raw["accounts"]["bob"]["balances"]["Q"]=json!("2");raw["accounts"]["carol"]["balances"]["Q"]=json!("998");
    let mut r=make(raw);
    let cfg=r.scenario.lending["credit"].clone();
    let collateral=cfg.positions["bob"].clone();
    let old=r.initial.balance("bob",&cfg.collateral_asset);
    Arc::make_mut(&mut r.initial.accounts).get_mut("bob").unwrap().balances.insert(cfg.collateral_asset.clone(),old.checked_sub(1.into()).unwrap());
    Arc::make_mut(&mut r.initial.accounts).get_mut(&collateral).unwrap().balances.insert(cfg.collateral_asset.clone(),1.into());
    let state=Arc::make_mut(&mut r.initial.lending).get_mut("credit").unwrap();
    state.positions.get_mut("bob").unwrap().debt=1.into();state.debt=1.into();state.equity=state.equity.checked_add(1.into()).unwrap();
    r.initial.invariant(&r.scenario).unwrap();r.state=r.initial.clone();
    let report=hodl::at(&r,0).unwrap();let p=report.portfolios.iter().find(|p|p.portfolio=="borrower").unwrap();
    assert!(p.initial_components.iter().any(|c|c.key.kind=="Debt"&&serde_json::to_value(c.quantity.as_ref().unwrap()).unwrap()["numerator"]=="-1"));
    assert!(matches!(p.ledger,hodl::Ledger::Unavailable{failure:hodl::Failure{ref reason,..},last_feasible:None} if reason=="InitialLiabilityUnsupported"));
    assert!(p.initial_holdings.is_none()&&p.hodl.mark.value.is_none()&&p.excess_pnl.value.is_none());
    let unaffected=report.portfolios.iter().find(|p|p.portfolio=="depositor").unwrap();assert!(matches!(unaffected.ledger,hodl::Ledger::Ready{..}));
    println!("PASS synthetic invariant-valid gross initial liability rejects benchmark despite positive Q wallet; independent depositor unaffected");
    // Very wide denominators can overflow only at the stable grouping step.
    let mut raw=fixture("sc-hodl-lp");raw["blocks"]=json!([]);raw["protocols"]=json!({});
    for a in raw["accounts"].as_object_mut().unwrap().values_mut(){a["balances"]=json!({"Q":"0","X":"0"});}
    let denoms=["12554203470773361527671578846415332832204710888928069025791","200867255532373784442745261542645325315275374222849104412671","803469022129495137770981046170581301261101496891396417650687"];
    for (i,d) in denoms.iter().enumerate(){
        raw["accounts"][format!("reserve{i}")]=json!({"balances":{"X":"1","Q":"1"},"nonce":"0"});
        raw["protocols"][format!("pool{i}")]=json!({"account":format!("reserve{i}"),"x":"X","y":"Q","fee_bps":0,"owner":"bob"});
        let _=d;
    }
    raw["hodl_policy"]["limits"]["rational_bits"]=json!(512);
    let mut r=make(raw);
    for (i,d) in denoms.iter().enumerate(){let n=amount(d);let id=format!("pool{i}");r.initial.shares.insert(id.clone(),n);Arc::make_mut(&mut r.initial.lp_balances).insert(id,BTreeMap::from([("alice".into(),1.into()),("bob".into(),n.checked_sub(1.into()).unwrap())]));}
    r.initial.invariant(&r.scenario).unwrap();r.state=r.initial.clone();
    let p=hodl::at(&r,0).unwrap().portfolios.remove(0);
    assert!(matches!(p.ledger,hodl::Ledger::Unavailable{failure:hodl::Failure{ref reason,..},last_feasible:None} if reason=="ArithmeticLimit"));
    assert!(p.initial_holdings.is_none());
    println!("PASS reduced initial quantity grouping exceeding 512 bits produces permanent Unavailable without partial inventory");
}
