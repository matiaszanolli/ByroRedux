use byroredux_sdk::identity::PrincipalId;

use crate::{Result, SandboxError};

const MAX_DISPLAY_NAME_BYTES: usize = 256;

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
        if display_name.chars().any(char::is_control) {
            return Err(SandboxError::InvalidPrincipal(
                "display name must not contain control characters".to_owned(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_display_names_are_bounded() {
        let id = PrincipalId::new("org.example.weather-overhaul").unwrap();
        assert!(Principal::new(id.clone(), "Weather overhaul").is_ok());
        assert!(Principal::new(id.clone(), " ").is_err());
        assert!(Principal::new(id, "x".repeat(MAX_DISPLAY_NAME_BYTES + 1)).is_err());
    }
}
