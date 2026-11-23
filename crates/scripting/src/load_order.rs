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
use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId, ENGINE_PLUGIN_ID};
use byroredux_plugin::esm::reader::GlobalSlot;
use byroredux_sdk::identity::FormRef;

/// Slot → owning plugin for every plugin in the active load order.
#[derive(Clone, Debug, Default)]
pub struct LoadOrderIdentity {
    owners: Vec<(GlobalSlot, PluginId)>,
}

impl Resource for LoadOrderIdentity {}

/// The load order a save in progress converts its FormIDs through (LMC-SAV-002..004): written as the stable identity
/// this load order gives them, read back as the global FormID the current one does.
#[cfg(feature = "save")]
impl byroredux_core::stable_form_id::FormIdTranslator for LoadOrderIdentity {
    fn pair(&self, global: u32) -> Option<FormIdPair> {
        LoadOrderIdentity::pair(self, global)
    }

    fn global(&self, pair: FormIdPair) -> Option<u32> {
        self.global_form_id(FormRef::new(pair.plugin.0.to_be_bytes(), pair.local.0))
    }
}

impl LoadOrderIdentity {
    pub fn new(owners: Vec<(GlobalSlot, PluginId)>) -> Self {
        Self { owners }
    }

    pub fn owners(&self) -> &[(GlobalSlot, PluginId)] {
        &self.owners
    }

    /// Allocate the engine's Oblivion records after all real plugins.
    /// The regular loader reserves 0xFC for alias owners and higher bytes
    /// for other form spaces, so only 0x00..=0xFB can be used here.
    pub fn allocate_engine_slot(&mut self) -> Result<GlobalSlot, String> {
        self.allocate_engine_slot_avoiding(&[])
    }

    /// Also avoid slots occupied by preserved, unowned record IDs. Some
    /// Oblivion masters contain out-of-range source indices; until their
    /// provenance is resolved they must not acquire the engine's identity.
    pub fn allocate_engine_slot_avoiding(&mut self, occupied_records: &[GlobalSlot]) -> Result<GlobalSlot, String> {
        if self.owners.iter().any(|(_, id)| *id == ENGINE_PLUGIN_ID) {
            return Err(format!("loaded plugin conflicts with engine identity {ENGINE_PLUGIN_ID:?}"));
        }
        for (i, (slot, id)) in self.owners.iter().enumerate() {
            if self.owners[..i].iter().any(|(prior, _)| prior == slot) {
                return Err(format!("duplicate load-order slot {slot:?} while allocating engine records"));
            }
            if self.owners[..i].iter().any(|(_, prior)| prior == id) {
                return Err(format!("duplicate plugin identity {id:?} while allocating engine records"));
            }
        }
        let slot = (0..=0xfb).map(GlobalSlot::Regular)
            .find(|slot| !occupied_records.contains(slot) && !self.owners.iter().any(|(occupied, _)| occupied == slot))
            .ok_or_else(|| "cannot allocate engine records: all 252 regular slots are occupied".to_owned())?;
        self.owners.push((slot, ENGINE_PLUGIN_ID));
        Ok(slot)
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

    /// Papyrus `Game.GetFormFromFile(aiFormID, asFilename)`: the form
    /// `form_id` names in the plugin `plugin`. The CK page: "the top most
    /// byte in the given ID is unused" — the load-order bits of whatever slot
    /// kind the plugin has are ignored. `None` when the plugin is not loaded.
    pub fn form_from_file(&self, form_id: u32, plugin: PluginId) -> Option<u32> {
        let (slot, _) = self.owners.iter().find(|(_, owner)| *owner == plugin)?;
        let local = form_id & object_id_mask(*slot);
        (local != 0).then(|| slot.compose(local))
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

/// The plugin-local object id of a load-order-global FormID: the part the
/// owning plugin's file stores, with the slot bits stripped (24 bits for a
/// regular slot, 12 for a light master, 16 for a Starfield medium master).
/// Archive paths keyed by the authoring plugin (`facegeom\<plugin>\<local>`,
/// `scol\<plugin>\cm<local>`) name files by this value, never the global id.
pub fn plugin_local_object_id(form_id: u32) -> u32 {
    form_id & object_id_mask(global_slot_of(form_id))
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
    fn engine_slot_round_trips_all_oblivion_identities() {
        let mut identity = LoadOrderIdentity::new(vec![
            (GlobalSlot::Regular(0), PluginId::from_filename("Oblivion.esm")),
            (GlobalSlot::Regular(2), PluginId::from_filename("Example.esp")),
        ]);
        let slot = identity.allocate_engine_slot().unwrap();
        assert_eq!(slot, GlobalSlot::Regular(1));
        for index in 0..72 {
            let pair = FormIdPair { plugin: ENGINE_PLUGIN_ID, local: LocalFormId(index + 1) };
            let form = FormRef::new(pair.plugin.0.to_be_bytes(), pair.local.0);
            let key = identity.global_form_id(form).unwrap();
            assert_eq!(identity.pair(key), Some(pair));
        }
    }

    #[test]
    fn engine_slot_rejects_collisions_and_exhaustion() {
        let mut collision = LoadOrderIdentity::new(vec![(GlobalSlot::Regular(0), ENGINE_PLUGIN_ID)]);
        assert!(collision.allocate_engine_slot().unwrap_err().contains("engine identity"));
        let mut duplicate = LoadOrderIdentity::new(vec![
            (GlobalSlot::Regular(0), PluginId(1)),
            (GlobalSlot::Regular(0), PluginId(2)),
        ]);
        assert!(duplicate.allocate_engine_slot().unwrap_err().contains("duplicate load-order slot"));
        let mut full = LoadOrderIdentity::new((0..=0xfb).map(|slot| (GlobalSlot::Regular(slot), PluginId(u128::from(slot) + 1))).collect());
        let error = full.allocate_engine_slot().unwrap_err();
        assert!(error.contains("engine records") && error.contains("252"));
        assert_eq!(full.owners().len(), 252);
    }

    #[test]
    fn engine_slot_skips_unowned_record_slots_without_claiming_them() {
        let master = PluginId::from_filename("Oblivion.esm");
        let mut identity = LoadOrderIdentity::new(vec![(GlobalSlot::Regular(0), master)]);
        let slot = identity.allocate_engine_slot_avoiding(&[GlobalSlot::Regular(1)]).unwrap();
        assert_eq!(slot, GlobalSlot::Regular(2));
        assert!(identity.pair(0x0100_1234).is_none());
        assert_eq!(identity.pair(0x0200_0001).unwrap().plugin, ENGINE_PLUGIN_ID);
    }

    #[test]
    fn plugin_local_object_id_strips_the_slot_bits_per_slot_kind() {
        assert_eq!(plugin_local_object_id(0x0001_A670), 0x0001_A670);
        assert_eq!(plugin_local_object_id(0x0200_2B6C), 0x0000_2B6C);
        assert_eq!(plugin_local_object_id(0xFE00_08F6), 0x0000_08F6);
        assert_eq!(plugin_local_object_id(0xFD00_0E90), 0x0000_0E90);
        assert_eq!(plugin_local_object_id(0xFD12_0E90), 0x0000_0E90);
    }

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
