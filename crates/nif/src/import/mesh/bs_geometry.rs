//! Starfield `BSGeometry` mesh extraction.
//!
//! External-mesh + internal-geom branches, decoded into the engine's
//! universal mesh shape.

use crate::blocks::bs_geometry::unpack_udec3_xyzw;
use crate::blocks::bs_geometry::BSGeometry;
use crate::scene::NifScene;
use crate::types::{clamp_sign, NiPoint3, NiTransform};

use super::super::coord::{zup_matrix_to_yup_quat, zup_point_to_yup};
use super::super::{ImportedMesh, MeshResolver};
use super::*;
use byroredux_core::string::StringPool;

/// Compose the archive path for a `BSGeometry` external `.mesh` companion,
/// without double-composing one that already carries the head or tail.
///
/// #1292 established that the archive stores these at `geometries\<X>.mesh`
/// while the block holds `X` as a bare content-addressed hash-tree stem
/// (e.g. `aa2d865fc6bf336b909b\e84b59f1a4b705a40845`) — so the importer
/// composes the canonical path rather than teaching every resolver impl
/// Starfield's convention.
///
/// #2361 / SF2D2-04 — but that composition was unconditional, while nifly
/// (the cited wire-format authority) and this codebase's own block-level doc
/// both describe the field as holding *either* a bare stem *or* a full path.
/// A name already carrying `geometries\` and/or `.mesh` composed into
/// `geometries\geometries\x.mesh.mesh` — a guaranteed miss, and a silent one
/// until #2357 gave the resolve-miss path a log. Vanilla is unaffected (every
/// sampled `.mesh` name is a bare 20-hex stem); this is authoring-tool output
/// and mods that use readable paths.
///
/// Head and tail are tested independently — a name may carry either without
/// the other — and both are case- and separator-insensitive, matching
/// `normalize_mesh_path`'s technique in `byroredux/src/asset_provider/archive.rs`.
/// That helper cannot be called from here: it lives in the top-level binary
/// crate, which `crates/nif` does not (and must not) depend on.
fn canonical_mesh_path(mesh_name: &str) -> String {
    const HEAD: &str = "geometries";
    const TAIL: &str = ".mesh";

    let bytes = mesh_name.as_bytes();
    // `geometries` + exactly one separator, either slash form.
    let has_head = bytes.len() > HEAD.len()
        && bytes[..HEAD.len()].eq_ignore_ascii_case(HEAD.as_bytes())
        && (bytes[HEAD.len()] == b'\\' || bytes[HEAD.len()] == b'/');
    // #3391 — byte-slice, NOT `mesh_name[mesh_name.len() - TAIL.len()..]`.
    // `mesh_name` is archive input decoded lossily (`read_sized_string` falls
    // back to `from_utf8_lossy`, so every invalid byte becomes a 3-byte
    // U+FFFD), and a `&str` byte-range slice panics when the cut lands inside
    // a multi-byte scalar — `"модель"` is 12 bytes and `12 - 5` is mid-char.
    // This is the same technique `has_head` uses two lines above, and the one
    // `normalize_mesh_path` uses end-to-end.
    let has_tail = bytes.len() > TAIL.len()
        && bytes[bytes.len() - TAIL.len()..].eq_ignore_ascii_case(TAIL.as_bytes());

    match (has_head, has_tail) {
        (true, true) => mesh_name.to_string(),
        (true, false) => format!("{mesh_name}{TAIL}"),
        (false, true) => format!("{HEAD}\\{mesh_name}"),
        (false, false) => format!("{HEAD}\\{mesh_name}{TAIL}"),
    }
}

pub fn extract_bs_geometry(
    scene: &NifScene,
    shape: &BSGeometry,
    world_transform: &NiTransform,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> Option<ImportedMesh> {
    use crate::blocks::bs_geometry::{BSGeometryMeshData, BSGeometryMeshKind};

    // Try each LOD slot in order; use the first one that yields geometry.
    // Also carry the selected slot's `tri_size`/`num_verts` hints through so
    // they can be cross-checked against the resolved body below (SF2-03 /
    // #1830) regardless of which stage resolved it, plus the slot's own
    // authored index (#2631 / SF2D2-D2-03) — `meshes[0]`/the first entry
    // that survives the sentinel-skip below is the first *present and
    // non-sentinel* slot, not necessarily authored LOD 0.
    let mesh_data_owned: Option<BSGeometryMeshData>;
    let hint: (u32, u32); // (tri_size, num_verts)
    let resolved_slot: u32;
    let mesh_data: &BSGeometryMeshData = if shape.has_internal_geom_data() {
        // Stage A: inline geometry embedded in the NIF. Iterate every LOD
        // slot — `meshes.first()` was a #982 short-circuit that silently
        // returned `None` when LOD 0 was `External` despite later LODs
        // being `Internal` (#1209). Matches the Stage-B iteration.
        //
        // SF2-02 / #1829: a slot's body can itself be the `scale<=0`
        // sentinel (empty `vertices`/`triangles`) — skip those so a
        // sentinel-first slot order doesn't hide a later populated slot.
        let (tri_size, num_verts, slot, data) =
            shape.meshes.iter().find_map(|m| match &m.kind {
                BSGeometryMeshKind::Internal { mesh_data }
                    if !mesh_data.vertices.is_empty() && !mesh_data.triangles.is_empty() =>
                {
                    Some((m.tri_size, m.num_verts, m.lod_slot, mesh_data.as_ref()))
                }
                _ => None,
            })?;
        hint = (tri_size, num_verts);
        resolved_slot = slot;
        data
    } else {
        // Stage B: external `.mesh` companion file. Try each LOD slot until
        // one resolves. When no resolver is provided, skip external geometry.
        //
        // #1292 — the BSGeometry block stores `mesh_name` as a raw
        // content-addressed hash-tree path (e.g.
        // `aa2d865fc6bf336b909b\e84b59f1a4b705a40845`). The archive
        // stores the actual `.mesh` file at the canonical
        // `geometries\<X>.mesh` path. Pre-#1292 the importer passed
        // the raw hash to the resolver, which never matched the
        // archive layout — every external BSGeometry returned `None`,
        // dropping 1 624 of 27 898 Cydonia REFRs to "zero meshes"
        // (99.7% spawn-rate failure on the first Cydonia render
        // attempt, 2026-05-28). Compose the canonical path here in
        // the importer rather than asking every resolver impl to
        // know about Starfield's hash-tree convention.
        // #2357 / SF2D2-03 — the three "no geometry found" exits below
        // (no resolver, per-slot resolve miss, every slot exhausted) used
        // to return `None` with no log signal at all. Only the rarer
        // sub-failures *inside* a successful resolve (parse error,
        // sentinel body) logged, and only at `debug!`. A future archive
        // misconfiguration reproducing #1292's near-total mesh-spawn
        // collapse would otherwise leave an empty log — recovering that
        // diagnosis cost a dedicated investigation session last time.
        let Some(resolver) = resolver else {
            log::debug!(
                "BSGeometry '{}' has no internal geometry and no mesh resolver was \
                 supplied — external .mesh resolve skipped",
                shape.av.net.name.as_deref().unwrap_or("<unnamed>"),
            );
            return None;
        };
        let mut found = None;
        for m in &shape.meshes {
            if let BSGeometryMeshKind::External { mesh_name } = &m.kind {
                let canonical = canonical_mesh_path(mesh_name);
                let Some(bytes) = resolver.resolve(&canonical) else {
                    log::debug!(
                        "BSGeometry external mesh '{}' (canonical '{}') not found by \
                         resolver; trying next LOD",
                        mesh_name,
                        canonical
                    );
                    continue;
                };
                match BSGeometryMeshData::parse_from_bytes(&bytes) {
                    Ok(data) => {
                        // SF2-01 / #1828: a slot can parse `Ok` yet be
                        // the `scale<=0` sentinel (empty
                        // vertices/triangles) — keep iterating so a
                        // sentinel-first slot order doesn't hide a
                        // later populated slot.
                        if !data.vertices.is_empty() && !data.triangles.is_empty() {
                            found = Some((m.tri_size, m.num_verts, m.lod_slot, data));
                            break;
                        }
                        log::debug!(
                            "BSGeometry external mesh '{}' (canonical '{}') \
                             parsed empty (sentinel slot); trying next LOD",
                            mesh_name,
                            canonical
                        );
                    }
                    Err(e) => {
                        log::debug!(
                            "BSGeometry external mesh '{}' (canonical '{}') \
                             parse error: {}",
                            mesh_name,
                            canonical,
                            e
                        );
                    }
                }
            }
        }
        let Some((tri_size, num_verts, slot, data)) = found else {
            log::warn!(
                "BSGeometry '{}' exhausted all {} external LOD slot(s) with no \
                 resolvable geometry — mesh will not spawn (#1292/#2357 symptom: \
                 check archive set / path convention if this is unexpected)",
                shape.av.net.name.as_deref().unwrap_or("<unnamed>"),
                shape.meshes.len(),
            );
            return None;
        };
        hint = (tri_size, num_verts);
        resolved_slot = slot;
        mesh_data_owned = Some(data);
        mesh_data_owned.as_ref().unwrap()
    };

    if mesh_data.vertices.is_empty() || mesh_data.triangles.is_empty() {
        return None;
    }

    // SF2-03 / #1830 — cross-check the NIF-level `tri_size`/`num_verts`
    // hints (always present regardless of internal/external) against the
    // body that actually resolved. A mismatch is a strong signal of a
    // wrong-file resolve (hash collision, stale archive) or a truncated
    // companion — not fatal (the resolved body still renders), but worth
    // surfacing during bring-up rather than leaving it undiagnosable.
    if let Some(mismatch) = bs_geometry_hint_mismatch(
        hint.0,
        hint.1,
        mesh_data.vertices.len() as u32,
        mesh_data.triangles.len() as u32,
    ) {
        log::debug!(
            "BSGeometry '{}' hint/body mismatch: num_verts hint={} resolved={}, \
             tri_size hint={} resolved={} — possible wrong-file resolve \
             (hash collision / stale archive) or truncated companion",
            shape.av.net.name.as_deref().unwrap_or("<unnamed>"),
            mismatch.num_verts_hint,
            mismatch.num_verts_resolved,
            mismatch.tri_size_hint,
            mismatch.tri_size_resolved,
        );
    }

    // Positions are already Y-up (decoded by the BSGeometryMeshData parser).
    let positions: Vec<[f32; 3]> = mesh_data.vertices.to_vec();

    let indices: Vec<u32> = mesh_data
        .triangles
        .iter()
        .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
        .collect();

    // Unpack UDEC3 normals from raw u32 (10:10:10:2 unsigned-fixed). Y-up already.
    // Keep authorship separate from the populated fallback vector: synthesized
    // tangents require real normals, not the renderer-safe placeholder below
    // (#2363).
    let normals_authored = !mesh_data.normals_raw.is_empty();
    let normals: Vec<[f32; 3]> = if normals_authored {
        mesh_data
            .normals_raw
            .iter()
            .map(|&raw| {
                let xyzw = unpack_udec3_xyzw(raw);
                [xyzw[0], xyzw[1], xyzw[2]]
            })
            .collect()
    } else {
        // #3541 — derive rather than substitute a constant. Starfield's
        // `.mesh` corpus is measured at 0 of 675,407 affected (there is no
        // distant-LOD `.mesh` content), so this arm is inert on shipped
        // Starfield today; it is here so the rule is uniform across every
        // extraction path rather than special-cased per game, per NIFAL.
        super::normal::synthesize_normals_or_default(&positions, &mesh_data.triangles)
    };

    // #2099 (SF2D2-02) — `mesh_data.uvs1` (the secondary UV channel;
    // Starfield uses it for detail/decal and some shader-layering work)
    // decodes correctly on the parser side but has no consumer here.
    // `ImportedMesh`/`Vertex` have only one UV slot today, so there is
    // nowhere to route it to — not actionable until the vertex format
    // grows a second UV channel. Affected content renders with the
    // primary UV set only (visual-only, no crash/corruption risk).
    let uvs = mesh_data.uvs0.clone();

    // Authored tangents from the UDEC3-packed `tangents_raw` channel.
    // BSGeometry is Starfield-native Y-up — no Z-up → Y-up axis swap is
    // needed (unlike the Oblivion/Skyrim NiBinaryExtraData path, which
    // is Z-up and requires `bs_tangents_zup_to_yup`). The 2-bit W
    // channel from `unpack_udec3_xyzw` carries the bitangent sign.
    // Without this decode, every Starfield mesh fell through to the
    // shader's screen-space derivative Path-2 in `perturbNormal`,
    // producing lower-quality normal maps and inverted normals on
    // UV-mirrored geometry (#1086 / REN-D16-001).
    let tangents: Vec<[f32; 4]> = if !mesh_data.tangents_raw.is_empty() {
        mesh_data
            .tangents_raw
            .iter()
            .map(|&raw| {
                let xyzw = unpack_udec3_xyzw(raw);
                // #2246 (REN-D19-02) — the W channel is only 2 bits, so
                // `unpack_udec3_xyzw`'s [0,3] → [-1,1] remap can land on
                // -1/3 or +1/3, not just ±1, unlike every other game's
                // import path (`bitangent_sign()` in `types.rs`, used by
                // `tangent.rs` / `sse_recon.rs`), which always derives an
                // exact ±1.0. Normalize here (via the shared `clamp_sign`,
                // #2313 / TD2-115) so no downstream consumer of
                // `vertexTangent.w` needs its own defensive re-clamp —
                // `material_sampling.glsl`'s `perturbNormal` already does
                // one (`vertexTangent.w < 0.0 ? -1.0 : 1.0`), but that
                // shouldn't be the only thing standing between an
                // off-nominal packed value and a wrong tangent frame.
                let bitangent_sign = clamp_sign(xyzw[3]);
                [xyzw[0], xyzw[1], xyzw[2], bitangent_sign]
            })
            .collect()
    } else if normals_authored && !uvs.is_empty() && !positions.is_empty() {
        // #1232 — no authored UDEC3 tangents but geometry is otherwise
        // populated. Route through the Y-up synthesis sibling instead of
        // dropping to `Vec::new()`, which would force every such mesh to
        // the shader's screen-space derivative Path-2 in `perturbNormal`
        // and inherit the #1104 UV-mirror handedness bug. Mirrors the
        // #1204 BSTriShape SSE-reconstructed branch — both consumers
        // share the helper now that `synthesize_tangents_yup` exists.
        // #2363 — the fallback normal vector is deliberately populated for
        // rendering, so testing `normals.is_empty()` here was vacuous and
        // synthesized a tangent basis from fabricated `[0, 1, 0]` normals.
        // `mesh_data.triangles` is already `Vec<[u16; 3]>`, so no index
        // conversion is needed (BSGeometry is Starfield-native Y-up).
        synthesize_tangents_yup(&positions, &normals, &uvs, &mesh_data.triangles)
    } else {
        Vec::new()
    };

    // Vertex colors: u8 RGBA → f32 [0, 1].
    let colors: Vec<[f32; 4]> = if !mesh_data.colors.is_empty() {
        mesh_data
            .colors
            .iter()
            .map(|&[r, g, b, a]| {
                [
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                    a as f32 / 255.0,
                ]
            })
            .collect()
    } else {
        vec![[1.0, 1.0, 1.0, 1.0]; positions.len()]
    };

    let mat = super::super::material::extract_material_info_from_refs(
        scene,
        shape.shader_property_ref,
        shape.alpha_property_ref,
        &[],
        &[],
        pool,
    );

    // #1203 — resolve the BSSkin::Instance + BSSkin::BoneData chain
    // hanging off `skin_instance_ref`, plus (#2613) the per-vertex bone
    // index/weight channel from `mesh_data.skin_weights` — decoded at
    // parse time since #873, plumbed through here since #2613. #1827
    // (FO4-D4-02) closed on the stale premise that this was undecoded;
    // it wasn't — just never passed to the extractor.
    let skin = extract_skin_bs_geometry(scene, shape, mesh_data);

    let t = &world_transform.translation;
    let quat = zup_matrix_to_yup_quat(&world_transform.rotation);

    // BSGeometry bounding sphere is already in Y-up (Starfield-native), so
    // use it directly when radius > 0; otherwise fall back to centroid.
    let (local_bound_center, local_bound_radius) = {
        let [cx, cy, cz] = shape.bounding_sphere.0;
        let r = shape.bounding_sphere.1;
        if r > 0.0 {
            // #2098 (SF2D2-01) — the raw authored sphere is used verbatim
            // here with no cross-check that it's expressed in the same
            // units as the decoded vertices. If a future content source
            // (or a parser regression) ever authors/decodes the two in
            // divergent scales (e.g. a havok-scaled sphere against
            // renderer-scaled vertices), the bound could be far too
            // small — off-axis culling pop. Non-fatal, log-only, mirrors
            // `bs_geometry_hint_mismatch`'s pattern.
            if let Some(mismatch) =
                bs_geometry_bounding_sphere_mismatch([cx, cy, cz], r, &positions)
            {
                log::debug!(
                    "BSGeometry '{}' bounding-sphere/vertex-extent mismatch: \
                     sphere_radius={} vertex_extent_radius={} — possible \
                     unit-scale divergence between the authored sphere and \
                     the decoded vertices",
                    shape.av.net.name.as_deref().unwrap_or("<unnamed>"),
                    mismatch.sphere_radius,
                    mismatch.vertex_extent_radius,
                );
            }
            ([cx, cy, cz], r)
        } else {
            extract_local_bound(
                NiPoint3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                0.0,
                &positions,
            )
        }
    };

    let material = mat.into_imported_material(pool, shape.av.net.name.as_deref());

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        tangents,
        uvs,
        indices,
        translation: zup_point_to_yup(t),
        rotation: quat,
        scale: world_transform.scale,
        material,
        name: shape.av.net.name.clone(),
        parent_node: None,
        skin,
        local_bound_center,
        local_bound_radius,
        flags: shape.av.flags,
        bs_lod_cutoffs: None,
        bs_sub_index: None,
        bs_geometry_lod_slot: Some(resolved_slot),
        billboard_mode: None,
        // #3231 — GPU morph-blend consumer is out of scope for Starfield's
        // BSGeometry in this pass (v1 targets NiTriShape/BSTriShape only,
        // matching the concrete FaceGen use case). Not wired here.
        morph_targets: None,
    })
}

/// Extract a BSGeometry with local transform (for hierarchical import).
pub fn extract_bs_geometry_local(
    scene: &NifScene,
    shape: &BSGeometry,
    pool: &mut StringPool,
    resolver: Option<&dyn MeshResolver>,
) -> Option<ImportedMesh> {
    extract_bs_geometry(scene, shape, &shape.av.transform, pool, resolver)
}

/// Reported when [`bs_geometry_hint_mismatch`] finds the NIF-level
/// `BSGeometryMesh` hints disagree with the resolved mesh body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BsGeometryHintMismatch {
    pub num_verts_hint: u32,
    pub num_verts_resolved: u32,
    pub tri_size_hint: u32,
    pub tri_size_resolved: u32,
}

/// SF2-03 / #1830 — cross-checks a `BSGeometryMesh` slot's `tri_size` /
/// `num_verts` hints against the vertex/triangle counts of the body that
/// actually resolved for it (either the inline `BSGeometryMeshData` or the
/// parsed external `.mesh` companion). Returns `None` when they agree.
///
/// `tri_size` is a byte-size hint over the triangle-index stream, which is
/// always 3 `u16` indices per triangle (6 bytes) — see
/// `BSGeometryMeshData::parse`'s `n_tri_indices` read.
pub(crate) fn bs_geometry_hint_mismatch(
    tri_size_hint: u32,
    num_verts_hint: u32,
    resolved_verts: u32,
    resolved_tri_count: u32,
) -> Option<BsGeometryHintMismatch> {
    let tri_size_resolved = resolved_tri_count.saturating_mul(6);
    if num_verts_hint == resolved_verts && tri_size_hint == tri_size_resolved {
        return None;
    }
    Some(BsGeometryHintMismatch {
        num_verts_hint,
        num_verts_resolved: resolved_verts,
        tri_size_hint,
        tri_size_resolved,
    })
}

/// Reported when [`bs_geometry_bounding_sphere_mismatch`] finds the raw
/// authored bounding-sphere radius far smaller than the decoded vertices'
/// actual extent from that same center.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BsGeometryBoundingSphereMismatch {
    pub sphere_radius: f32,
    pub vertex_extent_radius: f32,
}

/// #2098 (SF2D2-01) — cross-checks a BSGeometry block's raw authored
/// `bounding_sphere` radius against the actual max distance-from-center of
/// the decoded vertex positions. Mirrors [`bs_geometry_hint_mismatch`]'s
/// pattern: informational only (a real unit-scale mismatch would still
/// render, just with a wrong culling bound), `None` when they broadly
/// agree.
///
/// Only flags the sphere being far too SMALL relative to the vertices —
/// the failure mode that causes off-axis culling pop. A merely loose /
/// conservative authored bound (sphere bigger than the tight vertex
/// extent) is normal Bethesda authoring practice and intentionally not
/// flagged. `MIN_RATIO` is generous (10%) so this only fires on a genuine
/// scale divergence (the audit's ~70x hypothesis), not ordinary
/// bounding-sphere slack.
pub(crate) fn bs_geometry_bounding_sphere_mismatch(
    center: [f32; 3],
    sphere_radius: f32,
    positions: &[[f32; 3]],
) -> Option<BsGeometryBoundingSphereMismatch> {
    if sphere_radius <= 0.0 || positions.is_empty() {
        return None;
    }
    let vertex_extent_radius = positions
        .iter()
        .map(|p| {
            let dx = p[0] - center[0];
            let dy = p[1] - center[1];
            let dz = p[2] - center[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        })
        .fold(0.0f32, f32::max);
    if vertex_extent_radius <= 0.0 {
        return None;
    }
    const MIN_RATIO: f32 = 0.1;
    if sphere_radius >= vertex_extent_radius * MIN_RATIO {
        return None;
    }
    Some(BsGeometryBoundingSphereMismatch {
        sphere_radius,
        vertex_extent_radius,
    })
}

#[cfg(test)]
mod canonical_mesh_path_tests {
    use super::canonical_mesh_path;

    /// The vanilla shape #1292 fixed: a bare content-addressed hash-tree
    /// stem gets both the head and the tail.
    #[test]
    fn bare_stem_gets_head_and_tail() {
        assert_eq!(
            canonical_mesh_path(r"aa2d865fc6bf336b909b\e84b59f1a4b705a40845"),
            r"geometries\aa2d865fc6bf336b909b\e84b59f1a4b705a40845.mesh"
        );
    }

    /// #2361 — a name that already carries both must pass through
    /// unchanged rather than becoming
    /// `geometries\geometries\...mesh.mesh`.
    #[test]
    fn fully_qualified_path_is_not_double_composed() {
        let already = r"geometries\aa2d865fc6bf336b909b\e84b59f1a4b705a40845.mesh";
        assert_eq!(canonical_mesh_path(already), already);
    }

    /// Head and tail are independent — a name may carry either alone.
    #[test]
    fn head_only_and_tail_only_are_completed_not_duplicated() {
        assert_eq!(
            canonical_mesh_path(r"geometries\foo\bar"),
            r"geometries\foo\bar.mesh"
        );
        assert_eq!(canonical_mesh_path(r"foo\bar.mesh"), r"geometries\foo\bar.mesh");
    }

    /// Both tests are case-insensitive and accept either separator, since
    /// archive paths are inconsistent about both.
    #[test]
    fn head_and_tail_detection_ignore_case_and_separator() {
        assert_eq!(
            canonical_mesh_path(r"GEOMETRIES\Foo\Bar.MESH"),
            r"GEOMETRIES\Foo\Bar.MESH"
        );
        assert_eq!(canonical_mesh_path("Geometries/foo/bar"), r"Geometries/foo/bar.mesh");
    }

    /// A stem that merely *starts with* the letters `geometries` but is not
    /// the directory (no separator) must still be prefixed — otherwise a
    /// hash beginning with those characters would silently lose its head.
    #[test]
    fn head_match_requires_a_separator_not_just_a_prefix() {
        assert_eq!(
            canonical_mesh_path("geometriesabc123"),
            r"geometries\geometriesabc123.mesh"
        );
    }

    /// Likewise a name that is exactly the tail, or exactly the head, is not
    /// treated as already-composed — guards the length comparisons against
    /// an off-by-one that would accept an empty stem.
    #[test]
    fn degenerate_names_are_still_composed() {
        assert_eq!(canonical_mesh_path(".mesh"), r"geometries\.mesh.mesh");
        assert_eq!(canonical_mesh_path("geometries"), r"geometries\geometries.mesh");
    }

    /// #3391 — a non-ASCII name must compose, not panic.
    ///
    /// The tail test used to slice the `&str` by byte range, so any name
    /// whose last five bytes straddled a multi-byte scalar aborted the
    /// process. `mesh_name` reaches here from `read_sized_string`, which
    /// falls back to `from_utf8_lossy` — so both of these are real archive
    /// inputs, and neither is caught: `extract_bs_geometry` runs on the main
    /// thread outside every `catch_unwind` guard, under `panic = "unwind"`.
    #[test]
    fn non_ascii_names_compose_instead_of_panicking() {
        // Valid UTF-8, 12 bytes: `12 - 5 = 7` lands inside 'е'.
        assert_eq!(
            canonical_mesh_path("модель"),
            "geometries\\модель.mesh"
        );
        // Lossily-decoded invalid bytes: each becomes a 3-byte U+FFFD, so
        // the cut lands inside the first replacement char.
        let lossy = String::from_utf8_lossy(&[0xFF, 0xFF]).into_owned();
        assert_eq!(canonical_mesh_path(&lossy), format!("geometries\\{lossy}.mesh"));
        // A non-ASCII name that genuinely IS already composed still passes
        // through untouched — the boundary fix must not change semantics.
        assert_eq!(
            canonical_mesh_path("geometries\\модель.mesh"),
            "geometries\\модель.mesh"
        );
    }
}
