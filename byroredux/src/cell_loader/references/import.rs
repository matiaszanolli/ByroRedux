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

use crate::asset_provider::{merge_external_material, MaterialProvider};

use crate::cell_loader::nif_import_registry::CachedNifImport;

use super::attach::{
    attach_points_component, child_attach_connections_component, furniture_component,
};

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

    // #3036 — BSXFlags bit 5 is file-level metadata saying editor-marker
    // children are present; it does not classify the whole NIF as a marker.
    // FNV stool01.nif is the canonical counterexample: bit 5 coexists with
    // real geometry and collision. Marker children are culled individually
    // by the NIF walker, leaving a pure marker import empty naturally.
    // Skyrim+ also reuses this bit for MultiBound metadata, reinforcing that
    // no game era may use it as a whole-file rejection gate.
    let bsx = byroredux_nif::import::extract_bsx_flags(&scene);
    // Root-node NiAVObject.flags — surfaced for the placement-root
    // SceneFlags row. See #1235 / LC-D1-NEW-01.
    let root_flags = byroredux_nif::import::extract_root_flags(&scene);
    let collision_authoring =
        byroredux_nif::import::collision::summarize_collision_authoring(&scene);

    let (mut meshes, collisions) =
        byroredux_nif::import::import_nif_with_collision_and_resolver(&scene, pool, mesh_resolver);
    // FO4+ external material resolution (#493). Walk once at cache-fill
    // time so every REFR sharing this NIF sees the merged texture paths.
    // NIF fields take precedence; only empty slots are filled from the
    // resolved BGSM/BGEM chain.
    if let Some(provider) = mat_provider {
        for mesh in &mut meshes {
            // #2709 (SF-D9-03) — the merge mutates `mesh.material` in
            // place; the outcome is a diagnostic signal this path has no
            // sink for yet (there is no per-cell material tally to feed).
            // Discarded deliberately, not overlooked.
            let _ = merge_external_material(&mut mesh.material, provider, pool);
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

    // M41.5 Phase B — lift `BSFurnitureMarker` sit/sleep/lean positions to
    // the `Furniture` ECS component (the node array doesn't survive into
    // `CachedNifImport`). `None` for the dominant non-furniture case.
    let furniture = {
        let markers = byroredux_nif::import::extract_furniture_markers(&scene);
        if markers.is_empty() {
            None
        } else {
            Some(furniture_component(&markers))
        }
    };

    Some(Arc::new(CachedNifImport {
        meshes,
        collisions,
        collision_authoring,
        lights,
        particle_emitters,
        embedded_clip,
        // NIF cell-loader path leaves billboard wiring to a follow-up —
        // imported.nodes here represent the whole scene graph, not the
        // placement root, so we'd need a "which node corresponds to the
        // REFR placement" heuristic. Tracked alongside #994.
        placement_root_billboard: None,
        speedtree_wind: None,
        // #1214 / D1-NEW-03 — surface the BSXFlags bits on the cache
        // entry so the spawn site can attach a `BSXFlags` ECS row on
        // the placement root. Bit 5 rides through as marker-presence /
        // MultiBound metadata; the walker has already culled individual
        // editor-marker children without discarding their real siblings.
        bsx_flags: bsx,
        // #1235 / LC-D1-NEW-01 — root-node NiAVObject.flags for
        // placement-root SceneFlags parity with the loose-NIF loader.
        root_flags,
        flame_attach_offset,
        attach_points,
        child_attach_connections,
        furniture,
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
/// Parse failures degrade to the placeholder (with a warning) so a malformed
/// `.spt` never removes its REFR from the world; the cache still prevents
/// subsequent placements from re-attempting the doomed parse.

/// Candidate directories a bare `TREE.ICON` filename resolves against, in
/// probe order.
///
/// #3528 — **every vanilla `TREE.ICON` is a bare filename with no directory
/// component**, so the engine's only path normalisation
/// (`normalize_texture_path`, which prepends `textures\`) produced
/// `textures\<Name>.dds` — a path that exists in no shipped archive. The
/// placeholder billboard's one visible surface therefore always fell through
/// to the magenta checker, on 100 % of vanilla SpeedTree content across all
/// three `.spt` games.
///
/// The order is measured, not assumed. A census over `FalloutNV.esm`,
/// `Fallout3.esm` and `Oblivion.esm` (3 + 9 + 81 = 93 unique ICON values, 0
/// containing a path separator) resolved each filename against every texture
/// archive of its game:
///
/// | Directory | ICONs found there |
/// |---|---:|
/// | `textures\trees\leaves\` | **93 / 93** |
/// | `textures\trees\billboards\` | 10 / 93 |
///
/// `leaves\` covers the corpus completely and `billboards\` is a strict
/// subset that is never the sole location — which is what settles the
/// ordering question the finding deliberately left open. `billboards\` stays
/// in the chain as a second probe rather than being dropped, since it costs
/// one `contains` call on the miss path and covers mod content that ships
/// only the billboard variant.
const TREE_ICON_CANDIDATE_DIRS: [&str; 2] = ["trees\\leaves\\", "trees\\billboards\\"];

/// Resolve a `TREE.ICON` value to a path that actually exists in the loaded
/// archives (#3528).
///
/// `probe` answers "does this texture exist", normally
/// `TextureProvider::has_texture` — a `contains` check against the archive
/// file tables, no extraction or decompression. Pure over that closure so the
/// ordering is unit-testable without a BSA on disk.
///
/// The ICON is tried **verbatim first**: an authored path (mod content, or
/// any future vanilla record that carries one) is authoritative and must not
/// be second-guessed by a directory this function invented. Only a bare name
/// that does not already resolve falls through to
/// [`TREE_ICON_CANDIDATE_DIRS`].
///
/// Deliberately scoped to the SpeedTree route. `normalize_texture_path` is
/// shared by every texture consumer in the engine, and `trees\` prefixing is
/// a `TREE.ICON` rule, not a general one — pushing it down there would apply
/// it to every bare-filename texture field in the tree.
pub(super) fn resolve_tree_icon_path<'a>(
    icon: &'a str,
    probe: impl Fn(&str) -> bool,
) -> std::borrow::Cow<'a, str> {
    use std::borrow::Cow;
    if probe(icon) {
        return Cow::Borrowed(icon);
    }
    // An ICON that already names a directory has been tried as authored and
    // missed; prefixing a `trees\` folder onto it would only build a path
    // its author never meant. Leave it alone so the miss is reported against
    // what the record actually said.
    if icon.contains('\\') || icon.contains('/') {
        log::warn!(
            "TREE.ICON '{icon}' names a directory but does not resolve in any \
             loaded texture archive — the SpeedTree placeholder will render \
             with the missing-texture checker (#3528)"
        );
        return Cow::Borrowed(icon);
    }
    for dir in TREE_ICON_CANDIDATE_DIRS {
        let candidate = format!("{dir}{icon}");
        if probe(&candidate) {
            return Cow::Owned(candidate);
        }
    }
    log::warn!(
        "TREE.ICON '{icon}' resolves in no loaded texture archive, verbatim or \
         under {:?} — the SpeedTree placeholder will render with the \
         missing-texture checker (#3528)",
        TREE_ICON_CANDIDATE_DIRS,
    );
    Cow::Borrowed(icon)
}

pub(super) fn parse_and_import_spt(
    spt_data: &[u8],
    label: &str,
    tree_record: Option<&byroredux_plugin::esm::records::TreeRecord>,
    pool: &mut byroredux_core::string::StringPool,
    tex_provider: Option<&crate::asset_provider::TextureProvider>,
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
            // TREE metadata is sufficient for the placeholder. A malformed
            // parameter section must not erase the REFR (#3078).
            byroredux_spt::SptScene::default()
        }
    };

    // Build SptImportParams from the matching TREE record. Every
    // field defaults gracefully when the record is absent — a `.spt`
    // referenced from a stub TREE (or from non-TREE content) still
    // gets a generic-sized placeholder.
    // #3528 — the ICON is a bare filename on 100 % of vanilla TREE records,
    // so it has to be resolved against the archives before it can be the
    // billboard's texture. Without a provider (the loose-`.spt` route and the
    // unit tests) there is nothing to probe against, so the value passes
    // through as authored.
    let resolved_leaf_texture = tree_record
        .map(|t| t.leaf_texture.as_str())
        .filter(|s| !s.is_empty())
        .map(|icon| match tex_provider {
            Some(provider) => resolve_tree_icon_path(icon, |p| provider.has_texture(p)),
            None => std::borrow::Cow::Borrowed(icon),
        });
    let leaf_texture_override = resolved_leaf_texture.as_deref();

    let bounds = tree_record.and_then(|t| t.bounds).map(|b| {
        let min = [b.min[0] as f32, b.min[1] as f32, b.min[2] as f32];
        let max = [b.max[0] as f32, b.max[1] as f32, b.max[2] as f32];
        (min, max)
    });

    // CNAM's positional semantics remain unpinned across the 5-float
    // Oblivion and 8-float Fallout layouts. Do not invent named wind fields
    // from it: the placeholder uses a neutral runtime response until the real
    // SpeedTree parameter layout has a citable decoder (#3190).
    let wind = Some((1.0, 0.0));

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

    // The placeholder mesh carries the billboard mode on the renderable
    // entity; the placement root remains a plain transform anchor (#3076).
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
        collision_authoring: Default::default(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
        placement_root_billboard,
        speedtree_wind: wind,
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
        // SpeedTree placeholders carry no furniture markers. M41.5 Phase B.
        furniture: None,
    }))
}
