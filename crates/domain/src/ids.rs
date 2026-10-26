//! Opaque identifiers: 1–64 lowercase ASCII letters/digits, `_` or `-`,
//! starting with a letter or digit. No whitespace, normalization or silent aliases.
//!
//! Identifiers from different domains are not interchangeable:
//! ```compile_fail
//! use rolvya_domain::ids::{TaskId, AttemptId};
//! let task: TaskId = AttemptId::new("attempt-1").unwrap();
//! ```
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidIdentifier;
impl fmt::Display for InvalidIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "expected 1–64 lowercase ASCII identifier characters, starting with a letter or digit",
        )
    }
}
impl std::error::Error for InvalidIdentifier {}

fn valid_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_' || *byte == b'-'
        })
}

macro_rules! identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidIdentifier> {
                let value = value.into();
                if !valid_id(&value) {
                    return Err(InvalidIdentifier);
                }
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl TryFrom<String> for $name {
            type Error = InvalidIdentifier;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
        impl FromStr for $name {
            type Err = InvalidIdentifier;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
identifier!(ProjectId);
identifier!(PlanId);
identifier!(TaskId);
identifier!(AttemptId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidPlanVersion;
impl fmt::Display for InvalidPlanVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a positive plan version")
    }
}
impl std::error::Error for InvalidPlanVersion {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct PlanVersion(u64);
impl PlanVersion {
    pub fn new(value: u64) -> Result<Self, InvalidPlanVersion> {
        if value == 0 {
            Err(InvalidPlanVersion)
        } else {
            Ok(Self(value))
        }
    }
    pub fn get(self) -> u64 {
        self.0
    }
}
impl TryFrom<u64> for PlanVersion {
    type Error = InvalidPlanVersion;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<PlanVersion> for u64 {
    fn from(value: PlanVersion) -> u64 {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_identifiers_and_serde_cannot_bypass_validation() {
        for value in ["", " ", "UPPER", "a/b", "_a", "a\nb", "é", &"a".repeat(65)] {
            assert!(TaskId::new(value).is_err(), "accepted malformed identifier");
            assert!(serde_json::from_value::<TaskId>(serde_json::json!(value)).is_err());
        }
    }
    #[test]
    fn boundaries_and_versions() {
        assert!(TaskId::new("a").is_ok());
        assert!(TaskId::new("a".repeat(64)).is_ok());
        assert!(PlanVersion::new(0).is_err());
        assert!(serde_json::from_str::<PlanVersion>("0").is_err());
        assert_eq!(PlanVersion::new(u64::MAX).unwrap().get(), u64::MAX);
        let id = ProjectId::new("project-1_x").unwrap();
        assert_eq!(
            serde_json::from_str::<ProjectId>(&serde_json::to_string(&id).unwrap()).unwrap(),
            id
        );
    }
}
