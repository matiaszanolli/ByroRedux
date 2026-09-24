//! The active load order's global-FormID ↔ portable-identity bridge.
//!
//! ESM records are remapped into one global FormID space at parse, whose top
//! byte (or light/medium sub-index) is a *load-order slot* — meaningless once
//! the load order changes. Anything that must outlive the session (save data)
//! or cross the SDK boundary names a record by its owning plugin's stable
//! [`PluginId`] plus the plugin-local object id instead. This resource holds
//! the slot → plugin table that converts between the two, so a system in this
//! crate can persist a FormID without reaching into the binary's
//! `GlobalFormIdResolver`, which delegates here (#4414).

use byroredux_core::ecs::resource::Resource;
use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};
use byroredux_plugin::esm::reader::GlobalSlot;
use byroredux_sdk::identity::FormRef;

/// Slot → owning plugin for every plugin in the active load order.
#[derive(Clone, Debug, Default)]
pub struct LoadOrderIdentity {
    owners: Vec<(GlobalSlot, PluginId)>,
}

impl Resource for LoadOrderIdentity {}

impl LoadOrderIdentity {
    pub fn new(owners: Vec<(GlobalSlot, PluginId)>) -> Self {
        Self { owners }
    }

    pub fn owners(&self) -> &[(GlobalSlot, PluginId)] {
        &self.owners
    }

    /// The owning plugin and plugin-local object id of a global FormID.
    pub fn pair(&self, form_id: u32) -> Option<FormIdPair> {
        let slot = global_slot_of(form_id);
        let plugin = self
            .owners
            .iter()
            .find_map(|(owner, plugin)| (*owner == slot).then_some(*plugin))?;
        Some(FormIdPair {
            plugin,
            local: LocalFormId(form_id & object_id_mask(slot)),
        })
    }

    /// The load-order-independent identity of a global FormID.
    pub fn form_ref(&self, form_id: u32) -> Option<FormRef> {
        self.pair(form_id)
            .map(|pair| FormRef::new(pair.plugin.0.to_be_bytes(), pair.local.0))
    }

    /// The global FormID a portable identity maps to in this load order, or
    /// `None` when its plugin is not loaded or its object id cannot exist in
    /// that plugin's slot kind.
    pub fn global_form_id(&self, form: FormRef) -> Option<u32> {
        let plugin = PluginId(u128::from_be_bytes(form.source()));
        let (slot, _) = self.owners.iter().find(|(_, owner)| *owner == plugin)?;
        let local = form.local();
        (local != 0 && local <= object_id_mask(*slot)).then(|| slot.compose(local))
    }
}

/// The object-id bits a slot kind leaves below its load-order bits.
fn object_id_mask(slot: GlobalSlot) -> u32 {
    match slot {
        GlobalSlot::Regular(_) => 0x00FF_FFFF,
        GlobalSlot::Light(_) => 0x0000_0FFF,
        // #4639 — Starfield medium masters keep a 16-bit object id.
        GlobalSlot::Medium(_) => 0x0000_FFFF,
    }
}

/// Inverse of [`GlobalSlot::compose`]: which slot owns this global FormID.
/// `0xFE` is the light-master space (12-bit sub-index below the top byte),
/// `0xFD` the Starfield medium-master space (8-bit sub-index, #4639); anything
/// else is a full-byte regular slot.
pub fn global_slot_of(form_id: u32) -> GlobalSlot {
    const LIGHT_MASTER_BYTE: u32 = 0xFE;
    const MEDIUM_MASTER_BYTE: u32 = 0xFD;
    if (form_id >> 24) == LIGHT_MASTER_BYTE {
        GlobalSlot::Light(((form_id >> 12) & 0x0FFF) as u16)
    } else if (form_id >> 24) == MEDIUM_MASTER_BYTE {
        GlobalSlot::Medium(((form_id >> 16) & 0x00FF) as u16)
    } else {
        GlobalSlot::Regular((form_id >> 24) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_refs_round_trip_through_every_slot_kind() {
        let identity = LoadOrderIdentity::new(vec![
            (
                GlobalSlot::Regular(0),
                PluginId::from_filename("Skyrim.esm"),
            ),
            (
                GlobalSlot::Light(3),
                PluginId::from_filename("ccBGSSSE001.esl"),
            ),
            (GlobalSlot::Medium(1), PluginId::from_filename("Medium.esm")),
        ]);
        for form_id in [0x0001_3746, 0xFE00_3801, 0xFD01_2345] {
            let form = identity.form_ref(form_id).expect("owned slot");
            assert_eq!(identity.global_form_id(form), Some(form_id));
        }
        // A slot no plugin owns has no portable identity.
        assert_eq!(identity.form_ref(0x0500_0001), None);
        // An object id the slot kind cannot hold is rejected on the way back.
        let esl = PluginId::from_filename("ccBGSSSE001.esl").0.to_be_bytes();
        assert_eq!(identity.global_form_id(FormRef::new(esl, 0x1000)), None);
        assert_eq!(identity.global_form_id(FormRef::new(esl, 0)), None);
    }
}
