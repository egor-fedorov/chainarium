use num_bigint::BigUint;
use ruint::aliases::U256;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{fmt, str::FromStr};

/// Exact atoms and counters. The inner value cannot be mutated by a caller.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(U256);
impl Amount {
    pub const ZERO: Self = Self(U256::ZERO);
    pub const MAX: Self = Self(U256::MAX);
    pub fn is_zero(self) -> bool {
        self == Self::ZERO
    }
    pub fn checked_add(self, rhs: Self) -> Result<Self, String> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or_else(|| "amount overflow".into())
    }
    pub fn checked_sub(self, rhs: Self) -> Result<Self, String> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or_else(|| "insufficient balance".into())
    }
    pub fn checked_mul(self, rhs: Self) -> Result<Self, String> {
        self.0
            .checked_mul(rhs.0)
            .map(Self)
            .ok_or_else(|| "amount overflow".into())
    }
    pub fn big(self) -> BigUint {
        BigUint::from_bytes_le(&self.0.to_le_bytes::<32>())
    }
    pub fn from_big(n: BigUint) -> Result<Self, String> {
        n.to_string().parse()
    }
    pub fn mul_div(self, rhs: Self, denominator: Self, ceil: bool) -> Result<Self, String> {
        if denominator.is_zero() {
            return Err("zero denominator".into());
        }
        let product = self.big() * rhs.big(); // at most 512 bits
        let d = denominator.big();
        let n = if ceil {
            product + &d - BigUint::from(1u8)
        } else {
            product
        };
        Self::from_big(n / d)
    }
    pub fn units(text: &str, decimals: u8) -> Result<Self, String> {
        let mut parts = text.split('.');
        let whole = parts.next().unwrap_or("");
        let fraction = parts.next().unwrap_or("");
        if parts.next().is_some()
            || whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || !fraction.bytes().all(|b| b.is_ascii_digit())
            || fraction.len() > decimals as usize
            || (whole.len() > 1 && whole.starts_with('0'))
            || text.ends_with('.')
        {
            return Err("invalid exact decimal amount".into());
        }
        let digits = format!(
            "{whole}{fraction}{}",
            "0".repeat(decimals as usize - fraction.len())
        );
        let trimmed = digits.trim_start_matches('0');
        if trimmed.is_empty() {
            Ok(Self::ZERO)
        } else {
            trimmed.parse()
        }
    }
}
impl From<u64> for Amount {
    fn from(n: u64) -> Self {
        Self(U256::from(n))
    }
}
impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for Amount {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty()
            || s.len() > 78
            || (s.len() > 1 && s.starts_with('0'))
            || !s.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("expected canonical unsigned decimal string".into());
        }
        U256::from_str_radix(s, 10)
            .map(Self)
            .map_err(|_| "amount exceeds U256".into())
    }
}
impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(de::Error::custom)
    }
}

/// Rational price in quote atoms per base atom, with exact positive components.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "PriceWire", into = "PriceWire")]
pub struct Price {
    base_asset: String,
    quote_asset: String,
    numerator: Amount,
    denominator: Amount,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PriceWire {
    base_asset: String,
    quote_asset: String,
    numerator: Amount,
    denominator: Amount,
}
impl Price {
    pub fn base(&self) -> &str {
        &self.base_asset
    }
    pub fn quote(&self) -> &str {
        &self.quote_asset
    }
    pub fn numerator(&self) -> Amount {
        self.numerator
    }
    pub fn denominator(&self) -> Amount {
        self.denominator
    }
    pub fn new(
        base_asset: String,
        quote_asset: String,
        numerator: Amount,
        denominator: Amount,
    ) -> Result<Self, String> {
        if base_asset.is_empty()
            || quote_asset.is_empty()
            || base_asset == quote_asset
            || numerator.is_zero()
            || denominator.is_zero()
        {
            return Err("invalid rational price".into());
        }
        Ok(Self {
            base_asset,
            quote_asset,
            numerator,
            denominator,
        })
    }
}
impl TryFrom<PriceWire> for Price {
    type Error = String;
    fn try_from(w: PriceWire) -> Result<Self, String> {
        Self::new(w.base_asset, w.quote_asset, w.numerator, w.denominator)
    }
}
impl From<Price> for PriceWire {
    fn from(p: Price) -> Self {
        Self {
            base_asset: p.base_asset,
            quote_asset: p.quote_asset,
            numerator: p.numerator,
            denominator: p.denominator,
        }
    }
}
