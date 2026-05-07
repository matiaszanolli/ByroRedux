//! Per-REFR placement expansion + texture overrides.
//!
//! Two related concerns share this module because they both transform a
//! single placed reference into the inputs `spawn_placed_instances`
//! consumes downstream:
//!
//! * **Placement expansion** — PKIN (Pack-In) and SCOL (Static Collection)
//!   REFRs fan out into multiple synthetic placements at composed
//!   transforms. A normal REFR returns a single-entry vec; PKIN/SCOL
//!   return one entry per content reference. See #585 (SCOL) and #589
//!   (PKIN / FO4-DIM4-03).
//!
//! * **Texture overrides** — XATO, XTNM, and XTXR sub-records on a REFR
//!   shadow specific texture slots of its base mesh.
//!   `RefrTextureOverlay` carries the resolved per-slot paths through to
//!   spawn time, where they take precedence over the cached `ImportedMesh`
//!   reads without mutating the cache. See #584 (FO4-DIM6-02).
//!
//! Test coverage lives in three sibling files
//! (`cell_loader_pkin_expansion_tests`, `cell_loader_scol_expansion_tests`,
//! `cell_loader_refr_texture_overlay_tests`); each picks up these items
//! through the `pub(crate) use` re-exports in `cell_loader`.

use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::{FixedString, StringPool};
use byroredux_plugin::esm;

use crate::asset_provider::MaterialProvider;

use super::euler_zup_to_quat_yup_refr;

/// Per-REFR texture overlay computed from XATO / XTNM / XTXR sub-records
/// (#584). A populated overlay overrides specific texture slots of the
/// REFR's base mesh; precedence is:
///
/// 1. XATO full-TXST overlay (and XTNM for LAND-scoped refs) fills any
///    slots the referenced `TextureSet` carries.
/// 2. XTXR per-slot swaps override individual slots — later XTXR entries
///    win for the same slot.
/// 3. If the overlay picked up a `material_path` (MNAM-only TXSTs), the
///    BGSM chain fills any still-empty slot.
///
/// Applied at spawn time inside `spawn_placed_instances`, shadowing the
/// cached `ImportedMesh` reads. The original `ImportedMesh` is never
/// mutated — the overlay is a per-REFR shadow that respects the
/// process-lifetime NIF import cache.
///
/// Pre-#584, 37 % of vanilla Fallout4.esm TXSTs (140 / 382, MNAM-only)
/// parsed cleanly into `EsmCellIndex.texture_sets` with nowhere to go.
#[derive(Debug, Default, Clone)]
pub(crate) struct RefrTextureOverlay {
    /// Texture-slot paths interned through the engine [`StringPool`]
    /// (#609 / D6-NEW-01) so REFR overlays share the same dedup table
    /// as `ImportedMesh` and avoid per-overlay heap allocations.
    pub(crate) diffuse: Option<FixedString>,
    pub(crate) normal: Option<FixedString>,
    pub(crate) glow: Option<FixedString>,
    pub(crate) height: Option<FixedString>,
    pub(crate) env: Option<FixedString>,
    pub(crate) env_mask: Option<FixedString>,
    /// BSShaderTextureSet slot 6 — MultiLayerParallax inner layer.
    /// Not yet consumed by the spawn path; preserved for parity with
    /// `TextureSet.inner` so the slot_index=6 XTXR swap round-trips.
    #[allow(dead_code)]
    pub(crate) inner: Option<FixedString>,
    pub(crate) specular: Option<FixedString>,
    pub(crate) material_path: Option<FixedString>,
}

impl RefrTextureOverlay {
    /// First-non-empty-wins fill for an overlay slot. Routes through
    /// the engine [`StringPool`] so the resolved path lives in one
    /// dedup table shared with `ImportedMesh`. See #609.
    fn fill(slot: &mut Option<FixedString>, value: Option<&str>, pool: &mut StringPool) {
        if slot.is_none() {
            if let Some(v) = value {
                if !v.is_empty() {
                    *slot = Some(pool.intern(v));
                }
            }
        }
    }

    fn merge_from_texture_set(&mut self, ts: &esm::cell::TextureSet, pool: &mut StringPool) {
        Self::fill(&mut self.diffuse, ts.diffuse.as_deref(), pool);
        Self::fill(&mut self.normal, ts.normal.as_deref(), pool);
        Self::fill(&mut self.glow, ts.glow.as_deref(), pool);
        Self::fill(&mut self.height, ts.height.as_deref(), pool);
        Self::fill(&mut self.env, ts.env.as_deref(), pool);
        Self::fill(&mut self.env_mask, ts.env_mask.as_deref(), pool);
        Self::fill(&mut self.inner, ts.inner.as_deref(), pool);
        Self::fill(&mut self.specular, ts.specular.as_deref(), pool);
        Self::fill(&mut self.material_path, ts.material_path.as_deref(), pool);
    }

    /// Apply a single XTXR slot swap. `slot_index` picks one of TX00..TX07
    /// on the host mesh; the source path comes from the swap TXST's
    /// same-index slot. Later XTXR for the same slot overwrites.
    fn apply_slot_swap(
        &mut self,
        ts: &esm::cell::TextureSet,
        slot_index: u32,
        pool: &mut StringPool,
    ) {
        let src = match slot_index {
            0 => ts.diffuse.as_deref(),
            1 => ts.normal.as_deref(),
            2 => ts.glow.as_deref(),
            3 => ts.height.as_deref(),
            4 => ts.env.as_deref(),
            5 => ts.env_mask.as_deref(),
            6 => ts.inner.as_deref(),
            7 => ts.specular.as_deref(),
            _ => return,
        };
        let Some(path) = src else { return };
        if path.is_empty() {
            return;
        }
        let dest = match slot_index {
            0 => &mut self.diffuse,
            1 => &mut self.normal,
            2 => &mut self.glow,
            3 => &mut self.height,
            4 => &mut self.env,
            5 => &mut self.env_mask,
            6 => &mut self.inner,
            7 => &mut self.specular,
            _ => return,
        };
        *dest = Some(pool.intern(path));
    }

    /// Walk the overlay's `material_path` BGSM/BGEM chain and fill any
    /// still-empty texture slot. Matches `merge_bgsm_into_mesh`'s
    /// first-wins policy so REFR overlays and per-mesh imports agree on
    /// precedence for MNAM-only TXSTs. No-op when the path isn't a
    /// `.bgsm` / `.bgem` or the provider can't resolve it.
    fn fill_from_bgsm(&mut self, provider: &mut MaterialProvider, pool: &mut StringPool) {
        let Some(path_sym) = self.material_path else {
            return;
        };
        // `pool.resolve` returns the canonical lowercased form, so this
        // doubles as the suffix-dispatch check without an extra
        // `to_ascii_lowercase` allocation.
        let path: String = match pool.resolve(path_sym) {
            Some(s) => s.to_string(),
            None => return,
        };
        if path.ends_with(".bgsm") {
            let Some(resolved) = provider.resolve_bgsm(&path) else {
                return;
            };
            for step in resolved.walk() {
                let f = &step.file;
                Self::fill(&mut self.diffuse, Some(f.diffuse_texture.as_str()), pool);
                Self::fill(&mut self.normal, Some(f.normal_texture.as_str()), pool);
                Self::fill(&mut self.glow, Some(f.glow_texture.as_str()), pool);
                Self::fill(&mut self.specular, Some(f.smooth_spec_texture.as_str()), pool);
                Self::fill(&mut self.env, Some(f.envmap_texture.as_str()), pool);
                Self::fill(&mut self.height, Some(f.displacement_texture.as_str()), pool);
            }
        } else if path.ends_with(".bgem") {
            let Some(bgem) = provider.resolve_bgem(&path) else {
                return;
            };
            Self::fill(&mut self.normal, Some(bgem.normal_texture.as_str()), pool);
            Self::fill(&mut self.glow, Some(bgem.glow_texture.as_str()), pool);
            Self::fill(&mut self.env, Some(bgem.envmap_texture.as_str()), pool);
        }
    }
}

/// Build a texture overlay for a REFR when its parser-side override
/// sub-records (XATO, XTNM, XTXR) carry actionable TXST FormIDs. Returns
/// `None` when the REFR has no overrides — the hot path for interior
/// cells where > 99 % of REFRs use their base mesh's textures verbatim.
pub(crate) fn build_refr_texture_overlay(
    placed: &esm::cell::PlacedRef,
    index: &esm::cell::EsmCellIndex,
    mat_provider: Option<&mut MaterialProvider>,
    pool: &mut StringPool,
) -> Option<RefrTextureOverlay> {
    if placed.alt_texture_ref.is_none()
        && placed.land_texture_ref.is_none()
        && placed.texture_slot_swaps.is_empty()
    {
        return None;
    }

    let mut ov = RefrTextureOverlay::default();

    // XATO — mesh-scoped TXST override.
    if let Some(txst_ref) = placed.alt_texture_ref {
        if let Some(ts) = index.texture_sets.get(&txst_ref) {
            ov.merge_from_texture_set(ts, pool);
        }
    }
    // XTNM — LAND-scoped override; same wire layout as XATO. Fills slots
    // XATO didn't cover. Typical REFRs carry only one of the two.
    if let Some(txst_ref) = placed.land_texture_ref {
        if let Some(ts) = index.texture_sets.get(&txst_ref) {
            ov.merge_from_texture_set(ts, pool);
        }
    }

    // XTXR — per-slot swaps applied after the full-TXST overlay so
    // individual slot swaps can override what XATO/XTNM set. Later XTXR
    // for the same slot wins (authoring-order semantics).
    for swap in &placed.texture_slot_swaps {
        if let Some(ts) = index.texture_sets.get(&swap.texture_set) {
            ov.apply_slot_swap(ts, swap.slot_index, pool);
        }
    }

    // BGSM chain fill — MNAM-only TXSTs contribute nothing to the 8
    // direct slots, but their `material_path` resolves through the BGSM
    // template chain to real textures. Matches import-time
    // `merge_bgsm_into_mesh` semantics.
    if ov.material_path.is_some() {
        if let Some(mp) = mat_provider {
            ov.fill_from_bgsm(mp, pool);
        }
    }

    Some(ov)
}

/// Maximum PKIN-of-PKIN recursion depth (#635 / FNV-D3-06). Vanilla FO4
/// has zero PKIN-of-PKIN nesting; mods occasionally chain a few levels
/// deep but never (sanely) more than a handful. The cap exists as a
/// guard against author-error cycles, not a normal-case constraint.
const MAX_PKIN_DEPTH: u32 = 4;

/// Expand a PKIN (Pack-In) REFR into synthetic children.
///
/// PKIN records (FO4+) bundle LVLI / CONT / STAT / MSTT / FURN references
/// behind a single form ID so a level designer can drop a reusable
/// "generic workbench with loot" as one REFR. The parser captures every
/// `CNAM` sub-record into `PkinRecord::contents` at ESM-load time; this
/// helper fans the REFR out into one synthetic placement per content
/// entry — all at the SAME outer transform (PKIN carries no per-child
/// placement data, unlike SCOL).
///
/// PKIN-of-PKIN nesting is resolved recursively up to [`MAX_PKIN_DEPTH`]
/// levels (#635 / FNV-D3-06) so a child PKIN's contents fan out instead
/// of being silently dropped at the caller's `index.statics.get` lookup.
/// Children that resolve to a non-PKIN form pass through unchanged.
/// Children that resolve to a SCOL or LVLI stay single-level — those
/// expansions live in `expand_scol_placements` (#585) and an unimplemented
/// LVLI helper (#386).
///
/// Returns `None` when the outer REFR's base isn't a PKIN, or when the
/// PKIN's `contents` list is empty.
///
/// Pre-#589 all 872 vanilla Fallout4.esm PKIN records silently produced
/// no world content because the MODL-only parser discarded the CNAM
/// list. See audit FO4-DIM4-03.
pub(crate) fn expand_pkin_placements(
    base_form_id: u32,
    outer_pos: Vec3,
    outer_rot: Quat,
    outer_scale: f32,
    index: &esm::cell::EsmCellIndex,
) -> Option<Vec<(u32, Vec3, Quat, f32)>> {
    expand_pkin_placements_with_depth(base_form_id, outer_pos, outer_rot, outer_scale, index, 0)
}

fn expand_pkin_placements_with_depth(
    base_form_id: u32,
    outer_pos: Vec3,
    outer_rot: Quat,
    outer_scale: f32,
    index: &esm::cell::EsmCellIndex,
    depth: u32,
) -> Option<Vec<(u32, Vec3, Quat, f32)>> {
    let pkin = index.packins.get(&base_form_id)?;
    if pkin.contents.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(pkin.contents.len());
    for &child_form_id in &pkin.contents {
        // Recurse into nested PKINs up to the depth cap. Past the cap,
        // fall through to the leaf path so the synthetic placement at
        // least gets logged via `stat_miss` accounting (matches pre-#635
        // truncation but bounded — safe against accidental cycles).
        if depth + 1 < MAX_PKIN_DEPTH && index.packins.contains_key(&child_form_id) {
            if let Some(nested) = expand_pkin_placements_with_depth(
                child_form_id,
                outer_pos,
                outer_rot,
                outer_scale,
                index,
                depth + 1,
            ) {
                out.extend(nested);
                continue;
            }
        }
        out.push((child_form_id, outer_pos, outer_rot, outer_scale));
    }
    Some(out)
}

/// Produce the list of `(base_form_id, composed_pos, composed_rot,
/// composed_scale)` placements to spawn for one REFR.
///
/// Normal (non-SCOL) REFR: returns a single-entry vec with the outer
/// REFR's own base form ID + world-space transform — the hot path for
/// interior cells (~99 % of REFRs).
///
/// SCOL REFR with no cached `CM*.NIF`: flattens `ScolRecord.parts` into
/// synthetic children. Each `ScolPlacement` (Z-up Euler-radian local
/// transform per `records/scol.rs`) composes with the outer REFR's
/// world-space transform:
///
/// ```text
/// final_pos    = outer_rot * (outer_scale * local_pos) + outer_pos
/// final_rot    = outer_rot * local_rot
/// final_scale  = outer_scale * local_scale
/// ```
///
/// Vanilla FO4 ships 2616 / 2617 SCOLs with a cached `CM*.NIF` in
/// `statics[base].model_path`, so the normal path runs for those.
/// Mod-added SCOLs (and vanilla SCOLs whose CM file is absent under a
/// previsibine-bypass loadout) hit the expansion branch. Single-level
/// only — vanilla FO4 has no SCOL-of-SCOL nesting. See #585.
pub(crate) fn expand_scol_placements(
    base_form_id: u32,
    outer_pos: Vec3,
    outer_rot: Quat,
    outer_scale: f32,
    index: &esm::cell::EsmCellIndex,
) -> Vec<(u32, Vec3, Quat, f32)> {
    // Expand only when the outer REFR's base is a SCOL with no valid
    // cached model. `statics.get(base).model_path` empty — or the base
    // isn't in `statics` at all (mod-added SCOL without EDID/MODL) —
    // plus the base form IS in `scols`.
    let must_expand = index.scols.contains_key(&base_form_id)
        && index
            .statics
            .get(&base_form_id)
            .map_or(true, |s| s.model_path.is_empty());
    if !must_expand {
        return vec![(base_form_id, outer_pos, outer_rot, outer_scale)];
    }

    let Some(scol) = index.scols.get(&base_form_id) else {
        // Defensive: if contains_key passed but get returned None
        // (shouldn't happen outside concurrent mutation), fall back to
        // the non-expanded single-entry path so the REFR at least gets
        // logged as a stats miss rather than silently dropped.
        return vec![(base_form_id, outer_pos, outer_rot, outer_scale)];
    };

    let mut out = Vec::new();
    for part in &scol.parts {
        for p in &part.placements {
            // Z-up Bethesda → Y-up renderer, matching the outer REFR
            // conversion policy in `load_references`.
            let local_pos = Vec3::new(p.pos[0], p.pos[2], -p.pos[1]);
            let local_rot = euler_zup_to_quat_yup_refr(p.rot[0], p.rot[1], p.rot[2]);
            let final_pos = outer_rot * (outer_scale * local_pos) + outer_pos;
            let final_rot = outer_rot * local_rot;
            let final_scale = outer_scale * p.scale;
            out.push((part.base_form_id, final_pos, final_rot, final_scale));
        }
    }
    out
}
