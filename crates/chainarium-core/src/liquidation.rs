//! Accepted DEC-24: sell only collateral actually received from the nested liquidation.
use crate::{Amount, credit::OracleUse, routing::heterogeneous::RouteResult};
use serde::{Deserialize, Serialize};

pub const MODEL: &str = "liquidate-and-sell-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SaleResult {
    pub market: String,
    pub borrower: String,
    pub requested: Amount,
    pub paid: Amount,
    pub seized: Amount,
    pub oracle: Option<OracleUse>,
    // A diagnostic attempt can stop before or during the sale. Committed results have a full route.
    pub route: Option<RouteResult>,
}
