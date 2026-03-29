//! Legacy Form ID bridge: converts load-order-dependent Bethesda Form IDs
//! into stable [`FormIdPair`]s.
//!
//! After conversion through [`LegacyLoadOrder::resolve`], legacy data is
//! indistinguishable from Redux-native plugins — same types, same
//! resolution path, same storage in [`DataStore`](crate::DataStore).
//!
//! # Form ID layouts
//!
//! **Standard (ESM/ESP):** `0xPP_LLLLLL`
//! - `PP` = 8-bit plugin slot index (load order)
//! - `LLLLLL` = 24-bit local record ID
//!
//! **ESL (Light Master, slot 0xFE):** `0xFE_III_FFF`
//! - `III` = 12-bit ESL sub-index (bits 12..23)
//! - `FFF` = 12-bit local form ID (bits 0..11)
//!
//! **ESH (Medium Master, Starfield 1.11+, slot 0xFD):** `0xFD_SS_FFFF`
//! - `SS` = 8-bit sub-index (bits 16..23)
//! - `FFFF` = 16-bit local form ID (bits 0..15)
//!
//! **Save-generated (slot 0xFF):** ephemeral references (PlaceAtMe, fired
//! arrows, ash piles) — must never be interned as stable identities.

pub mod fo4;
pub mod tes3;
pub mod tes4;
pub mod tes5;

use gamebyro_core::form_id::{FormIdPair, LocalFormId, PluginId};

// ── LegacyFormId ────────────────────────────────────────────────────────

/// Raw legacy form ID as found in ESM/ESP/ESL binary files.
///
/// The upper 8 bits encode the plugin's load-order slot, making the same
/// record produce different IDs depending on what other plugins are loaded.
/// [`LegacyLoadOrder::resolve`] converts these into stable [`FormIdPair`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LegacyFormId(pub u32);

impl LegacyFormId {
    /// Upper 8 bits — plugin slot index in load order.
    pub fn plugin_index(&self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// Lower 24 bits — record ID within the plugin.
    pub fn local_id(&self) -> u32 {
        self.0 & 0x00FF_FFFF
    }

    /// Returns true if this is a save-generated reference (slot 0xFF).
    ///
    /// These are ephemeral (PlaceAtMe, fired arrows, ash piles) and must
    /// never be interned into [`FormIdPool`](gamebyro_core::form_id::FormIdPool)
    /// as stable identities.
    pub fn is_save_generated(&self) -> bool {
        self.plugin_index() == 0xFF
    }

    /// Returns true if this is a null/invalid form (local_id == 0).
    pub fn is_null(&self) -> bool {
        self.local_id() == 0
    }

    /// Returns true if this is an ESL (Light Master) reference.
    ///
    /// ESL slot is `0xFE`. Bits 12..23 = ESL plugin sub-index,
    /// bits 0..11 = local form ID.
    pub fn is_esl(&self) -> bool {
        self.plugin_index() == 0xFE
    }

    /// ESL sub-index (bits 12..23). Only meaningful when [`is_esl`](Self::is_esl) is true.
    pub fn esl_index(&self) -> u16 {
        ((self.0 >> 12) & 0xFFF) as u16
    }

    /// ESL local form ID (bits 0..11). Only meaningful when [`is_esl`](Self::is_esl) is true.
    pub fn esl_local(&self) -> u16 {
        (self.0 & 0xFFF) as u16
    }

    /// Returns true if this is an ESH (Medium Master, Starfield 1.11+) reference.
    ///
    /// ESH slot is `0xFD`. Bits 16..23 = sub-index, bits 0..15 = local form ID.
    pub fn is_esh(&self) -> bool {
        self.plugin_index() == 0xFD
    }

    /// ESH sub-index (bits 16..23). Only meaningful when [`is_esh`](Self::is_esh) is true.
    pub fn esh_index(&self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }

    /// ESH local form ID (bits 0..15). Only meaningful when [`is_esh`](Self::is_esh) is true.
    pub fn esh_local(&self) -> u32 {
        self.0 & 0x0000_FFFF
    }
}

// ── LegacyLoadOrder ─────────────────────────────────────────────────────

/// Maps legacy load-order slots to stable [`PluginId`]s.
///
/// Built from the plugin list in a save file or game session. Each slot
/// type (standard, ESL, ESH) has its own namespace.
///
/// After registration, [`resolve`](Self::resolve) converts any
/// [`LegacyFormId`] into a [`FormIdPair`] using deterministic
/// [`PluginId::from_filename`].
pub struct LegacyLoadOrder {
    /// Slot 0x00–0xFC → PluginId (derived from filename).
    slots: Vec<Option<PluginId>>,
    /// ESL sub-index 0x000–0xFFF → PluginId.
    esl_slots: Vec<Option<PluginId>>,
    /// ESH sub-index 0x00–0xFF → PluginId (Starfield).
    esh_slots: Vec<Option<PluginId>>,
}

impl LegacyLoadOrder {
    pub fn new() -> Self {
        Self {
            slots: vec![None; 0xFD], // 0x00 through 0xFC
            esl_slots: vec![None; 0x1000], // 0x000 through 0xFFF
            esh_slots: vec![None; 0x100], // 0x00 through 0xFF
        }
    }

    /// Register a standard master/plugin in the given load-order slot.
    ///
    /// # Panics
    /// Panics if `slot > 0xFC` (0xFD/0xFE/0xFF are reserved).
    pub fn register(&mut self, slot: u8, filename: &str) {
        assert!(
            slot <= 0xFC,
            "slot 0x{slot:02X} is reserved (0xFD=ESH, 0xFE=ESL, 0xFF=save-generated)"
        );
        self.slots[slot as usize] = Some(PluginId::from_filename(filename));
    }

    /// Register an ESL (Light Master) plugin at the given sub-index.
    ///
    /// # Panics
    /// Panics if `index > 0xFFF`.
    pub fn register_esl(&mut self, index: u16, filename: &str) {
        assert!(index <= 0xFFF, "ESL index 0x{index:03X} out of range (max 0xFFF)");
        self.esl_slots[index as usize] = Some(PluginId::from_filename(filename));
    }

    /// Register an ESH (Medium Master, Starfield) plugin at the given sub-index.
    pub fn register_esh(&mut self, index: u8, filename: &str) {
        self.esh_slots[index as usize] = Some(PluginId::from_filename(filename));
    }

    /// Convert a legacy Form ID to a stable [`FormIdPair`].
    ///
    /// Returns `None` if:
    /// - The form is save-generated (slot 0xFF) — ephemeral, never intern
    /// - The form is null (local_id == 0) — invalid
    /// - The slot/sub-index is not registered in this load order
    pub fn resolve(&self, legacy: LegacyFormId) -> Option<FormIdPair> {
        if legacy.is_save_generated() || legacy.is_null() {
            return None;
        }

        if legacy.is_esl() {
            let plugin = *self.esl_slots.get(legacy.esl_index() as usize)?.as_ref()?;
            return Some(FormIdPair {
                plugin,
                local: LocalFormId(legacy.esl_local() as u32),
            });
        }

        if legacy.is_esh() {
            let plugin = *self.esh_slots.get(legacy.esh_index() as usize)?.as_ref()?;
            return Some(FormIdPair {
                plugin,
                local: LocalFormId(legacy.esh_local()),
            });
        }

        // Standard ESM/ESP slot.
        let plugin = *self.slots.get(legacy.plugin_index() as usize)?.as_ref()?;
        Some(FormIdPair {
            plugin,
            local: LocalFormId(legacy.local_id()),
        })
    }
}

impl Default for LegacyLoadOrder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── LegacyFormId bit extraction ─────────────────────────────────────

    #[test]
    fn plugin_index_and_local_id() {
        let fid = LegacyFormId(0x01_000ABC);
        assert_eq!(fid.plugin_index(), 0x01);
        assert_eq!(fid.local_id(), 0x000ABC);
    }

    #[test]
    fn is_save_generated() {
        assert!(LegacyFormId(0xFF_000001).is_save_generated());
        assert!(!LegacyFormId(0x01_000001).is_save_generated());
        assert!(!LegacyFormId(0xFE_000001).is_save_generated());
    }

    #[test]
    fn is_null() {
        assert!(LegacyFormId(0x01_000000).is_null());
        assert!(!LegacyFormId(0x01_000001).is_null());
        // Slot 0xFF with local 0 is both save-generated and null.
        assert!(LegacyFormId(0xFF_000000).is_null());
    }

    #[test]
    fn esl_fields() {
        let fid = LegacyFormId(0xFE_ABC123);
        assert!(fid.is_esl());
        assert!(!fid.is_esh());
        assert_eq!(fid.esl_index(), 0xABC);
        assert_eq!(fid.esl_local(), 0x123);
    }

    #[test]
    fn esh_fields() {
        let fid = LegacyFormId(0xFD_12ABCD);
        assert!(fid.is_esh());
        assert!(!fid.is_esl());
        assert_eq!(fid.esh_index(), 0x12);
        assert_eq!(fid.esh_local(), 0xABCD);
    }

    // ── LegacyLoadOrder::resolve ────────────────────────────────────────

    #[test]
    fn resolve_standard_slot() {
        let mut order = LegacyLoadOrder::new();
        order.register(0x01, "Skyrim.esm");

        let pair = order.resolve(LegacyFormId(0x01_000014)).unwrap();
        assert_eq!(pair.plugin, PluginId::from_filename("Skyrim.esm"));
        assert_eq!(pair.local, LocalFormId(0x000014));
    }

    #[test]
    fn resolve_returns_none_for_save_generated() {
        let order = LegacyLoadOrder::new();
        assert!(order.resolve(LegacyFormId(0xFF_000001)).is_none());
    }

    #[test]
    fn resolve_returns_none_for_null() {
        let mut order = LegacyLoadOrder::new();
        order.register(0x01, "Skyrim.esm");
        assert!(order.resolve(LegacyFormId(0x01_000000)).is_none());
    }

    #[test]
    fn resolve_returns_none_for_unregistered_slot() {
        let order = LegacyLoadOrder::new();
        assert!(order.resolve(LegacyFormId(0x05_000001)).is_none());
    }

    #[test]
    fn resolve_esl_slot() {
        let mut order = LegacyLoadOrder::new();
        order.register_esl(0x00A, "LightMod.esl");

        // 0xFE + sub-index 0x00A in bits 12..23 + local 0xFA0 in bits 0..11
        let raw = 0xFE_000000 | (0x00A << 12) | 0xFA0;
        let pair = order.resolve(LegacyFormId(raw)).unwrap();
        assert_eq!(pair.plugin, PluginId::from_filename("LightMod.esl"));
        assert_eq!(pair.local, LocalFormId(0xFA0));
    }

    #[test]
    fn resolve_esl_unregistered_returns_none() {
        let order = LegacyLoadOrder::new();
        assert!(order.resolve(LegacyFormId(0xFE_ABC123)).is_none());
    }

    #[test]
    fn resolve_esh_slot() {
        let mut order = LegacyLoadOrder::new();
        order.register_esh(0x05, "MediumMod.esm");

        let raw = 0xFD_000000 | (0x05 << 16) | 0x1234;
        let pair = order.resolve(LegacyFormId(raw)).unwrap();
        assert_eq!(pair.plugin, PluginId::from_filename("MediumMod.esm"));
        assert_eq!(pair.local, LocalFormId(0x1234));
    }

    #[test]
    fn same_filename_same_plugin_id_deterministic() {
        let mut order_a = LegacyLoadOrder::new();
        order_a.register(0x00, "Skyrim.esm");

        let mut order_b = LegacyLoadOrder::new();
        order_b.register(0x05, "Skyrim.esm"); // different slot, same filename

        let pair_a = order_a.resolve(LegacyFormId(0x00_000014)).unwrap();
        let pair_b = order_b.resolve(LegacyFormId(0x05_000014)).unwrap();

        // Same filename → same PluginId, regardless of slot
        assert_eq!(pair_a.plugin, pair_b.plugin);
        assert_eq!(pair_a.local, pair_b.local);
    }

    #[test]
    fn slot_zero_resolves() {
        let mut order = LegacyLoadOrder::new();
        order.register(0x00, "Fallout4.esm");

        let pair = order.resolve(LegacyFormId(0x00_000001)).unwrap();
        assert_eq!(pair.plugin, PluginId::from_filename("Fallout4.esm"));
        assert_eq!(pair.local, LocalFormId(0x000001));
    }

    #[test]
    fn max_standard_slot_resolves() {
        let mut order = LegacyLoadOrder::new();
        order.register(0xFC, "LastPlugin.esp");

        let pair = order.resolve(LegacyFormId(0xFC_000001)).unwrap();
        assert_eq!(pair.plugin, PluginId::from_filename("LastPlugin.esp"));
    }

    #[test]
    #[should_panic(expected = "reserved")]
    fn register_reserved_slot_panics() {
        let mut order = LegacyLoadOrder::new();
        order.register(0xFE, "Bad.esm");
    }
}
