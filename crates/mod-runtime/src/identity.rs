use crate::{Result, SandboxError};
use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::fmt;

/// Capability required to emit a host-attributed log record.
pub const LOG_CAPABILITY: &str = "byro.host.log";

const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_DISPLAY_NAME_BYTES: usize = 256;

/// Stable identity used for grants, budgets, state, logs, and fault attribution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PrincipalId(String);

impl PrincipalId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_identifier(&value)
            .map_err(|reason| SandboxError::InvalidPrincipal(reason.to_owned()))?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrincipalId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Human-facing information associated with a stable principal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Principal {
    id: PrincipalId,
    display_name: String,
}

impl Principal {
    pub fn new(id: PrincipalId, display_name: impl Into<String>) -> Result<Self> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(SandboxError::InvalidPrincipal(
                "display name must not be empty".to_owned(),
            ));
        }
        if display_name.len() > MAX_DISPLAY_NAME_BYTES {
            return Err(SandboxError::InvalidPrincipal(format!(
                "display name exceeds {MAX_DISPLAY_NAME_BYTES} bytes"
            )));
        }
        Ok(Self { id, display_name })
    }

    pub fn id(&self) -> &PrincipalId {
        &self.id
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// Validated, namespaced unit of host authority.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapabilityId(String);

impl CapabilityId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        validate_identifier(&value)
            .map_err(|reason| SandboxError::InvalidCapability(reason.to_owned()))?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for CapabilityId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

/// Effective capabilities granted to one principal.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilitySet {
    granted: BTreeSet<CapabilityId>,
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&mut self, capability: impl Into<String>) -> Result<bool> {
        Ok(self.granted.insert(CapabilityId::new(capability)?))
    }

    pub fn contains(&self, capability: &str) -> bool {
        self.granted.contains(capability)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CapabilityId> {
        self.granted.iter()
    }
}

fn validate_identifier(value: &str) -> std::result::Result<(), &'static str> {
    if value.is_empty() {
        return Err("identifier must not be empty");
    }
    if value.len() > MAX_IDENTIFIER_BYTES {
        return Err("identifier exceeds 128 bytes");
    }
    if !value.as_bytes()[0].is_ascii_alphanumeric() {
        return Err("identifier must start with an ASCII letter or digit");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err("identifier contains unsupported characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_stable_ascii_names() {
        assert!(PrincipalId::new("org.example.weather-overhaul").is_ok());
        assert!(PrincipalId::new("../escape").is_err());
        assert!(PrincipalId::new("contains spaces").is_err());
        assert!(PrincipalId::new("").is_err());
    }

    #[test]
    fn capabilities_are_deduplicated() {
        let mut grants = CapabilitySet::new();
        assert!(grants.grant(LOG_CAPABILITY).unwrap());
        assert!(!grants.grant(LOG_CAPABILITY).unwrap());
        assert!(grants.contains(LOG_CAPABILITY));
        assert_eq!(grants.iter().count(), 1);
    }
}
