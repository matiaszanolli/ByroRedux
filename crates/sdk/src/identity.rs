use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::fmt;
use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

const MAX_NAMESPACED_ID_BYTES: usize = 128;

/// Failure to construct a stable SDK identifier.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdentityError {
    /// The supplied identifier was empty.
    #[error("identifier must not be empty")]
    Empty,
    /// The supplied identifier exceeded the wire-contract bound.
    #[error("identifier exceeds {maximum} bytes")]
    TooLong { maximum: usize },
    /// The first byte was not an ASCII letter or digit.
    #[error("identifier must start with an ASCII letter or digit")]
    InvalidStart,
    /// The identifier contained a character outside the portable grammar.
    #[error("identifier contains an unsupported character at byte {index}")]
    InvalidCharacter { index: usize },
}

fn validate_namespaced_id(value: &str) -> Result<(), IdentityError> {
    if value.is_empty() {
        return Err(IdentityError::Empty);
    }
    if value.len() > MAX_NAMESPACED_ID_BYTES {
        return Err(IdentityError::TooLong {
            maximum: MAX_NAMESPACED_ID_BYTES,
        });
    }
    if !value.as_bytes()[0].is_ascii_alphanumeric() {
        return Err(IdentityError::InvalidStart);
    }
    if let Some((index, _)) = value.bytes().enumerate().find(|(_, byte)| {
        !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    }) {
        return Err(IdentityError::InvalidCharacter { index });
    }
    Ok(())
}

macro_rules! namespaced_id {
    ($name:ident, $docs:literal) => {
        #[doc = $docs]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Construct and validate an identifier.
            pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
                let value = value.into();
                validate_namespaced_id(&value)?;
                Ok(Self(value))
            }

            /// Return the stable wire spelling.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

namespaced_id!(
    ExtensionId,
    "Stable identity of an installed extension package, independent of paths and load order."
);
namespaced_id!(
    PrincipalId,
    "Security principal used for grants, state, budgets, diagnostics, and faults."
);
namespaced_id!(
    CapabilityId,
    "Namespaced unit of host authority requested by an extension and granted by policy."
);
namespaced_id!(
    ServiceId,
    "Namespaced semantic engine or community service identifier."
);
namespaced_id!(
    EventId,
    "Namespaced canonical engine or community event identifier."
);
namespaced_id!(
    ComponentSchemaId,
    "Namespaced identifier for an engine-owned dynamic extension-component schema."
);
namespaced_id!(
    ComponentFieldId,
    "Manifest-local identifier for one field in an extension-component schema."
);
namespaced_id!(
    ComponentId,
    "Manifest-local identifier for one executable WebAssembly component."
);
namespaced_id!(
    ConsoleCommandId,
    "Manifest-local identifier for one engine-console command."
);
namespaced_id!(
    SettingId,
    "Manifest-local identifier for one principal-owned engine setting."
);
namespaced_id!(
    StorageKey,
    "Stable key in one principal's private persistent storage namespace."
);

impl From<&ExtensionId> for PrincipalId {
    fn from(value: &ExtensionId) -> Self {
        Self(value.0.clone())
    }
}

/// Deterministic set of effective capabilities granted to one principal.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilitySet {
    granted: BTreeSet<CapabilityId>,
}

impl CapabilitySet {
    /// Construct an empty grant set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate and add a capability by its wire spelling.
    pub fn grant(&mut self, capability: impl Into<String>) -> Result<bool, IdentityError> {
        Ok(self.granted.insert(CapabilityId::new(capability)?))
    }

    /// Add an already validated capability.
    pub fn grant_id(&mut self, capability: CapabilityId) -> bool {
        self.granted.insert(capability)
    }

    /// Return whether this set includes the named capability.
    pub fn contains(&self, capability: &str) -> bool {
        self.granted.contains(capability)
    }

    /// Iterate in stable lexical order.
    pub fn iter(&self) -> impl Iterator<Item = &CapabilityId> {
        self.granted.iter()
    }
}

/// Stable, document-local identity for an object exposed through the SDK.
///
/// The value is deliberately unrelated to the host ECS entity ID. Hosts must
/// preserve the mapping for the lifetime of a document and reproduce the same
/// IDs when unchanged source content is imported in the same canonical order.
/// Zero is reserved so absent or invalid IDs stay distinguishable at FFI and
/// serialization boundaries.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectId(NonZeroU64);

impl ObjectId {
    /// Construct an ID from its non-zero wire value.
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    /// Assign an ID from a zero-based canonical import ordinal.
    pub fn from_import_ordinal(ordinal: usize) -> Option<Self> {
        u64::try_from(ordinal)
            .ok()
            .and_then(|value| value.checked_add(1))
            .and_then(Self::new)
    }

    /// Return the stable non-zero wire value.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Opaque reference to a live runtime entity exposed across the SDK boundary.
///
/// Hosts issue a new non-zero world generation whenever a live world is
/// replaced. `object` is host-assigned within that generation and is never a
/// raw ECS slot, pointer, or load-order-dependent form identifier. Together
/// the pair makes stale handles rejectable without exposing engine internals.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct EntityRef {
    world_generation: NonZeroU64,
    object: NonZeroU64,
}

/// Load-order-independent identity for a plugin-authored runtime object.
///
/// `source` is the 128-bit stable content/plugin identity in network byte
/// order; `local` is the source-local record identity. Unlike [`EntityRef`],
/// this value is suitable for save data and survives world replacement.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct FormRef {
    source: [u8; 16],
    local: u32,
}

impl FormRef {
    /// Construct a stable form reference from its portable wire parts.
    pub const fn new(source: [u8; 16], local: u32) -> Self {
        Self { source, local }
    }

    /// Stable 128-bit source identity in network byte order.
    pub const fn source(self) -> [u8; 16] {
        self.source
    }

    /// Source-local record identity.
    pub const fn local(self) -> u32 {
        self.local
    }
}

impl EntityRef {
    /// Construct a handle from its two non-zero wire values.
    pub const fn new(world_generation: u64, object: u64) -> Option<Self> {
        match (NonZeroU64::new(world_generation), NonZeroU64::new(object)) {
            (Some(world_generation), Some(object)) => Some(Self {
                world_generation,
                object,
            }),
            _ => None,
        }
    }

    /// Generation of the live world that issued this handle.
    pub const fn world_generation(self) -> u64 {
        self.world_generation.get()
    }

    /// Opaque object value within the issuing world generation.
    pub const fn object(self) -> u64 {
        self.object.get()
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_ids_reserve_zero_and_assign_one_based_ordinals() {
        assert_eq!(ObjectId::new(0), None);
        assert_eq!(ObjectId::from_import_ordinal(0).unwrap().get(), 1);
        assert_eq!(ObjectId::from_import_ordinal(41).unwrap().get(), 42);
    }

    #[test]
    fn entity_refs_reserve_zero_in_both_coordinates() {
        assert_eq!(EntityRef::new(0, 1), None);
        assert_eq!(EntityRef::new(1, 0), None);
        let entity = EntityRef::new(7, 42).unwrap();
        assert_eq!(entity.world_generation(), 7);
        assert_eq!(entity.object(), 42);
    }

    #[test]
    fn form_refs_round_trip_portable_parts() {
        let source = 0x0123_4567_89ab_cdef_fedc_ba98_7654_3210_u128.to_be_bytes();
        let form = FormRef::new(source, 0x123456);
        assert_eq!(form.source(), source);
        assert_eq!(form.local(), 0x123456);
        assert_eq!(
            serde_json::from_str::<FormRef>(&serde_json::to_string(&form).unwrap()).unwrap(),
            form
        );
    }

    #[test]
    fn namespaced_ids_reject_path_and_whitespace_spoofing() {
        assert!(ExtensionId::new("org.example.weather").is_ok());
        assert!(CapabilityId::new("byro.world.transform.read").is_ok());
        assert!(ExtensionId::new("../escape").is_err());
        assert!(ExtensionId::new("contains spaces").is_err());
        assert!(ExtensionId::new("").is_err());
    }

    #[test]
    fn capability_sets_are_validated_and_deduplicated() {
        let mut grants = CapabilitySet::new();
        assert!(grants.grant("byro.log.write").unwrap());
        assert!(!grants.grant("byro.log.write").unwrap());
        assert!(grants.contains("byro.log.write"));
        assert_eq!(grants.iter().count(), 1);
    }
}
