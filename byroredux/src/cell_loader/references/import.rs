//! REFR model import — parse & cache NIF / SpeedTree scenes for a placed
//! reference, plus the flame-attach-offset probe. Split out of the original
//! `cell_loader/references.rs` (#1877).

//! Per-cell reference loading: walk PlacedRefs, expand PKIN/SCOL
//! containers, parse NIFs/SPTs through the registry cache, and dispatch
//! to `spawn_placed_instances` for actual entity creation.
//!
//! The bulk of cell load time lives here — parsing NIFs (cache miss
//! path), expanding container placements, resolving base records,
//! and committing the per-cell NifImportRegistry deltas.

use byroredux_core::ecs::BillboardMode;
use std::sync::Arc;

use crate::asset_provider::{merge_bgsm_into_mesh, MaterialProvider};

use crate::cell_loader::nif_import_registry::CachedNifImport;

use super::attach::{attach_points_component, child_attach_connections_component};

/// Parse + import a NIF scene once. Returns `None` on parse failure
/// or when the scene has zero useful geometry. All per-block parse
/// warnings and the truncation message (if any) are emitted exactly
/// once per unique NIF at this step — subsequent placements read
/// from the cache without re-parsing. See runtime-spam incident from
/// the `AnvilHeinrichOakenHallsHouse` trace.
/// Public re-export of `parse_and_import_nif` for the precombined-mesh
/// loader (#1188). `pub(super)` so only sibling modules in
/// `cell_loader` can reach it.
pub(crate) fn parse_and_import_nif_pub(
    nif_data: &[u8],
    label: &str,
    mat_provider: Option<&mut MaterialProvider>,
    pool: &mut byroredux_core::string::StringPool,
    mesh_resolver: Option<&dyn byroredux_nif::import::MeshResolver>,
) -> Option<Arc<CachedNifImport>> {
    parse_and_import_nif(nif_data, label, mat_provider, pool, mesh_resolver)
}

pub(super) fn parse_and_import_nif(
    nif_data: &[u8],
    label: &str,
    mat_provider: Option<&mut MaterialProvider>,
    pool: &mut byroredux_core::string::StringPool,
    mesh_resolver: Option<&dyn byroredux_nif::import::MeshResolver>,
) -> Option<Arc<CachedNifImport>> {
    let scene = match byroredux_nif::parse_nif(nif_data) {
        Ok(s) => {
            log::debug!("Parsed NIF '{}': {} blocks", label, s.len());
            if s.truncated {
                log::warn!(
                    "  NIF '{}' parsed with truncation — downstream import will \
                     work from the partial block list",
                    label
                );
            }
            s
        }
        Err(e) => {
            log::warn!("Failed to parse NIF '{}': {}", label, e);
            return None;
        }
    };

    // BSXFlags bit 5 — semantics differ across game eras:
    //   * Oblivion / FO3 / FNV (BSVER < FALLOUT4): bit 5 = `EditorMarker`.
    //     The NIF is an invisible CK pin (XMarker, PrisonMarker, etc.)
    //     and must not render.
    //   * Skyrim / FO4 / FO76 / Starfield (BSVER >= FALLOUT4):
    //     bit 5 = `MultiBoundNode` (Bethesda re-purposed it). A hint
    //     that the NIF carries an authored BSMultiBound for culling.
    //     Filtering on it drops legitimate architecture — F4 in the
    //     2026-05-26 sweep was caused by `hitfloorsolidfull01.nif`
    //     (FO4 Institute floor, BSXFlags = 0xA2, bit 5 set) and 14
    //     siblings being wrongly classified as editor markers.
    //
    // For FO4+ we rely on the name-based check in `walk/mod.rs:1430`
    // (`is_editor_marker`) which catches names matching `EditorMarker*`,
    // `marker_*`, `MarkerX`, `marker:*`, `MapMarker` — every shipping
    // FO4 editor-marker NIF authored a name in that family.
    let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
    // NifScene doesn't retain the header, so re-parse the header
    // (~60 bytes) to read `bs_version`. Cheap relative to the full
    // scene parse we already did.
    let bsver = byroredux_nif::header::NifHeader::parse(nif_data)
        .map(|(h, _)| h.user_version_2)
        .unwrap_or(0);
    let bsx_editor_marker = bsx & 0x20 != 0 && bsver < byroredux_nif::version::bsver::FALLOUT4;
    if bsx_editor_marker {
        log::debug!(
            "Skipping editor marker NIF '{}' (BSXFlags 0x{:X}, BSVER {})",
            label,
            bsx,
            bsver,
        );
        return None;
    }
    // Root-node NiAVObject.flags — surfaced for the placement-root
    // SceneFlags row. See #1235 / LC-D1-NEW-01.
    let root_flags = byroredux_nif::import::extract_root_flags(&scene);

    let (mut meshes, collisions) =
        byroredux_nif::import::import_nif_with_collision_and_resolver(&scene, pool, mesh_resolver);
    // FO4+ external material resolution (#493). Walk once at cache-fill
    // time so every REFR sharing this NIF sees the merged texture paths.
    // NIF fields take precedence; only empty slots are filled from the
    // resolved BGSM/BGEM chain.
    if let Some(provider) = mat_provider {
        for mesh in &mut meshes {
            merge_bgsm_into_mesh(mesh, provider, pool);
        }
    }
    let lights = byroredux_nif::import::import_nif_lights(&scene);
    let particle_emitters = byroredux_nif::import::import_nif_particle_emitters(&scene);
    let embedded_clip = byroredux_nif::anim::import_embedded_animations(&scene);
    // Cell-load path doesn't yet attach `Name` components or a
    // per-placement subtree root to spawned mesh entities, so the
    // AnimationStack's name-keyed subtree lookup can't anchor onto the
    // flat-spawn hierarchy. Clips extracted here are captured on the
    // cache entry for a follow-up wiring pass (add placement-root
    // entities + parent meshes under them, then attach a scoped
    // AnimationPlayer per placement). See #261. The loose-NIF
    // `load_nif_bytes` path already consumes embedded clips end-to-end.
    if let Some(ref clip) = embedded_clip {
        log::debug!(
            "NIF '{}' has {} embedded controllers ({} float + {} color + {} bool) \
             — captured on cache; cell-loader spawn wiring is a follow-up",
            label,
            clip.float_channels.len() + clip.color_channels.len() + clip.bool_channels.len(),
            clip.float_channels.len(),
            clip.color_channels.len(),
            clip.bool_channels.len(),
        );
    }
    // #1215 / D2 FIND-1 — surface zero-contribution imports loudly. A
    // NIF that parses cleanly but yields no meshes / collisions / lights /
    // emitters / clips is almost always either a CSG-deferred precombined
    // `_oc.nif` (Shared variant, geometry in companion `.csg` blob —
    // #1188) or a malformed scene. Pre-#1215 these were silently
    // returned as empty `CachedNifImport` entries and the operator
    // hit a "props in a void" symptom downstream with no log clue.
    // The fix is observability-only — cache invariants unchanged.
    if meshes.is_empty()
        && collisions.is_empty()
        && lights.is_empty()
        && particle_emitters.is_empty()
        && embedded_clip.is_none()
    {
        log::warn!(
            "NIF '{}' imported with zero meshes / collisions / lights / \
             emitters / clips — likely CSG-deferred (`_oc.nif` Shared \
             variant, #1188) or pure marker scene",
            label,
        );
    }
    // Phase 18 — walk the scene graph for a flame-marker node and
    // capture its world position relative to the root. Most Skyrim
    // candles + chandeliers + torches author this as a `Flame01` /
    // `AttachFire` / `AttachLight` NiNode child of the root; the
    // ESM-fallback light should sit at that offset, not at the
    // placement root. The nodes array doesn't survive into
    // `CachedNifImport`, so the offset is computed once here and
    // stored as `flame_attach_offset` for the spawn site to read.
    let flame_attach_offset = find_flame_attach_offset(&scene);

    // #985 / #1594 — materialize the FO4+ weapon-mod attach graph. The flat
    // import drops the node array, so pull the `BSConnectPoint` blocks
    // straight off the parsed scene (the transforms are already Y-up — the
    // extractor converts) and intern them into the ECS components here,
    // where the StringPool lives. The spawn site stamps them onto the
    // placement root. `None` for the dominant non-modular case.
    let attach_points = byroredux_nif::import::extract_attach_points(&scene)
        .map(|pts| attach_points_component(&pts, pool));
    let child_attach_connections = byroredux_nif::import::extract_child_attach_connections(&scene)
        .map(|c| child_attach_connections_component(&c, pool));

    Some(Arc::new(CachedNifImport {
        meshes,
        collisions,
        lights,
        particle_emitters,
        embedded_clip,
        // NIF cell-loader path leaves billboard wiring to a follow-up —
        // imported.nodes here represent the whole scene graph, not the
        // placement root, so we'd need a "which node corresponds to the
        // REFR placement" heuristic. Tracked alongside #994.
        placement_root_billboard: None,
        // #1214 / D1-NEW-03 — surface the BSXFlags bits on the cache
        // entry so the spawn site can attach a `BSXFlags` ECS row on
        // the placement root. The editor-marker bit (0x20) is consumed
        // above as an early-return; the remaining bits (havok-managed,
        // ragdoll, articulated, externally-emitted-particles, etc.)
        // ride through to the ECS for downstream consumers.
        bsx_flags: bsx,
        // #1235 / LC-D1-NEW-01 — root-node NiAVObject.flags for
        // placement-root SceneFlags parity with the loose-NIF loader.
        root_flags,
        flame_attach_offset,
        attach_points,
        child_attach_connections,
    }))
}

/// Phase 18 — locate the flame-attach marker node in a parsed NIF
/// scene. Scans every node's name for the canonical flame-marker
/// substrings Skyrim's CK uses, then composes the node's world
/// position relative to the placement root by walking its parent
/// chain.
///
/// Names checked (case-insensitive substring match):
/// - `flame` — `Flame01`, `FlameNode`, `CandleFlame`
/// - `fire` — `FireNode01`, `AttachFire`
/// - `attachlight` — `AttachLight01`
///
/// First match wins. Returns `None` when no matching node is
/// authored — the typical case for static props that ship LIGH
/// data only on the REFR placement (no NIF marker). The spawn
/// path falls back to the placement-root position in that case,
/// preserving pre-Phase-18 behaviour.
///
/// Cost: O(nodes). NIF scenes typically have 10-100 nodes; the
/// search runs once per unique model path at cache fill time
/// and the result is cached across every placement.
pub(super) fn find_flame_attach_offset(scene: &byroredux_nif::scene::NifScene) -> Option<[f32; 3]> {
    const PATTERNS: &[&str] = &["flame", "fire", "attachlight"];

    // Walk raw NIF blocks. Limited to first-level lookup: returns
    // the flame node's local translation (relative to its
    // immediate parent — typically the scene root, where this
    // composes correctly). Deep-nested flame nodes (some
    // chandelier rigs) would need full parent-chain composition
    // by following `children` references back to root; deferred
    // until a visible bug surfaces.
    for idx in 0..scene.blocks.len() {
        // `NifScene::get_as` downcasts the boxed NiObject to the
        // concrete type via `as_any().downcast_ref()`. NiNode
        // carries `av.net.name` + `av.transform.translation` —
        // everything the flame-marker search needs.
        let Some(node) = scene.get_as::<byroredux_nif::blocks::node::NiNode>(idx) else {
            continue;
        };
        let name = match node.name() {
            Some(n) => n,
            None => continue,
        };
        let name_lower = name.to_ascii_lowercase();
        if PATTERNS.iter().any(|p| name_lower.contains(p)) {
            let t = node.transform().translation;
            // NIF is Z-up; the engine is Y-up. Route through the canonical
            // array-form flip so this stays in lockstep with the importer
            // (was an inline `[t.x, t.z, -t.y]` copy — #1318 / TD3-NEW-B).
            return Some(byroredux_core::math::coord::zup_to_yup_pos([t.x, t.y, t.z]));
        }
    }
    None
}

/// Parse a SpeedTree `.spt` byte slice and convert it to the same
/// [`CachedNifImport`] shape every other model goes through. Lets the
/// cache + spawn paths consume `.spt` REFRs without a parallel
/// dispatch tree.
///
/// Today (Phase 1.4 + 1.5) the SPT importer ships the **placeholder
/// fallback** — a single yaw-billboard quad textured with the leaf
/// icon resolved from the matching `TreeRecord` (TREE.ICON wins,
/// `.spt` tag 4003 falls back). When the geometry-tail decoder lands
/// later, `byroredux_spt::import_spt_scene` will start producing
/// real branch / frond meshes + per-leaf billboards without any
/// signature change here.
///
/// Returns `None` on parse failure or when the importer produces no
/// usable geometry (e.g. `.spt` magic missing) so subsequent REFRs
/// of the same model don't re-attempt the doomed parse.
pub(super) fn parse_and_import_spt(
    spt_data: &[u8],
    label: &str,
    tree_record: Option<&byroredux_plugin::esm::records::TreeRecord>,
    pool: &mut byroredux_core::string::StringPool,
) -> Option<Arc<CachedNifImport>> {
    let scene = match byroredux_spt::parse_spt(spt_data) {
        Ok(s) => {
            // #1820 / SPT-NEW-01 — logged sanity check, not a dispatch
            // input: `detect_variant` had zero production callers, which
            // read as a live per-game hook while actually being inert
            // (the placeholder importer below is variant-agnostic).
            // Logging it here gives the Phase 2 geometry-tail decoder a
            // corpus trail to consult once it needs Oblivion-vs-FO3/FNV
            // body disambiguation, without changing today's behaviour.
            let variant = byroredux_spt::detect_variant(spt_data);
            log::debug!(
                "Parsed SPT '{}': {} entries, tail at offset {}, variant={}",
                label,
                s.entries.len(),
                s.tail_offset,
                variant.tag(),
            );
            if !s.unknown_tags.is_empty() {
                log::debug!(
                    "  SPT '{}' bailed at unknown tag {} (offset {}) — \
                     parameter section partial; placeholder still renders",
                    label,
                    s.unknown_tags[0].0,
                    s.unknown_tags[0].1,
                );
            }
            s
        }
        Err(e) => {
            log::warn!("Failed to parse SPT '{}': {}", label, e);
            return None;
        }
    };

    // Build SptImportParams from the matching TREE record. Every
    // field defaults gracefully when the record is absent — a `.spt`
    // referenced from a stub TREE (or from non-TREE content) still
    // gets a generic-sized placeholder.
    let leaf_texture_override = tree_record
        .map(|t| t.leaf_texture.as_str())
        .filter(|s| !s.is_empty());

    let bounds = tree_record.and_then(|t| t.bounds).map(|b| {
        let min = [b.min[0] as f32, b.min[1] as f32, b.min[2] as f32];
        let max = [b.max[0] as f32, b.max[1] as f32, b.max[2] as f32];
        (min, max)
    });

    // Wind sensitivity / strength would come from CNAM, not BNAM
    // (BNAM is billboard-card width/height per UESP). CNAM semantics
    // aren't pinned down yet — Phase 2 wires it. Leave None so the
    // placeholder doesn't pretend to know the wind response.
    let wind = None;

    let form_id = tree_record.map(|t| t.form_id);

    // #1001 — Oblivion ships MODB on 100 % of TREE records and OBND
    // on none, so the placeholder size fallback needs MODB to size
    // Cyrodiil trees correctly (vanilla MODB range 157–3621 game
    // units). FO3/FNV are inverse: 100 % OBND, 0 % MODB. Surface both
    // and let `compute_billboard_size` pick its precedence.
    let bound_radius = tree_record.map(|t| t.bound_radius).filter(|r| *r > 0.0);

    // #1002 — BNAM (FO3/FNV billboard width × height) as a fallback
    // BELOW OBND. Corpus inspection (2026-05-13) showed BNAM clamps
    // tall trees vs their physical OBND extent (e.g. `WhiteOak01`
    // BNAM 768×768 vs OBND 802×1567), so OBND wins for the
    // whole-tree placeholder. BNAM only reaches `compute_billboard_size`
    // when OBND is absent — a rare mod-content case in FO3/FNV.
    let billboard_size = tree_record.and_then(|t| t.billboard_size);

    let params = byroredux_spt::SptImportParams {
        leaf_texture_override,
        bounds,
        wind,
        form_id,
        bound_radius,
        billboard_size,
    };

    let imported = byroredux_spt::import_spt_scene(&scene, &params, pool);

    // #994 — the placeholder root node is authored with
    // `billboard_mode = Some(BsRotateAboutUp)` so the cell-loader spawn
    // can attach a `Billboard` ECS component to the placement root.
    // Pre-#994 this field was dropped because `CachedNifImport` carried
    // no node metadata; trees rendered as static quads facing whichever
    // direction the REFR was authored at.
    let placement_root_billboard = imported
        .nodes
        .first()
        .and_then(|n| n.billboard_mode)
        .map(BillboardMode::from_nif);

    Some(Arc::new(CachedNifImport {
        meshes: imported.meshes,
        // No collisions / lights / particles / animation clips on
        // the placeholder. Real branch geometry might emit a sphere
        // collision (tree-trunk collider) once the geometry tail is
        // decoded — follow-up sub-phase.
        collisions: Vec::new(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
        placement_root_billboard,
        // SpeedTree `.spt` files carry no BSXFlags — they're a
        // separate format outside the NIF block hierarchy. #1214.
        bsx_flags: 0,
        // SpeedTree `.spt` placeholders have no NiAVObject root, so no
        // NiAVObject.flags to propagate. #1235 / LC-D1-NEW-01.
        root_flags: 0,
        // SpeedTree placeholders carry no flame markers — they're
        // pure billboard quads. Phase 18.
        flame_attach_offset: None,
        // SpeedTree `.spt` is a separate format with no BSConnectPoint
        // blocks. #1594.
        attach_points: None,
        child_attach_connections: None,
    }))
}
