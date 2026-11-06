use crate::{Amount, Call, Scenario, Transaction, amount::Price};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MODEL: &str = "reference-mtm-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub portfolios: usize,
    pub accounts_per_portfolio: usize,
    pub reference_assets: usize,
    pub updates_total: usize,
    pub capital_transfers: usize,
    pub component_checks_per_portfolio: usize,
    pub rational_bits: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            portfolios: 16,
            accounts_per_portfolio: 64,
            reference_assets: 64,
            updates_total: 16384,
            capital_transfers: 4096,
            component_checks_per_portfolio: 4096,
            rational_bits: 32768,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Portfolio {
    pub accounts: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub available_at: Amount,
    pub observed_at: Amount,
    pub sequence: Amount,
    pub price: Price,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Feed {
    pub source_id: String,
    pub records: Vec<Record>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct CapitalTransfer {
    pub portfolio: String,
    pub tx_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: String,
    pub numeraire: String,
    pub evaluation_point: String,
    pub source: String,
    pub max_age_seconds: Amount,
    pub limits: Limits,
    pub portfolios: BTreeMap<String, Portfolio>,
    pub reference_feeds: BTreeMap<String, Feed>,
    pub capital_transfers: Vec<CapitalTransfer>,
}
pub(crate) fn literal_transactions(s: &Scenario) -> impl Iterator<Item = &Transaction> {
    s.blocks.iter().flatten().chain(
        s.scheduler
            .iter()
            .flat_map(|c| c.emissions.iter().map(|e| &e.transaction)),
    )
}
pub fn validate(s: &Scenario) -> Result<(), String> {
    let has = s.model_versions.iter().any(|m| m == MODEL);
    let Some(p) = &s.valuation_policy else {
        return if has {
            Err("valuation model/config mismatch".into())
        } else {
            Ok(())
        };
    };
    if !has
        || p.version != MODEL
        || p.numeraire != "Q"
        || !s.assets.contains_key(&p.numeraire)
        || p.evaluation_point != "committed-block-end"
        || p.source != "environment-reference"
        || s.rounds() > 10000
    {
        return Err("invalid valuation profile".into());
    }
    let limits = serde_json::to_value(&p.limits).map_err(|e| e.to_string())?;
    let caps = serde_json::to_value(Limits::default()).map_err(|e| e.to_string())?;
    for (key, value) in limits.as_object().ok_or("valuation limits")? {
        let n = value.as_u64().ok_or("valuation limit")?;
        if n == 0
            || n > caps[key].as_u64().ok_or("valuation cap")?
            || (key == "rational_bits" && n < 512)
        {
            return Err(format!("valuation limit outside cap: {key}"));
        }
    }
    if p.portfolios.is_empty()
        || p.portfolios.len() > p.limits.portfolios
        || p.reference_feeds.len() > p.limits.reference_assets
        || p.reference_feeds
            .values()
            .map(|f| f.records.len())
            .sum::<usize>()
            > p.limits.updates_total
        || p.capital_transfers.len() > p.limits.capital_transfers
    {
        return Err("valuation input cap".into());
    }
    for (id, portfolio) in &p.portfolios {
        let accounts: BTreeSet<_> = portfolio.accounts.iter().collect();
        if id.is_empty()
            || accounts.is_empty()
            || accounts.len() != portfolio.accounts.len()
            || accounts.len() > p.limits.accounts_per_portfolio
            || accounts
                .iter()
                .any(|a| !s.accounts.contains_key(*a) || s.protected(a))
        {
            return Err("invalid valuation portfolio".into());
        }
    }
    for (asset, f) in &p.reference_feeds {
        if asset == &p.numeraire
            || !s.assets.contains_key(asset)
            || f.source_id.is_empty()
            || f.records
                .iter()
                .any(|r| r.price.base() != asset || r.price.quote() != p.numeraire)
            || f.records
                .windows(2)
                .any(|w| w[1].available_at < w[0].available_at || w[1].sequence <= w[0].sequence)
        {
            return Err("invalid valuation reference feed".into());
        }
    }
    let mut txs: BTreeMap<&str, Vec<&Transaction>> = BTreeMap::new();
    for tx in literal_transactions(s) {
        txs.entry(&tx.id).or_default().push(tx);
    }
    let payments: BTreeSet<_> = s
        .scheduler
        .iter()
        .flat_map(|c| c.bundles.iter())
        .flat_map(|c| &c.emissions)
        .filter_map(|e| e.bundle.payment_tx_id.as_deref())
        .collect();
    let mut seen = BTreeSet::new();
    let own_definitions: BTreeSet<_> = s
        .scheduler
        .iter()
        .flat_map(|c| c.bundles.iter())
        .flat_map(|c| &c.emissions)
        .flat_map(|e| &e.bundle.members)
        .filter_map(|m| match m {
            crate::scheduler::bundles::Member::Own { transaction } => Some(transaction.id.as_str()),
            crate::scheduler::bundles::Member::VisibleTxId { .. } => None,
        })
        .collect();
    for binding in &p.capital_transfers {
        let portfolio = p
            .portfolios
            .get(&binding.portfolio)
            .ok_or("unknown capital portfolio")?;
        let targets = txs
            .get(binding.tx_id.as_str())
            .ok_or("capital target must be literal transaction")?;
        if !seen.insert(binding)
            || targets.len() != 1
            || payments.contains(binding.tx_id.as_str())
            || own_definitions.contains(binding.tx_id.as_str())
        {
            return Err("ambiguous, duplicate or bid capital binding".into());
        }
        let tx = targets[0];
        let [
            Call::Transfer {
                to,
                asset: _,
                amount,
            },
        ] = tx.program.as_slice()
        else {
            return Err("capital target must be sole Transfer".into());
        };
        if amount.is_zero()
            || s.protected(&tx.sender)
            || s.protected(to)
            || !s.accounts.contains_key(to)
            || portfolio.accounts.contains(&tx.sender) == portfolio.accounts.contains(to)
        {
            return Err("capital Transfer must cross ordinary portfolio boundary".into());
        }
    }
    Ok(())
}
