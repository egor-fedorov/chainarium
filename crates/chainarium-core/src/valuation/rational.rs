use crate::Amount;
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, Zero};
use serde::{Serialize, Serializer};

/// Reduced signed fractions. Only presentation uses floor; never ledger arithmetic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exact {
    n: BigInt,
    d: BigUint,
}
impl Default for Exact {
    fn default() -> Self {
        Self::integer(Amount::ZERO)
    }
}
impl Exact {
    pub fn integer(n: Amount) -> Self {
        Self {
            n: n.big().into(),
            d: BigUint::one(),
        }
    }
    pub fn ratio(n: BigInt, d: BigUint, bits: u64) -> Result<Self, String> {
        if d.is_zero() {
            return Err("InvalidClaim".into());
        }
        let (mut a, mut b) = (n.magnitude().clone(), d.clone());
        while !b.is_zero() {
            let rem = a % &b;
            a = b;
            b = rem;
        }
        let result = Self {
            n: n / BigInt::from(a.clone()),
            d: d / a,
        };
        if result.n.bits() > bits || result.d.bits() > bits {
            return Err("ArithmeticLimit".into());
        }
        Ok(result)
    }
    pub fn add(&self, rhs: &Self, bits: u64) -> Result<Self, String> {
        Self::ratio(
            &self.n * BigInt::from(rhs.d.clone()) + &rhs.n * BigInt::from(self.d.clone()),
            &self.d * &rhs.d,
            bits,
        )
    }
    pub fn sub(&self, rhs: &Self, bits: u64) -> Result<Self, String> {
        self.add(&rhs.neg(), bits)
    }
    pub fn mul(&self, rhs: &Self, bits: u64) -> Result<Self, String> {
        Self::ratio(&self.n * &rhs.n, &self.d * &rhs.d, bits)
    }
    pub fn neg(&self) -> Self {
        Self {
            n: -&self.n,
            d: self.d.clone(),
        }
    }
    pub fn is_zero(&self) -> bool {
        self.n.is_zero()
    }
    pub fn floor(&self) -> BigInt {
        let d = BigInt::from(self.d.clone());
        let q = &self.n / &d;
        if self.n.sign() == Sign::Minus && !(&self.n % d).is_zero() {
            q - 1
        } else {
            q
        }
    }
}
impl Serialize for Exact {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Wire {
            numerator: String,
            denominator: String,
            floor_atoms: String,
        }
        Wire {
            numerator: self.n.to_string(),
            denominator: self.d.to_string(),
            floor_atoms: self.floor().to_string(),
        }
        .serialize(s)
    }
}
