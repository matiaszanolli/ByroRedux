//! Equipment components and build_npc_equip_state, the spawn-time armor / weapon assembly.

use super::*;

/// One armor piece resolved against the ESM index, queued for mesh
/// dispatch. The borrow into `EsmIndex` keeps this lifetime-tied to
/// the spawn-function scope.
pub(super) struct ResolvedArmor<'a> {
    pub(super) form_id: u32,
    /// Form referenced by OTFT/CNTO before leveled-list expansion. This is
    /// identical to `form_id` for a base ARMO and for the race skin layer.
    pub(super) source_form_id: u32,
    pub(super) model_path: &'a str,
    /// Biped slots that another equipped item displaced from this mesh.
    /// Non-zero only for the low-priority `RACE.WNAM` skin layer; the spawn
    /// hook uses it to remove matching dismember partitions while preserving
    /// still-uncovered body regions from the same NIF.
    pub(super) hidden_biped_mask: u32,
    /// Inventory row this armor mesh resolves from, or None for intrinsic
    /// race skin (which is not an inventory item). Cross-checked
    /// against `EquipmentSlots.occupants` after the equip loop
    /// finishes (#2094 / SKY-D3-NEW-02) — an entry whose index no
    /// longer occupies any of the biped bits it was equipped into
    /// was displaced by a later overlapping entry (multi-pick LVLI,
    /// mod CNTO overlapping a default OTFT slot) and must not spawn
    /// a mesh alongside the winner.
    pub(super) inv_idx: Option<InventoryIndex>,
    pub(super) intrinsic_skin: bool,
    /// The ARMO's own authored `BOD2`/`BODT` biped mask, as handed to
    /// `EquipmentSlots::equip`. **Zero is meaningful**: such a record
    /// claims no biped region at all, so it never enters `occupants` and
    /// the #2094 occupancy filter has no opinion about it — see the
    /// retain at the end of [`build_npc_equip_state`] and #3408.
    pub(super) authored_biped_mask: u32,
}

/// Spawn-derived ownership of an armor NIF root. Multiple ARMA meshes may
/// belong to one inventory row. Race skin is a body layer, not removable gear.
/// Rebuilt on spawn, never serialized with process-local entity IDs.
/// #5049 - no inventory-row field: hide/reveal and ownership match on
/// `form_id`, and the row a form resolves to can change under a load or a
/// stack merge, so a cached index was dead state that could only mislead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NpcEquipmentPart {
    pub actor: EntityId,
    pub form_id: u32,
    pub intrinsic_skin: bool,
    /// Partitions removed at import time; restoring them requires a rebuild.
    pub hidden_biped_mask: u32,
}

impl byroredux_core::ecs::Component for NpcEquipmentPart {
    type Storage = byroredux_core::ecs::SparseSetStorage<Self>;
}

/// P3 mid-life gear import — the spawn job's skeleton bone-name → entity
/// map, retained on the actor so a mid-life equipment import can bind a
/// newly imported armor NIF's skinned meshes to the living actor's bones.
/// The corpse-restoration path retains the same map in
/// `NpcLootAppearance::skeleton`, which living actors never grow. Spawn-
/// derived, re-derived identically every load — never serialized (same
/// posture as [`crate::components::AnimationTarget`]).
#[derive(Default)]
pub(crate) struct NpcSkeletonBones(pub(crate) HashMap<Arc<str>, EntityId>);

impl byroredux_core::ecs::Component for NpcSkeletonBones {
    type Storage = byroredux_core::ecs::SparseSetStorage<Self>;
}

/// #5095 — a head mesh root a spawn loaded: the pre-baked path's head-part
/// fallback (`PrebakedPhase::HeadParts`, the head a facegeom-less NPC gets
/// instead of none) and the runtime path's race head (`RuntimePhase::Head`,
/// the Oblivion / FO3 / FNV head NIF the FaceGen morphs shape). Carried so
/// `player.body` can report `head_parts=N` / `face_parts=N` and the P3 smoke
/// can gate that the third-person player has a face (#5408), not just a hair
/// shell — on either path. Spawn-derived, never serialized.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct HeadPartRoot {
    /// The part carries the face: the `Face` head-part type (`HDPT.PNAM` 1)
    /// on the pre-baked path, the race head itself on the runtime path.
    pub(crate) is_face: bool,
}

impl byroredux_core::ecs::Component for HeadPartRoot {
    type Storage = byroredux_core::ecs::SparseSetStorage<Self>;
}

/// P3 mid-life gear import — the gender + race the spawn path resolved for
/// this actor's body/gear mesh selection, retained so an equip of a
/// never-worn item resolves the same gendered/raced armor meshes the spawn
/// path would have. Spawn-derived, never serialized.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ActorBodyClass {
    pub(crate) gender: Gender,
    pub(crate) race_form_id: u32,
}

impl byroredux_core::ecs::Component for ActorBodyClass {
    type Storage = byroredux_core::ecs::SparseSetStorage<Self>;
}

/// P3 mid-life gear import — one equipped item awaiting its worn-mesh NIF
/// import. `loot_appearance::equipment_appearance_system` queues it when an
/// equip names a form id with no spawn-time root; `GearImportLoader` drains
/// it at one NIF per frame. Runtime handoff scratch, never serialized.
#[derive(Debug, Clone)]
pub(crate) struct PendingGearImport {
    pub(crate) form_id: u32,
    pub(crate) paths: Vec<String>,
}

impl byroredux_core::ecs::Component for PendingGearImport {
    type Storage = byroredux_core::ecs::SparseSetStorage<Self>;
}

/// #5028 — worn gear whose item left the inventory entirely, awaiting root
/// release. Queued by `loot_appearance::equipment_appearance_system` from
/// `ItemEventBatch` leaves-transfers for wearers with **no** `CellRoot`
/// (the player: their gear is never stamped into a cell range, so cell
/// teardown cannot release it — NPC mid-life imports stay cell-owned and
/// ride the normal range release). `GearImportLoader` drains it through
/// `cell_loader::unload::release_entities`, the same despawn + GPU-handle
/// release path cell teardown uses. Runtime handoff scratch, never
/// serialized.
#[derive(Debug, Clone, Default)]
pub(crate) struct PendingGearRelease {
    pub(crate) form_ids: Vec<u32>,
}

impl byroredux_core::ecs::Component for PendingGearRelease {
    type Storage = byroredux_core::ecs::SparseSetStorage<Self>;
}

/// Equip pipeline state built purely from `&NpcRecord` + `&EsmIndex`
/// — no World, no VulkanContext, no archive I/O. Both spawn paths
/// insert `inventory` + `equipment_slots` on the placement root
/// **before** skeleton / FaceGen load so the equip data lands even
/// when the spawn function early-returns on a missing archive. The
/// `armor_to_spawn` list is consumed after the skeleton resolves;
/// when the skeleton load early-returns, the meshes simply don't
/// spawn but the components are already in place for inspection +
/// the eventual save round-trip (M45).
pub(super) struct NpcEquipState<'a> {
    pub(super) inventory: Inventory,
    pub(super) equipment_slots: EquipmentSlots,
    pub(super) equipped_weapon: Option<EquippedWeapon>,
    pub(super) armor_to_spawn: Vec<ResolvedArmor<'a>>,
    /// Unmasked skin sources needed when worn gear is removed, including
    /// skins whose entire mesh is suppressed by the initial outfit.
    pub(super) restore_skin_paths: Vec<&'a str>,
    /// Biped bits the pre-baked FaceGen head must suppress, in the same
    /// `hide_skin_partitions` format as [`ResolvedArmor::hidden_biped_mask`].
    ///
    /// The FaceGeom NIF is a multi-region mesh source exactly like the race
    /// skin — vanilla Skyrim heads carry dismember partitions 130 (head +
    /// beard), 131 / 141 (hair), 143 (ears) and 132 (neck) — but it is never
    /// enrolled in `EquipmentSlots`, so nothing was ever computing its
    /// displacement. See #3409 and the fold that builds this at the end of
    /// [`build_npc_equip_state`].
    pub(super) facegen_hidden_mask: u32,
}

impl NpcEquipState<'_> {
    pub(super) fn covers_biped_mask(&self, mask: u32) -> bool {
        self.equipment_slots
            .occupants
            .iter()
            .enumerate()
            .any(|(bit, occupant)| mask & (1u32 << bit) != 0 && occupant.is_some())
    }

    /// Whether the winning equipment layer occupies this game's canonical
    /// main-body slot. Runtime-FaceGen games use this to suppress their loose
    /// naked upper-body mesh; prebaked-FaceGen games get the equivalent result
    /// by displacing the lower-priority `RACE.WNAM` skin armor.
    pub(super) fn main_body_covered(&self, game: GameKind) -> bool {
        byroredux_plugin::equip::main_body_bit(game)
            .is_some_and(|bit| self.covers_biped_mask(1u32 << bit))
    }
}

/// Walk the NPC's default outfit + inventory, expand LVLI refs to
/// base ARMO records, populate `Inventory` + `EquipmentSlots`, and
/// collect the armor mesh paths the spawn-side mesh loader will
/// dispatch. Independent of World / VulkanContext so the caller can
/// insert the components ahead of any archive I/O — that way a
/// missing skeleton.nif (e.g. a modded path the resolver can't
/// match, or a future game whose humanoid-skeleton convention isn't
/// yet in `humanoid_skeleton_path`) still leaves the equip data
/// inspectable on the placement root.
///
/// Race comes from the resolved record's "Use Traits" terminal (#4457 —
/// formerly the caller's responsibility, #4092 / D5-01): the same source
/// `stamp_character_components`' `Background` uses, so the race default
/// skin and every `resolve_armor_meshes` race match agree with the rest
/// of the actor's resolved traits instead of the raw shell's. Inventory
/// comes from the resolved "Use Inventory" terminal.
pub(super) fn build_npc_equip_state<'a>(
    resolved: &ResolvedNpc<'a>,
    index: &'a EsmIndex,
    game: GameKind,
    gender: Gender,
) -> NpcEquipState<'a> {
    // #4812 — no read here touches the shell any more: race/traits, stats
    // and the outfit+CNTO all come from their own template terminals, and
    // identity fields were already `resolved`'s before #4457.
    let race_form_id = resolved.r#traits.race_form_id;
    struct ExpandedEquip {
        form_id: u32,
        source_form_id: u32,
        count: u32,
    }

    let mut inventory = Inventory::new();
    let mut equipment_slots = EquipmentSlots::new();
    let mut equipped_weapon: Option<EquippedWeapon> = None;
    let mut armor_to_spawn: Vec<ResolvedArmor<'a>> = Vec::new();
    let mut race_skin_mask: Option<u32> = None;
    // #2955 — same gate as `stamp_character_components`: a PC-level-multiplier
    // record's `level` is not a level, and `expand_leveled_form_id` filters
    // `entry.level <= actor_level` then takes the highest eligible tier, so the
    // raw multiplier made every entry eligible and always drew the top one.
    //
    // #4812 — the level is the Use **Stats** terminal's, not the shell's:
    // TPLT 0x02 covers level, so a templated actor whose own record sits at
    // level 1 while its terminal carries the real level was expanding its
    // leveled gear at the wrong tier. Same source `resolve_actor_spells`
    // already reads (equip.rs).
    let actor_level = effective_actor_level(resolved.stats);
    let mut expanded: Vec<ExpandedEquip> = Vec::new();
    let mut resolved_buf = Vec::new();
    // #4696 — the carry-list expander's (form_id, count) pairs.
    let mut loot_buf = Vec::new();

    // #2093 / SKY-D3-NEW-01 — race default skin (`RACE.WNAM`) is the
    // lowest-priority body layer, not a lootable inventory item. OTFT/CNTO
    // armor masks its covered regions after equipment resolution; no
    // synthetic stack or equipment occupant is needed. Without this layer,
    // an NPC whose OTFT/CNTO
    // doesn't cover a biped region has zero mesh source there — the
    // prebaked path's FaceGeom NIF is head-only (Bethesda FaceGen
    // convention), not "head and body in one mesh."
    //
    // #5359 — a per-NPC `NPC_.WNAM` skin (the CK Traits-tab "Skin"
    // field) overrides the race default as that intrinsic layer. It is
    // read off the Use-Traits terminal — the flag ("Use Traits",
    // `0x0001`) governs inheriting it, same as the race itself — so a
    // templated actor wears its terminal's skin, and 442 vanilla
    // Skyrim actors (Alduin, every Draugr variant skin, the Falmer
    // variants…) get their authored body instead of the race default.
    if let Some(race) = index.races.get(&race_form_id) {
        if let Some(skin_fid) = resolved.r#traits.worn_skin.or(race.default_skin) {
            if let Some(item) = index.items.get(&skin_fid) {
                if let ItemKind::Armor { biped_flags, .. } = item.kind {
                    race_skin_mask = Some(biped_flags);
                    // #3357 — the race skin is the multi-ARMA case: its
                    // BOD2 covers Head|Body|Hands|Feet and three separate
                    // addons (torso / hands / feet) serve any given race.
                    // One `ResolvedArmor` per mesh, all marked intrinsic,
                    // so every addon receives the same displacement mask.
                    for model_path in byroredux_plugin::equip::resolve_armor_meshes(
                        item,
                        gender,
                        race_form_id,
                        index,
                        game,
                    ) {
                        armor_to_spawn.push(ResolvedArmor {
                            form_id: skin_fid,
                            source_form_id: skin_fid,
                            model_path,
                            hidden_biped_mask: 0,
                            inv_idx: None,
                            intrinsic_skin: true,
                            authored_biped_mask: biped_flags,
                        });
                    }
                }
            }
        }
    }

    // Default outfit (OTFT.items) → expand each entry through the
    // LVLI dispatcher. Skyrim+ NPCs typically reference leveled
    // lists for outfit variety; the pre-fix loop skipped LVLI refs
    // silently. See M41 Phase 2 close-out / #896.
    //
    // #4812 — xEdit places DOFT in the Inventory group, so the outfit is
    // the "Use Inventory" terminal's (TPLT 0x100 covers "Inventory tab,
    // including all outfits"): a shell whose own record carries no DOFT —
    // or an LVLN-resolved shell that lost its chain — dressed from the
    // terminal record instead. `npc` (the shell) remains the reader of
    // identity fields only.
    if let Some(otft_fid) = resolved.inventory.default_outfit {
        if let Some(otft) = index.outfits.get(&otft_fid) {
            for &fid in &otft.items {
                resolved_buf.clear();
                byroredux_plugin::equip::expand_leveled_form_id(
                    fid,
                    actor_level,
                    index,
                    &mut resolved_buf,
                );
                expanded.extend(resolved_buf.iter().copied().map(|form_id| ExpandedEquip {
                    form_id,
                    source_form_id: fid,
                    count: 1,
                }));
            }
        }
    }

    // CNTO inventory entries, from the resolved "Use Inventory" terminal
    // (#4457; the TPLT walk itself is `ResolvedNpc::resolve`'s). #1658 —
    // the helper returns the NPC's own inventory when no template
    // applies, or walks `template_form_id` (NPC_ or LVLN) when
    // `template_flags & TEMPLATE_FLAG_USE_INVENTORY` is set. Without it,
    // templated Skyrim NPCs with an empty own CNTO (leveled actors that
    // inherit gear via TPLT) spawned naked. Negative counts are
    // remove-from-inventory deltas; clamp at runtime.
    //
    // #4696 — carry lists expand through `expand_leveled_loot`, the same
    // pair-producing walk the container path uses, not the flat
    // `expand_leveled_form_id`: the flat walk assigned every leaf the
    // calling CNTO entry's own count, dropping the nested LVLO counts, so
    // a corpse held about a quarter of what the same list yields in a
    // container (FNV WithAmmo10mmPistolNPC: ×2 vs ×16 10mm). The worn-gear
    // outfit loop above keeps the flat walk — a count doesn't apply to a
    // piece of clothing.
    for entry in &resolved.inventory.inventory {
        let count = entry.count.max(0) as u32;
        if count == 0 {
            continue;
        }
        resolved_buf.clear();
        byroredux_plugin::equip::expand_leveled_loot(
            entry.item_form_id,
            count,
            actor_level.max(0) as u16,
            index,
            &mut loot_buf,
        );
        expanded.extend(loot_buf.drain(..).map(|(form_id, count)| ExpandedEquip {
            form_id,
            source_form_id: entry.item_form_id,
            count,
        }));
    }

    for expanded in expanded {
        let form_id = expanded.form_id;
        let stack = ItemStack::new(form_id, expanded.count);
        let inv_idx = inventory.push(stack);

        let Some(item) = index.items.get(&form_id) else {
            // LVLI dispatcher already flattened to base records;
            // anything still unresolved here is a master / DLC
            // master-list miss. Silent — the inventory row stays.
            continue;
        };
        let biped_flags = match &item.kind {
            ItemKind::Weapon {
                damage,
                reach,
                speed,
                ..
            } => {
                let candidate = EquippedWeapon {
                    inventory_index: inv_idx,
                    base_form_id: form_id,
                    damage: *damage as f32,
                    reach: *reach,
                    speed: *speed,
                };
                let replace = equipped_weapon.is_none_or(|current| {
                    candidate.damage > current.damage
                        || (candidate.damage == current.damage
                            && candidate.base_form_id < current.base_form_id)
                });
                if replace {
                    equipped_weapon = Some(candidate);
                }
                continue;
            }
            ItemKind::Armor { biped_flags, .. } => *biped_flags,
            // Food, ammo, MISC, and other non-equipment inventory keeps its
            // row but has no live equip state in this slice.
            _ => continue,
        };

        equipment_slots.equip(biped_flags, inv_idx);

        // #3357 — 166 of 2,762 Skyrim ARMOs serve one race with more than
        // one ARMA; each contributes its own mesh.
        for model_path in
            byroredux_plugin::equip::resolve_armor_meshes(item, gender, race_form_id, index, game)
        {
            armor_to_spawn.push(ResolvedArmor {
                form_id,
                source_form_id: expanded.source_form_id,
                model_path,
                hidden_biped_mask: 0,
                inv_idx: Some(inv_idx),
                intrinsic_skin: false,
                authored_biped_mask: biped_flags,
            });
        }
    }

    // A race skin can cover several regions in one mesh. If later armor wins
    // only some of those bits, keep the skin mesh but tell the importer which
    // dismember partitions to suppress. Bits outside the skin ARMO's authored
    // mask are irrelevant even if some other item occupies them.
    if let Some(skin_biped_flags) = race_skin_mask {
        let displaced_mask =
            equipment_slots
                .occupants
                .iter()
                .enumerate()
                .fold(0u32, |mask, (bit, occupant)| {
                    let bit_mask = 1u32 << bit;
                    if skin_biped_flags & bit_mask != 0 && occupant.is_some() {
                        mask | bit_mask
                    } else {
                        mask
                    }
                });
        // #3357 — `filter`, not `find`: the skin contributes one
        // `ResolvedArmor` per ARMA mesh (torso / hands / feet), and every
        // one of them needs the displacement mask. With `find`, only the
        // first got it and the rest rendered through gear that should
        // have hidden them.
        for skin in armor_to_spawn
            .iter_mut()
            .filter(|armor| armor.intrinsic_skin)
        {
            skin.hidden_biped_mask = displaced_mask;
        }
    }

    // #2094 / SKY-D3-NEW-02 — drop any queued mesh whose inventory
    // index no longer occupies a biped bit. `equip()` above already
    // resolved slot-overlap precedence (later entry in the expanded
    // list wins any bit it shares with an earlier one, e.g. two
    // multi-pick LVLI entries or a mod CNTO overlapping a default
    // OTFT slot) — this pass makes the mesh set agree with that
    // resolution instead of spawning every candidate regardless of
    // whether it was displaced.
    //
    // #3408 / SKY-2026-08-27b-D3-01 — a record whose authored mask is ZERO is
    // exempt. `equip()` iterates the set bits of the mask, so a zero mask
    // sets none, so such an item can never appear in `occupants` and can
    // never satisfy the retain — its mesh was discarded unconditionally. That
    // is not a displacement; a skin that claims no biped region cannot be
    // displaced out of one, so this filter has no opinion about it.
    //
    // #5358 — the census this exemption was built on ("10 of 2,762
    // Skyrim.esm ARMOs author `BOD2 == 0`") was a misread: those 10 author
    // the pre-BOD2 `BODT` body template, which the parser dropped, so
    // their real slot claims (0x04 body on the six creature skins, 0x02
    // hair / 0x10 beard on the four Draugr parts) read as zero. With
    // `BODT` decoded, no vanilla Skyrim ARMO has a zero mask; the
    // exemption now guards only genuinely mask-less authored records
    // (mods), for which the no-opinion semantics above still hold.
    let restore_skin_paths = armor_to_spawn
        .iter()
        .filter(|armor| armor.intrinsic_skin && armor.hidden_biped_mask != 0)
        .map(|armor| armor.model_path)
        .collect();
    armor_to_spawn.retain(|armor| {
        armor.authored_biped_mask == 0
            || match armor.inv_idx {
                Some(idx) => equipment_slots.occupants.contains(&Some(idx)),
                None => armor.authored_biped_mask & !armor.hidden_biped_mask != 0,
            }
    });

    // #3409 / SKY-2026-08-27b-D3-02 — the pre-baked FaceGen head's own
    // displacement mask. The head is a multi-region mesh source (partitions
    // 130 head+beard, 131/141 hair, 143 ears, 132 neck) but is never enrolled
    // in `EquipmentSlots`, so every bit an *equipped item* holds is a bit
    // something else covers — the same question the `displaced_mask` fold
    // above answers for the race skin, with the whole biped range in scope
    // instead of the skin's authored mask.
    //
    // Keeping race skin out of equipment occupants is load-bearing:
    // `SkinNaked` authors bit 0 (Head), and 47 of Skyrim's 99 races point
    // `WNAM` at a skin that does. Folding those in would hide partition 130
    // on every one of them — i.e. delete the face of most humanoid NPCs.
    // Bit 0 only suppresses the face when actual armor claims it,
    // which is exactly what a closed helm does:
    //
    //   Dwarven / Daedric / Nord Plate / Guard "FullReach"  bits 0,1,12,13
    //     → hides 130 (face + beard), 131 (hair), 143 (ears); the helm ships
    //       its own partition-30 geometry (Dwarven: 1514 triangles) to
    //       replace it. 175 of Skyrim's 2,762 ARMOs are authored this way.
    //   Iron / Hide / Studded / Steel light helms          bits 1,12
    //     → hides 131 only; the face survives, which is the visible
    //       difference between an open and a closed helm.
    //   Circlets                                            bit 12
    //     → hides nothing on the head; a circlet displaces other circlets.
    //
    // Known residual: partition 141 (long hair) maps to bit 11 (slot 41),
    // which exactly ONE of Skyrim's 2,762 ARMOs claims, so long hair is never
    // displaced by this rule. Coupling 141 to bit 1 would be inventing a
    // mapping the data doesn't author — and it would be wrong for the
    // deliberate `HairLine*` sub-meshes, which Bethesda authors to show
    // *because* a helmet is worn. Left for a HDPT-type-aware follow-up.
    let facegen_hidden_mask =
        equipment_slots
            .occupants
            .iter()
            .enumerate()
            .fold(0u32, |mask, (bit, occupant)| match occupant {
                Some(_) => mask | (1u32 << bit),
                _ => mask,
            });

    NpcEquipState {
        inventory,
        equipment_slots,
        equipped_weapon,
        armor_to_spawn,
        restore_skin_paths,
        facegen_hidden_mask,
    }
}
