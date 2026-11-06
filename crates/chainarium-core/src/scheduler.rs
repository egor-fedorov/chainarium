//! Accepted DEC-21: deterministic delivery, restricted observations and bounded builders.
pub mod backrun;
pub mod bundles;
mod runtime;
use crate::{Amount, Call, Scenario, Transaction, World, routing::Observation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MODELS: [&str; 4] = [
    "event-scheduler-v1",
    "bounded-mempool-v1",
    "basic-builders-v1",
    "on-visible-program-v1",
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Channel {
    Public,
    Private { recipients: BTreeSet<String> },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Delivery {
    pub channel: Channel,
    pub builder_delay: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub observe_public: bool,
    pub observation_delay: Amount,
    pub outgoing: Delivery,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Builder {
    Fifo,
    FeePriority,
    Fixed { priority: Vec<String> },
    FixedItems { priority: Vec<bundles::CandidateId> },
    Greedy,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub agents: usize,
    pub emissions: usize,
    pub rounds: usize,
    pub pending: usize,
    pub queue: usize,
    pub visible: usize,
    pub known: usize,
    pub events_per_round: usize,
    pub reactions_per_round: usize,
    pub depth: usize,
    pub intents_per_agent: usize,
    pub retention: usize,
    pub inclusion_attempts: usize,
    pub eligibility_checks: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            agents: 64,
            emissions: 4096,
            rounds: 10000,
            pending: 1024,
            queue: 4096,
            visible: 256,
            known: 65536,
            events_per_round: 4096,
            reactions_per_round: 64,
            depth: 8,
            intents_per_agent: 1,
            retention: 32,
            inclusion_attempts: 256,
            eligibility_checks: 65536,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Emission {
    pub at: Amount,
    pub producer_id: String,
    pub producer_sequence: Amount,
    pub transaction: Transaction,
    pub delivery: Delivery,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReactivePolicy {
    pub account: String,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    pub reaction_delay: Amount,
    pub lifetime: Amount,
    pub rules: BTreeMap<String, Vec<Call>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub rounds: usize,
    pub builder: Builder,
    pub limits: Limits,
    pub agents: BTreeMap<String, Network>,
    pub emissions: Vec<Emission>,
    pub reactive: BTreeMap<String, ReactivePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundles: Option<bundles::Config>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backrun: Option<backrun::Config>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidator: Option<crate::policy::liquidator::Config>,
}

pub fn validate(s: &Scenario) -> Result<(), String> {
    let has = |m: &str| s.model_versions.iter().any(|v| v == m);
    let Some(c) = &s.scheduler else {
        if has(crate::policy::liquidator::MODEL)
            || has(backrun::MODEL)
            || !has("scheduler-m1-v1")
            || MODELS.iter().chain(bundles::MODELS.iter()).any(|m| has(m))
        {
            return Err("scheduler profile/config mismatch".into());
        }
        return Ok(());
    };
    if has("scheduler-m1-v1") || !s.blocks.is_empty() || MODELS[..3].iter().any(|m| !has(m)) {
        return Err("M1 and event scheduler profiles are mutually exclusive".into());
    }
    // Each cap is explicit in JSON. Rust's Default only supplies reference presets in code.
    let limits = serde_json::to_value(&c.limits).map_err(|e| e.to_string())?;
    let caps = serde_json::to_value(Limits::default()).map_err(|e| e.to_string())?;
    for (key, value) in limits.as_object().ok_or("limits object")? {
        let n = value.as_u64().ok_or("invalid scheduler limit")?;
        if n == 0 || n > caps[key].as_u64().ok_or("cap")? {
            return Err(format!("scheduler limit outside cap: {key}"));
        }
    }
    if s.agents.len() > c.limits.agents
        || c.emissions.len() > c.limits.emissions
        || c.rounds == 0
        || c.rounds > c.limits.rounds
        || c.agents.keys().ne(s.agents.keys())
    {
        return Err("scheduler agents/emissions/rounds limit or missing network".into());
    }
    let channel = |d: &Delivery| -> Result<(), String> {
        if let Channel::Private { recipients } = &d.channel
            && recipients.iter().any(|a| !s.agents.contains_key(a))
        {
            return Err("unknown private recipient".into());
        }
        Ok(())
    };
    for n in c.agents.values() {
        channel(&n.outgoing)?;
    }
    let end = s.chain.interval.checked_mul((c.rounds as u64).into())?;
    let mut producers = BTreeSet::new();
    for e in &c.emissions {
        channel(&e.delivery)?;
        if e.at > end
            || e.producer_id.is_empty()
            || !producers.insert((&e.producer_id, e.producer_sequence))
            || e.transaction.id.starts_with("policy:")
            || e.transaction.id.starts_with("reactive:")
            || serde_json::to_vec(&e.transaction)
                .map_err(|e| e.to_string())?
                .len()
                > crate::model::MAX_INPUT
        {
            return Err("invalid scheduler emission/time/producer or reserved TxId".into());
        }
        // Invalid envelopes are observable admission rejections, never pre-authorized here.
    }
    if let Builder::Fixed { priority } = &c.builder
        && (priority.len() > c.limits.known
            || priority.iter().collect::<BTreeSet<_>>().len() != priority.len())
    {
        return Err("invalid Fixed priority list".into());
    }
    if !c.reactive.is_empty() && !has(MODELS[3]) {
        return Err("missing reactive policy model".into());
    }
    for (agent, p) in &c.reactive {
        if s.policies.contains_key(agent)
            || s.lp_policies.contains_key(agent)
            || s.mm_policies.contains_key(agent)
            || !s
                .agents
                .get(agent)
                .is_some_and(|a| a.controlled_accounts.contains(&p.account))
            || p.lifetime.is_zero()
            || p.rules.len() > 16
        {
            return Err("invalid/duplicate reactive policy".into());
        }
        for (trigger, program) in &p.rules {
            if trigger.is_empty() {
                return Err("empty reactive trigger".into());
            }
            s.validate_tx(&Transaction {
                id: "validation".into(),
                agent: agent.clone(),
                sender: p.account.clone(),
                chain: s.chain.id.clone(),
                nonce: Amount::ZERO,
                limit: p.resource_limit,
                price: p.resource_price,
                deadline: Amount::ZERO,
                program: program.clone(),
            })?;
        }
        if p.rules.is_empty() || p.resource_limit < 100.into() {
            return Err("empty/invalid reactive policy".into());
        }
    }
    bundles::validate_config(s, c)?;
    backrun::validate(s, c)?;
    crate::policy::liquidator::validate(s, c)?;
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Status {
    BundleMember,
    InFlight,
    Pending,
    Waiting { reason: String },
    Included { block: Amount, outcome: String },
    Rejected { reason: String },
}
impl Status {
    pub fn unfinished(&self) -> bool {
        matches!(self, Self::InFlight | Self::Pending | Self::Waiting { .. })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Known {
    #[serde(default = "yes", skip_serializing_if = "is_yes")]
    pub standalone: bool,
    pub transaction: Transaction,
    pub delivery: Delivery,
    pub submitted_at: Amount,
    pub depth: usize,
    pub status: Status,
    // Only this status is exposed in observations until the next committed boundary.
    pub committed_status: Status,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pending {
    pub arrival: Amount,
    pub event_id: Amount,
    pub round: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum EventKind {
    BackrunReaction {
        tx_id: String,
        agent: String,
        depth: usize,
    },
    BundleEmission {
        index: usize,
    },
    BundleBuilderDelivery {
        bundle_id: String,
    },
    BundleAgentDelivery {
        bundle_id: String,
        agent: String,
    },
    Emission {
        index: usize,
    },
    BuilderDelivery {
        tx_id: String,
    },
    AgentDelivery {
        tx_id: String,
        agent: String,
    },
    Reaction {
        tx_id: String,
        agent: String,
        depth: usize,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Queued {
    pub id: Amount,
    pub due_at: Amount,
    pub kind: EventKind,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub transaction: Transaction,
    pub channel: Channel,
    pub submitted_at: Amount,
    pub delivered_at: Amount,
    pub event_id: Amount,
    pub round: usize,
    pub depth: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub next_event_id: Amount,
    pub emission_cursor: usize,
    pub queue: Vec<Queued>,
    pub known: BTreeMap<String, Known>,
    pub pending: BTreeMap<String, Pending>,
    pub visible: BTreeMap<String, Vec<Message>>,
    pub triggered: BTreeMap<String, BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bundles: BTreeMap<String, bundles::KnownBundle>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bundle_pending: BTreeMap<String, Pending>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bundle_visible: BTreeMap<String, Vec<bundles::BundleMessage>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            next_event_id: 1.into(),
            emission_cursor: 0,
            queue: vec![],
            known: BTreeMap::new(),
            pending: BTreeMap::new(),
            visible: BTreeMap::new(),
            triggered: BTreeMap::new(),
            bundles: BTreeMap::new(),
            bundle_pending: BTreeMap::new(),
            bundle_visible: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnSubmission {
    pub transaction: Transaction,
    pub submitted_at: Amount,
    pub status: Status,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentObservation {
    pub market: Observation,
    pub messages: Vec<Message>,
    pub own_outbox: Vec<OwnSubmission>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bundle_messages: Vec<bundles::BundleMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub own_bundles: Vec<bundles::OwnBundle>,
}
impl ReactivePolicy {
    // Only delivered trigger metadata, own observation and public chain configuration.
    // No World, Scenario, future emissions, global pending queue or simulation access.
    pub fn decide(
        &self,
        agent: &str,
        chain: &crate::Chain,
        obs: &AgentObservation,
        trigger: &str,
        event_id: Amount,
        at: Amount,
    ) -> Result<crate::policy::Decision, String> {
        let tx = Transaction {
            id: format!(
                "reactive:{}",
                crate::run::digest(
                    &serde_json::to_vec(&(agent, trigger, event_id)).map_err(|e| e.to_string())?
                )
            ),
            agent: agent.into(),
            sender: obs.market.account.clone(),
            chain: chain.id.clone(),
            nonce: obs.market.nonce,
            limit: self.resource_limit,
            price: self.resource_price,
            deadline: at.checked_add(self.lifetime)?,
            program: self
                .rules
                .get(trigger)
                .ok_or("unknown reactive trigger")?
                .clone(),
        };
        let mut d = crate::policy::Decision::new(
            agent,
            &obs.market,
            &format!("visible {trigger}; reaction at {at}"),
        );
        d.action = crate::policy::DecisionAction::Submit;
        d.transaction = Some(tx);
        Ok(d)
    }
}

impl State {
    pub fn observation(
        &self,
        s: &Scenario,
        parent: &World,
        agent: &str,
        account: &str,
    ) -> Result<AgentObservation, String> {
        if !s
            .agents
            .get(agent)
            .is_some_and(|a| a.controlled_accounts.contains(account))
        {
            return Err("unauthorized scheduler observation".into());
        }
        Ok(AgentObservation {
            market: Observation::capture(s, parent, account)?,
            bundle_messages: self.bundle_visible.get(agent).cloned().unwrap_or_default(),
            own_bundles: self
                .bundles
                .iter()
                .filter(|(_, k)| k.agent == agent)
                .map(|(id, k)| bundles::OwnBundle {
                    id: id.clone(),
                    bundle: k.resolved.clone(),
                    submitted_at: k.submitted_at,
                    status: k.committed_status.clone(),
                })
                .collect(),
            messages: self.visible.get(agent).cloned().unwrap_or_default(),
            own_outbox: self
                .known
                .values()
                .filter(|k| k.standalone && k.transaction.agent == agent)
                .map(|k| OwnSubmission {
                    transaction: k.transaction.clone(),
                    submitted_at: k.submitted_at,
                    status: k.committed_status.clone(),
                })
                .collect(),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub event_id: Option<Amount>,
    pub due_at: Amount,
    pub at: Amount,
    pub tx_id: Option<String>,
    pub agent: Option<String>,
    pub action: String,
    pub detail: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub actions: BTreeMap<String, usize>,
    #[serde(default)]
    pub probes: usize,
    #[serde(default)]
    pub probe_members: usize,
    #[serde(default)]
    pub probe_used: Amount,
    #[serde(default)]
    pub truncated: bool,
    pub events: usize,
    pub reactions: usize,
    pub attempts: usize,
    pub checks: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Report {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidators: Option<crate::policy::liquidator::Report>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<backrun::Report>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<bundles::Probe>,
    pub trace: Vec<Trace>,
    pub metrics: Metrics,
    pub state: State,
}
pub(crate) use runtime::advance;

fn yes() -> bool {
    true
}
fn is_yes(value: &bool) -> bool {
    *value
}
