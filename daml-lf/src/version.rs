use std::convert::TryFrom;
use std::fmt::{Display, Error, Formatter};

use bounded_static::ToStatic;
use serde::Serialize;

use crate::DamlLfError;

/// Daml-LF language version. v2 of this crate dropped support for the
/// LF1 line of minor versions; `Lv2` is the only major-version variant
/// the enum needs today. The enum shape is preserved so a future LF3
/// can slot in alongside without breaking the public API.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, ToStatic)]
pub enum LanguageVersion {
    Lv2(LanguageV2MinorVersion),
}

impl LanguageVersion {
    /// LF 2.1 — the first stable LF2 minor version.
    pub const V2_1: LanguageVersion = LanguageVersion::Lv2(LanguageV2MinorVersion::V1);
    /// LF 2.dev — staging area for the next LF2 minor version.
    pub const V2_DEV: LanguageVersion = LanguageVersion::Lv2(LanguageV2MinorVersion::Dev);

    pub fn new_v2(minor: LanguageV2MinorVersion) -> Self {
        LanguageVersion::Lv2(minor)
    }
}

impl Display for LanguageVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match *self {
            LanguageVersion::Lv2(minor) => write!(f, "v2.{}", minor),
        }
    }
}

/// LF2 minor version.
///
/// Per `daml_lf2.proto`, LF2 started at minor `2.1` (no `2.0` exists).
/// New minor versions are added by extending this enum; the ordering
/// of variants must be ascending so the derived `Ord` works.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, ToStatic)]
pub enum LanguageV2MinorVersion {
    V1,
    Dev,
}

impl Display for LanguageV2MinorVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match *self {
            LanguageV2MinorVersion::V1 => write!(f, "1"),
            LanguageV2MinorVersion::Dev => write!(f, "dev"),
        }
    }
}

impl TryFrom<&str> for LanguageV2MinorVersion {
    type Error = DamlLfError;

    fn try_from(minor_version: &str) -> Result<Self, Self::Error> {
        match minor_version {
            "1" => Ok(LanguageV2MinorVersion::V1),
            "dev" => Ok(LanguageV2MinorVersion::Dev),
            _ => Err(DamlLfError::new_unknown_version(minor_version)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LanguageV2MinorVersion, LanguageVersion};

    #[test]
    fn minor_version_ordering() {
        assert!(LanguageV2MinorVersion::V1 < LanguageV2MinorVersion::Dev);
    }

    #[test]
    fn version_matches() {
        assert_eq!(LanguageVersion::V2_1, LanguageVersion::Lv2(LanguageV2MinorVersion::V1));
        assert_ne!(LanguageVersion::V2_1, LanguageVersion::V2_DEV);
    }

    #[test]
    fn display_version() {
        assert_eq!("v2.1", LanguageVersion::V2_1.to_string());
        assert_eq!("v2.dev", LanguageVersion::V2_DEV.to_string());
    }
}
