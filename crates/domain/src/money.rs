//! Exact monetary values for forecasts and bounded exposures.
//!
//! Amounts use SQLite-compatible signed integer minor units, constrained to
//! nonnegative values. Signed corrections are a distinct type; they carry no
//! approval, ledger provenance, exchange-rate, or spending authority semantics.
//! Currency validation checks a canonical three-letter code, not membership in
//! an external currency registry. No currency conversion is performed.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;

/// A canonical three-uppercase-ASCII-letter accounting currency identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency([u8; 3]);

impl Currency {
    pub fn new(code: &str) -> Result<Self, MoneyError> {
        let bytes = code.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(u8::is_ascii_uppercase) {
            return Err(MoneyError::InvalidCurrency);
        }
        Ok(Self([bytes[0], bytes[1], bytes[2]]))
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.0[0] as char, self.0[1] as char, self.0[2] as char
        )
    }
}

impl Serialize for Currency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).map_err(de::Error::custom)
    }
}

/// A nonnegative amount in integer minor units. This is not an approval token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawMoney")]
pub struct Money {
    minor_units: i64,
    currency: Currency,
}

impl Money {
    pub fn new(minor_units: i64, currency: Currency) -> Result<Self, MoneyError> {
        if minor_units < 0 {
            return Err(MoneyError::NegativeExposure);
        }
        Ok(Self {
            minor_units,
            currency,
        })
    }

    pub fn minor_units(self) -> i64 {
        self.minor_units
    }

    pub fn currency(self) -> Currency {
        self.currency
    }

    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        self.require_currency(other.currency)?;
        let minor_units = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or(MoneyError::Overflow)?;
        Self::new(minor_units, self.currency)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, MoneyError> {
        self.require_currency(other.currency)?;
        let minor_units = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or(MoneyError::Overflow)?;
        Self::new(minor_units, self.currency)
    }

    pub fn apply_correction(self, correction: Correction) -> Result<Self, MoneyError> {
        self.require_currency(correction.currency)?;
        let minor_units = self
            .minor_units
            .checked_add(correction.minor_units)
            .ok_or(MoneyError::Overflow)?;
        Self::new(minor_units, self.currency)
    }

    fn require_currency(self, currency: Currency) -> Result<(), MoneyError> {
        if self.currency != currency {
            Err(MoneyError::CurrencyMismatch)
        } else {
            Ok(())
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMoney {
    minor_units: i64,
    currency: Currency,
}

impl TryFrom<RawMoney> for Money {
    type Error = MoneyError;

    fn try_from(raw: RawMoney) -> Result<Self, Self::Error> {
        Self::new(raw.minor_units, raw.currency)
    }
}

/// A signed correction, separate from nonnegative forecasts/authorizations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Correction {
    minor_units: i64,
    currency: Currency,
}

impl Correction {
    pub fn new(minor_units: i64, currency: Currency) -> Self {
        Self {
            minor_units,
            currency,
        }
    }

    pub fn minor_units(self) -> i64 {
        self.minor_units
    }

    pub fn currency(self) -> Currency {
        self.currency
    }
}

/// Four distinct categories in one accounting currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawCostBreakdown")]
pub struct CostBreakdown {
    pub ai_api: Money,
    pub compute: Money,
    pub storage: Money,
    pub transfer: Money,
}

impl CostBreakdown {
    pub fn validate(&self) -> Result<(), MoneyError> {
        self.total().map(|_| ())
    }

    pub fn total(&self) -> Result<Money, MoneyError> {
        self.ai_api
            .checked_add(self.compute)?
            .checked_add(self.storage)?
            .checked_add(self.transfer)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCostBreakdown {
    ai_api: Money,
    compute: Money,
    storage: Money,
    transfer: Money,
}

impl TryFrom<RawCostBreakdown> for CostBreakdown {
    type Error = MoneyError;

    fn try_from(raw: RawCostBreakdown) -> Result<Self, Self::Error> {
        let result = Self {
            ai_api: raw.ai_api,
            compute: raw.compute,
            storage: raw.storage,
            transfer: raw.transfer,
        };
        result.validate()?;
        Ok(result)
    }
}

/// Distinct scenario forecasts. Forecasts never imply permission to spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawCostScenarios")]
pub struct CostScenarios {
    pub optimistic: CostBreakdown,
    pub expected: CostBreakdown,
    pub pessimistic: CostBreakdown,
}

impl CostScenarios {
    pub fn validate(&self) -> Result<(), MoneyError> {
        let optimistic = self.optimistic.total()?;
        let expected = self.expected.total()?;
        let pessimistic = self.pessimistic.total()?;
        optimistic.require_currency(expected.currency)?;
        optimistic.require_currency(pessimistic.currency)?;
        if optimistic.minor_units > expected.minor_units
            || expected.minor_units > pessimistic.minor_units
        {
            return Err(MoneyError::ScenarioOrder);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCostScenarios {
    optimistic: CostBreakdown,
    expected: CostBreakdown,
    pessimistic: CostBreakdown,
}

impl TryFrom<RawCostScenarios> for CostScenarios {
    type Error = MoneyError;

    fn try_from(raw: RawCostScenarios) -> Result<Self, Self::Error> {
        let result = Self {
            optimistic: raw.optimistic,
            expected: raw.expected,
            pessimistic: raw.pessimistic,
        };
        result.validate()?;
        Ok(result)
    }
}

/// Stable, data-independent failures without embedding untrusted monetary input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoneyError {
    InvalidCurrency,
    NegativeExposure,
    CurrencyMismatch,
    Overflow,
    ScenarioOrder,
}

impl fmt::Display for MoneyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCurrency => "currency must contain exactly three uppercase ASCII letters",
            Self::NegativeExposure => "an exposure cannot be negative",
            Self::CurrencyMismatch => "monetary currencies do not match",
            Self::Overflow => "monetary arithmetic exceeds signed integer minor-unit capacity",
            Self::ScenarioOrder => {
                "forecast totals must satisfy optimistic <= expected <= pessimistic"
            }
        })
    }
}

impl std::error::Error for MoneyError {}
