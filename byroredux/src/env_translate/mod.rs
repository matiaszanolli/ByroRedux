//! EXAL (Exterior Abstraction Layer) — the exterior **environment**
//! translation boundary.
//!
//! This module is to outdoors rendering what [`crate::material_translate`]
//! is to materials: the single home where per-game [`byroredux_plugin`]
//! ESM records (WRLD / CLMT / WTHR / LAND / WATR / CELL lighting) are
//! resolved into the engine's canonical, game-agnostic representation.
//! Everything downstream — the sky pass, the terrain pass, the water
//! pass, the sun directional light, the LOD ring — consumes the canonical
//! resources identically for every game, with no per-game branches and no
//! render-time fallbacks.
//!
//! Architecture + rollout: see `docs/engine/exal.md`. Per the canonical-
//! type rule it shares with NIFAL (`docs/engine/nifal.md`), the canonical
//! tier is the ECS resource/component that already serves the renderer-
//! facing role ([`WaterMaterial`], `SkyParamsRes`, `WeatherDataRes`,
//! `CellLightingRes`, …); this module is the `translate()` step, not a new
//! type.
//!
//! The rollout slices are complete: every scattered sky / sun / weather /
//! water / LOD producer has been folded in through `docs/engine/exal.md`'s
//! §7 plan (#5731 — this header used to describe Step 1 as current).
//!
//! ## Layout (#5584)
//!
//! One submodule per independent translation concern, re-exported here so
//! every caller keeps the `crate::env_translate::…` path:
//!
//! - [`lod`] — distant terrain / object LOD + worldspace-default and LOD water;
//! - [`climate`] — worldspace / climate / default-weather inheritance;
//! - [`water`] — `WATR` → [`WaterMaterial`];
//! - [`sky_weather`] — sky, sun, cell lighting, weather, image-space sources;
//! - [`fallback`] — the procedural no-data defaults.
//!
//! The `PNAM` parent-use bits and the [`inherit_up_chain`] walk they drive
//! are shared by the LOD, climate and image-space halves, so they live here.

use std::collections::HashMap;

use byroredux_plugin::esm::cell::WorldspaceRecord;

mod climate;
mod fallback;
mod lod;
mod sky_weather;
mod tod;
mod water;

pub(crate) use climate::*;
pub(crate) use fallback::*;
pub(crate) use lod::*;
pub(crate) use sky_weather::*;
pub(crate) use tod::*;
pub(crate) use water::*;

/// `PNAM` parent-use flags — which fields a child worldspace takes from its
/// [`WNAM`](WorldspaceRecord::parent_worldspace) parent.
///
/// # How these were identified
///
/// The bit meanings are not documented in the record; they were established by
/// correlating each bit against the *absence* of the sub-record it would
/// govern, across every child worldspace in the shipped masters (#2735):
///
/// | Bit | Sub-record | Skyrim | FO3 | FO4 | FNV |
/// |---|---|---|---|---|---|
/// | `0x01` | `DNAM` land data | 7/7 | 4/4 | 1/1 | — |
/// | `0x02` | `NAM3`+`NAM4` LOD water | 7/7 | 4/4 | 3/3 | — |
/// | `0x04` | `ICON` map | 34/34 | 23/23 | 3/3 | 10/10 |
/// | `0x08` | `NAM2` water | 6/6 | 1/1 | 1/1 | — |
/// | `0x10` | `CNAM` climate | 20 set / 23 absent | 1/1 | 3/3 | — |
///
/// Every ratio is "bit set" over "own sub-record absent", and they match
/// exactly in every game that uses the bit. The invariant the data supports is
/// therefore **bit set ⟺ no own value, take the parent's**.
///
/// Climate is the one inexact row and it confirms rather than undermines the
/// reading: three Skyrim children lack `CNAM` *without* setting the bit, i.e.
/// they are genuinely climate-less rather than inheriting — which is precisely
/// the distinction [`resolve_worldspace_climate`] already draws.
mod pnam {
    /// `DNAM` — land data, which carries the default water *height*.
    pub(super) const INHERIT_LAND: u16 = 0x01;
    /// The GECK's whole-LOD inheritance ("Use LOD Data"): the distant
    /// terrain and object quads — and the `NAM3`/`NAM4` LOD water type
    /// and height, which are sub-fields of that checkbox's group — come
    /// from the `WNAM` parent. #2735 wired the water half;
    /// `lod_source_worldspace` (#5496) keys the rings themselves on the
    /// same bit.
    pub(super) const INHERIT_LOD: u16 = 0x02;
    /// `ICON` — worldspace map texture. Parsed but currently unconsumed, so
    /// deliberately not wired; add a resolver alongside its first consumer.
    #[allow(dead_code)]
    pub(super) const INHERIT_MAP: u16 = 0x04;
    /// `NAM2` — default water *type*.
    pub(super) const INHERIT_WATER: u16 = 0x08;
    /// `CNAM` — climate.
    pub(super) const INHERIT_CLIMATE: u16 = 0x10;
    /// `INAM` — image space (FO3/FNV). Unlike bits 0-4 this one is not in
    /// the #2735 correlation table: its meaning is xEdit's name for the
    /// bit ("Use Image Space Data", `wbDefinitionsFNV.pas` WRLD PNAM).
    pub(super) const INHERIT_IMAGE_SPACE: u16 = 0x20;
}

/// Resolve one inheritable worldspace field, walking the `WNAM` chain.
///
/// A child's own authored value always wins; the chain is consulted only when
/// the field is absent **and** `bit` is set. That ordering is what the
/// correlation above establishes, and it means a worldspace that authors a
/// value can never have it overridden by an ancestor.
///
/// Cycle-guarded: corrupt or adversarial plugin data terminates the walk
/// instead of hanging.
///
/// `extract` receives the current worldspace key alongside its record
/// (#2814). Most inheritable fields live on [`WorldspaceRecord`] and ignore
/// the key, but climate is held in a side-map keyed the same way, and giving
/// it the key is what lets [`resolve_worldspace_climate`] use this walk
/// instead of carrying a second copy of the cycle guard, the linear
/// form_id reverse lookup, the three `warn!` termination cases, and the
/// precedence ordering.
fn inherit_up_chain<T, F>(
    worldspaces: &HashMap<String, WorldspaceRecord>,
    start_key: &str,
    bit: u16,
    extract: F,
) -> Option<T>
where
    F: Fn(&str, &WorldspaceRecord) -> Option<T>,
{
    let mut current = start_key.to_string();
    let mut visited = std::collections::HashSet::new();
    loop {
        let record = worldspaces.get(&current)?;
        if let Some(value) = extract(&current, record) {
            return Some(value);
        }
        if record.parent_flags & bit == 0 {
            // No own value and not flagged to inherit — genuinely unauthored,
            // not an inheritance gap.
            return None;
        }
        if !visited.insert(current.clone()) {
            log::warn!(
                "inherit_up_chain: cyclic WNAM chain from '{start_key}' (revisited \
                 '{current}') while resolving PNAM bit {bit:#06X} — treating as unresolved",
            );
            return None;
        }
        let parent_fid = record.parent_worldspace.or_else(|| {
            log::warn!(
                "inherit_up_chain: '{current}' sets PNAM bit {bit:#06X} but authors no WNAM \
                 parent — chain terminates unresolved (from '{start_key}')",
            );
            None
        })?;
        let (parent_key, _) = worldspaces
            .iter()
            .find(|(_, w)| w.form_id == parent_fid)
            .or_else(|| {
                log::warn!(
                    "inherit_up_chain: '{current}'s WNAM parent {parent_fid:08X} is not among \
                     parsed worldspaces — chain terminates unresolved (from '{start_key}')",
                );
                None
            })?;
        current = parent_key.clone();
    }
}

/// Regression tests for [`climate_tod_hours`] — #530 / FNV-CELL-8. Lives
/// beside the function under the EXAL boundary (#2453).
#[cfg(test)]
mod climate_tod_hours_tests;

/// Fixtures shared by the LOD-water and image-space PNAM inheritance tests.
#[cfg(test)]
mod test_support {
    use super::*;

    /// Two-worldspace fixture: child `"c"` under parent `"p"`, with the
    /// child's PNAM bits set to `child_flags`.
    pub(super) fn parent_child(
        parent: WorldspaceRecord,
        child_flags: u16,
        child: WorldspaceRecord,
    ) -> HashMap<String, WorldspaceRecord> {
        let parent_fid = 0x0000_0100;
        HashMap::from([
            (
                "p".to_string(),
                WorldspaceRecord {
                    form_id: parent_fid,
                    ..parent
                },
            ),
            (
                "c".to_string(),
                WorldspaceRecord {
                    form_id: 0x0000_0200,
                    parent_worldspace: Some(parent_fid),
                    parent_flags: child_flags,
                    ..child
                },
            ),
        ])
    }
}
