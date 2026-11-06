//! Accepted DEC-22: explicit bundle authorship, paid bids and bounded builder trials.
use super::*;
use crate::Receipt;
pub const MODELS: [&str; 2] = ["native-bundles-v1", "bounded-greedy-v1"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum CandidateId {
    Transaction { id: String },
    Bundle { id: String },
}
impl CandidateId {
    pub fn label(&self) -> String {
        match self {
            Self::Transaction { id } => format!("tx:{id}"),
            Self::Bundle { id } => format!("bundle:{id}"),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Member {
    Own { transaction: Box<Transaction> },
    VisibleTxId { tx_id: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub id: String,
    pub agent: String,
    pub members: Vec<Member>,
    pub min_block: Amount,
    pub max_block: Amount,
    pub deadline: Amount,
    pub allowed_reverts: BTreeSet<String>,
    pub bid: Amount,
    pub payment_tx_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Resolved {
    pub id: String,
    pub agent: String,
    pub members: Vec<Transaction>,
    pub min_block: Amount,
    pub max_block: Amount,
    pub deadline: Amount,
    pub allowed_reverts: BTreeSet<String>,
    pub bid: Amount,
    pub payment_tx_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Emission {
    pub at: Amount,
    pub producer_id: String,
    pub producer_sequence: Amount,
    pub bundle: Bundle,
    pub delivery: Delivery,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub members: usize,
    pub bytes: usize,
    pub pending: usize,
    pub known: usize,
    pub probes: usize,
    pub probe_members: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            members: 16,
            bytes: 1024 * 1024,
            pending: 128,
            known: 4096,
            probes: 64,
            probe_members: 256,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub limits: Limits,
    pub emissions: Vec<Emission>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KnownBundle {
    pub fingerprint: String,
    pub agent: String,
    pub submitted_at: Amount,
    pub delivery: Delivery,
    pub resolved: Option<Resolved>,
    pub status: Status,
    pub committed_status: Status,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BundleMessage {
    pub bundle: Resolved,
    pub channel: Channel,
    pub submitted_at: Amount,
    pub delivered_at: Amount,
    pub event_id: Amount,
    pub round: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnBundle {
    pub id: String,
    pub bundle: Option<Resolved>,
    pub submitted_at: Amount,
    pub status: Status,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    pub candidate: CandidateId,
    pub mode: String,
    pub outcome: String,
    pub selected: bool,
    pub used: Amount,
    pub paid_fees: Amount,
    pub bid: Amount,
    pub score: Amount,
    pub builder_delta: crate::clmm::SignedAmount,
    pub receipts: Vec<Receipt>,
}
impl Probe {
    pub fn empty(candidate: CandidateId, mode: &str, outcome: &str) -> Self {
        Self {
            candidate,
            mode: mode.into(),
            outcome: outcome.into(),
            selected: false,
            used: Amount::ZERO,
            paid_fees: Amount::ZERO,
            bid: Amount::ZERO,
            score: Amount::ZERO,
            builder_delta: crate::clmm::SignedAmount {
                negative: false,
                magnitude: Amount::ZERO,
            },
            receipts: vec![],
        }
    }
}

pub(super) fn validate_config(s: &Scenario, c: &super::Config) -> Result<(), String> {
    let has = |name: &str| s.model_versions.iter().any(|m| m == name);
    let Some(cfg) = &c.bundles else {
        if has("native-bundles-v1")
            || has("bounded-greedy-v1")
            || matches!(c.builder, Builder::Greedy | Builder::FixedItems { .. })
        {
            return Err("bundle builder/profile/config mismatch".into());
        }
        return Ok(());
    };
    if !has("native-bundles-v1")
        || (matches!(c.builder, Builder::Greedy) && !has("bounded-greedy-v1"))
    {
        return Err("missing bundle/greedy model".into());
    }
    let limits = serde_json::to_value(&cfg.limits).map_err(|e| e.to_string())?;
    let caps = serde_json::to_value(Limits::default()).map_err(|e| e.to_string())?;
    for (key, value) in limits.as_object().ok_or("bundle limits")? {
        let n = value.as_u64().ok_or("bundle limit")?;
        if n == 0 || n > caps[key].as_u64().ok_or("bundle cap")? {
            return Err(format!("bundle limit outside cap: {key}"));
        }
    }
    if c.emissions.len() + cfg.emissions.len() > c.limits.emissions {
        return Err("combined emissions limit".into());
    }
    let end = s.chain.interval.checked_mul((c.rounds as u64).into())?;
    let mut producers: BTreeSet<_> = c
        .emissions
        .iter()
        .map(|e| (&e.producer_id, e.producer_sequence))
        .collect();
    for e in &cfg.emissions {
        if e.at > end
            || e.producer_id.is_empty()
            || !producers.insert((&e.producer_id, e.producer_sequence))
        {
            return Err("invalid bundle emission/time/producer".into());
        }
        if let Channel::Private { recipients } = &e.delivery.channel
            && recipients.iter().any(|a| !s.agents.contains_key(a))
        {
            return Err("unknown bundle recipient".into());
        }
    }
    if let Builder::FixedItems { priority } = &c.builder
        && (priority.len() > c.limits.known + cfg.limits.known
            || priority.iter().collect::<BTreeSet<_>>().len() != priority.len())
    {
        return Err("invalid FixedItems priority list".into());
    }
    Ok(())
}

impl State {
    pub(super) fn busy(&self, agent: &str, account: &str, nonce: Amount) -> bool {
        self.known.values().any(|k| {
            k.standalone
                && k.transaction.agent == agent
                && k.transaction.sender == account
                && k.transaction.nonce == nonce
                && k.status.unfinished()
        }) || self.bundles.values().any(|k| {
            k.status.unfinished()
                && k.resolved.as_ref().is_some_and(|b| {
                    b.members
                        .iter()
                        .any(|t| t.agent == agent && t.sender == account && t.nonce == nonce)
                })
        })
    }
    /// Resolve from this sender's already authorized information, never the global registry.
    pub fn visible_transaction(&self, agent: &str, id: &str) -> Option<Transaction> {
        self.visible
            .get(agent)
            .into_iter()
            .flatten()
            .find(|m| m.transaction.id == id)
            .map(|m| m.transaction.clone())
            .or_else(|| {
                self.known
                    .get(id)
                    .filter(|k| k.standalone && k.transaction.agent == agent)
                    .map(|k| k.transaction.clone())
            })
            .or_else(|| {
                self.bundle_visible
                    .get(agent)
                    .into_iter()
                    .flatten()
                    .flat_map(|m| &m.bundle.members)
                    .find(|t| t.id == id)
                    .cloned()
            })
            .or_else(|| {
                self.bundles
                    .values()
                    .filter(|k| k.agent == agent)
                    .filter_map(|k| k.resolved.as_ref())
                    .flat_map(|b| &b.members)
                    .find(|t| t.id == id)
                    .cloned()
            })
    }
}
