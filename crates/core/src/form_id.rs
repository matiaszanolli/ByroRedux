//! Stable, load-order-independent record identity.
//!
//! Legacy Gamebryo/Creation Engine Form IDs encode the plugin's load-order
//! slot in the upper 8 bits — meaning the same record gets a different ID
//! depending on what other plugins are loaded. This module replaces that
//! with a two-layer scheme:
//!
//! - **[`FormIdPair`]** — the canonical identity: `(PluginId, LocalFormId)`.
//!   Stable across sessions, serialized to saves and manifests.
//! - **[`FormId`]** — a runtime handle interned via [`FormIdPool`].
//!   Integer comparison, O(1). Same pattern as
//!   [`FixedString`](crate::string::FixedString) / [`StringPool`](crate::string::StringPool).
//!
//! Legacy Form IDs are converted at load time by deriving a deterministic
//! [`PluginId`] from the plugin filename (UUID v5), then pairing it with
//! the 24-bit local ID. After conversion there is no distinction between
//! legacy and Redux-native plugins.

use crate::ecs::resource::Resource;
use std::collections::HashMap;
use uuid::Uuid;

/// Fixed UUID v5 namespace for deriving [`PluginId`] from legacy filenames.
/// Generated once, used everywhere — ensures the same .esm always maps to
/// the same PluginId regardless of load order or session.
const PLUGIN_NAMESPACE: Uuid = Uuid::from_bytes([
    0x67, 0x61, 0x6d, 0x65, // "game"
    0x62, 0x79, // "by"
    0x72, 0x6f, // "ro"
    0x2d, 0x72, // "-r"
    0x65, 0x64, 0x75, 0x78, // "edux"
    0x21, 0x00, // "!\0"
]);

// ── Runtime handle ──────────────────────────────────────────────────────

/// Runtime form identifier. Integer comparison, O(1).
///
/// Never stored in saves — this is a session-local handle into
/// [`FormIdPool`]. Use [`FormIdPair`] for anything that must survive
/// across sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormId(u64);

// ── Stable identity types ───────────────────────────────────────────────

/// Content-addressed plugin identifier.
///
/// For legacy files: deterministic UUID v5 derived from the filename,
/// so `Skyrim.esm` always produces the same `PluginId` regardless of
/// load order.
///
/// For Redux-native plugins: declared UUID in the plugin manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginId(pub u128);

impl PluginId {
    /// Derive a deterministic `PluginId` from a legacy plugin filename.
    ///
    /// Uses UUID v5 with a fixed engine namespace so the same filename
    /// always produces the same identity.
    ///
    /// ```
    /// # use gamebyro_core::form_id::PluginId;
    /// let a = PluginId::from_filename("Skyrim.esm");
    /// let b = PluginId::from_filename("Skyrim.esm");
    /// assert_eq!(a, b);
    ///
    /// let c = PluginId::from_filename("Dawnguard.esm");
    /// assert_ne!(a, c);
    /// ```
    pub fn from_filename(name: &str) -> Self {
        let uuid = Uuid::new_v5(&PLUGIN_NAMESPACE, name.as_bytes());
        Self(uuid.as_u128())
    }

    /// Create a `PluginId` from an existing UUID (for native plugin manifests).
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid.as_u128())
    }

    /// Convert back to a [`Uuid`] for serialization or display.
    pub fn as_uuid(&self) -> Uuid {
        Uuid::from_u128(self.0)
    }
}

/// A record's identity within its originating plugin.
///
/// For legacy files this is the lower 24 bits of the original Form ID.
/// For Redux-native plugins this is assigned by the plugin author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalFormId(pub u32);

/// The canonical, load-order-independent identity of a record.
///
/// This is what gets written to save files and plugin manifests.
/// At runtime it is interned into a [`FormId`] via [`FormIdPool`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormIdPair {
    pub plugin: PluginId,
    pub local: LocalFormId,
}

// ── Runtime interning pool ──────────────────────────────────────────────

/// Maps [`FormIdPair`] ↔ [`FormId`]. Same pattern as
/// [`StringPool`](crate::string::StringPool) /
/// [`FixedString`](crate::string::FixedString).
///
/// Register as a global [`Resource`] on the [`World`](crate::ecs::world::World):
///
/// ```ignore
/// world.insert_resource(FormIdPool::new());
/// ```
pub struct FormIdPool {
    to_runtime: HashMap<FormIdPair, FormId>,
    to_pair: Vec<FormIdPair>,
}

impl FormIdPool {
    pub fn new() -> Self {
        Self {
            to_runtime: HashMap::new(),
            to_pair: Vec::new(),
        }
    }

    /// Intern a [`FormIdPair`], returning its runtime [`FormId`].
    /// If the pair was already interned, returns the existing handle.
    pub fn intern(&mut self, pair: FormIdPair) -> FormId {
        if let Some(&id) = self.to_runtime.get(&pair) {
            return id;
        }
        let id = FormId(self.to_pair.len() as u64);
        self.to_pair.push(pair);
        self.to_runtime.insert(pair, id);
        id
    }

    /// Resolve a runtime [`FormId`] back to its canonical [`FormIdPair`].
    pub fn resolve(&self, id: FormId) -> Option<&FormIdPair> {
        self.to_pair.get(id.0 as usize)
    }

    /// Look up a [`FormIdPair`] without interning it.
    /// Returns `None` if the pair has never been interned.
    pub fn get(&self, pair: &FormIdPair) -> Option<FormId> {
        self.to_runtime.get(pair).copied()
    }

    /// Number of interned form IDs.
    pub fn len(&self) -> usize {
        self.to_pair.len()
    }

    /// Returns `true` if no form IDs have been interned.
    pub fn is_empty(&self) -> bool {
        self.to_pair.is_empty()
    }
}

impl Default for FormIdPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Resource for FormIdPool {}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── PluginId ────────────────────────────────────────────────────────

    #[test]
    fn plugin_id_from_filename_deterministic() {
        let a = PluginId::from_filename("Skyrim.esm");
        let b = PluginId::from_filename("Skyrim.esm");
        assert_eq!(a, b);
    }

    #[test]
    fn different_filenames_different_plugin_ids() {
        let a = PluginId::from_filename("Skyrim.esm");
        let b = PluginId::from_filename("Dawnguard.esm");
        assert_ne!(a, b);
    }

    #[test]
    fn plugin_id_uuid_round_trip() {
        let original = PluginId::from_filename("Fallout4.esm");
        let uuid = original.as_uuid();
        let restored = PluginId::from_uuid(uuid);
        assert_eq!(original, restored);
    }

    // ── FormIdPool ──────────────────────────────────────────────────────

    fn test_pair(plugin_name: &str, local: u32) -> FormIdPair {
        FormIdPair {
            plugin: PluginId::from_filename(plugin_name),
            local: LocalFormId(local),
        }
    }

    #[test]
    fn intern_same_pair_returns_same_id() {
        let mut pool = FormIdPool::new();
        let pair = test_pair("Skyrim.esm", 0x000014);
        let a = pool.intern(pair);
        let b = pool.intern(pair);
        assert_eq!(a, b);
    }

    #[test]
    fn different_pairs_different_ids() {
        let mut pool = FormIdPool::new();
        let a = pool.intern(test_pair("Skyrim.esm", 0x000014));
        let b = pool.intern(test_pair("Skyrim.esm", 0x000015));
        let c = pool.intern(test_pair("Dawnguard.esm", 0x000014));
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn resolve_round_trips() {
        let mut pool = FormIdPool::new();
        let pair = test_pair("Skyrim.esm", 0xABC);
        let id = pool.intern(pair);
        let resolved = pool.resolve(id).unwrap();
        assert_eq!(*resolved, pair);
    }

    #[test]
    fn get_without_interning() {
        let mut pool = FormIdPool::new();
        let pair = test_pair("Skyrim.esm", 0x001);
        assert!(pool.get(&pair).is_none());

        let id = pool.intern(pair);
        assert_eq!(pool.get(&pair), Some(id));

        let other = test_pair("Skyrim.esm", 0x002);
        assert!(pool.get(&other).is_none());
    }

    #[test]
    fn len_and_is_empty() {
        let mut pool = FormIdPool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);

        pool.intern(test_pair("Skyrim.esm", 0x001));
        assert!(!pool.is_empty());
        assert_eq!(pool.len(), 1);

        pool.intern(test_pair("Skyrim.esm", 0x002));
        assert_eq!(pool.len(), 2);

        // Re-interning doesn't grow.
        pool.intern(test_pair("Skyrim.esm", 0x001));
        assert_eq!(pool.len(), 2);
    }
}
