//! Per-sub-mesh instance spawning: resolve the mesh/material/texture paths,
//! upload (or reuse a cached) GPU mesh, and stamp the render + physics +
//! bounds components onto the placement child entity.
//!
//! Split out of `spawn.rs` (#2410 / TD1-007), which had crossed 2000 LOC on
//! production code alone — `spawn_mesh_instance` was 546 LOC of it. Contents
//! moved verbatim; only the visibility of the items `spawn.rs` still calls
//! was widened.

use super::*;
use byroredux_core::ecs::SpeedTreeWind;
use byroredux_core::string::FixedString;
use byroredux_nif::import::{
    slot_to_colocated_role, slot_to_role, TextureRole, TextureSlotContext,
};

/// Effective per-mesh texture-slot paths, resolved in one StringPool
/// lock (#882). Promoted to module scope from `spawn_placed_instances`
/// so `resolve_mesh_paths` + `spawn_mesh_instance` can share it (#2057).
pub(super) struct ResolvedMeshPaths {
    textures: byroredux_nif::import::MaterialTextureSet<Option<String>>,
    sources: byroredux_nif::import::MaterialTextureSet<MaterialTextureSource>,
    material_path: Option<String>,
    name_sym: Option<byroredux_core::string::FixedString>,
    /// #4229 / FNV-D2-02 — the mesh's own un-overlaid base-color path,
    /// captured before `textures.base_color` is overwritten by the REFR
    /// overlay resolve below. Forwarded to `translate_material` so it can
    /// detect when the overlay actually swapped the effective texture and
    /// recompute the stale NIF-import-time PBR classification instead of
    /// carrying it onto a materially different surface.
    source_base_color: Option<String>,
    /// #4290 — the swap target's external material merged onto the mesh's
    /// pre-merge snapshot, when a REFR material swap (MSWP / XMSP, or a TXST
    /// MNAM) changed this shape's `material_path`. `None` when no swap fired;
    /// read through [`ResolvedMeshPaths::material`], never directly.
    swapped_material: Option<byroredux_nif::import::ImportedMaterial>,
}

impl ResolvedMeshPaths {
    /// The raw material every spawn-side consumer of this shape reads: the
    /// swapped material when a REFR material swap fired, otherwise the
    /// cached mesh's own merged material.
    pub(super) fn material<'a>(
        &'a self,
        mesh: &'a byroredux_nif::import::ImportedMesh,
    ) -> &'a byroredux_nif::import::ImportedMaterial {
        self.swapped_material.as_ref().unwrap_or(&mesh.material)
    }
}

fn resolve_to_owned(
    pool: &byroredux_core::string::StringPool,
    sym: Option<byroredux_core::string::FixedString>,
) -> Option<String> {
    sym.and_then(|s| pool.resolve(s)).map(|s| s.to_string())
}

/// Resolve every mesh's effective texture-slot paths + interned name
/// under a single StringPool lock (#882). Split out of
/// `spawn_placed_instances` (#2057).
///
/// `mat_provider` is `Some` on the same REFR-processing paths that already
/// build a `RefrTextureOverlay` (see `build_refr_texture_overlay`'s own
/// `mat_provider` param) — needed here so a per-shape MSWP swap (#973 /
/// FO4-D4-NEW-08-followup) can walk the swapped target's BGSM/BGEM chain,
/// not just substitute the `material_path` string.
///
/// Takes the cache entry's pre-merge material snapshots
/// ([`crate::cell_loader::nif_import_registry::CachedNifImport`]'s
/// `pre_merge_materials`, parallel to `imported`). With them, a REFR
/// material swap re-merges the swap TARGET's sidecar onto the NIF-authored
/// material instead of only walking its textures (#4290); without them (an
/// empty slice) a swap on a mesh that had its own sidecar keeps the cached
/// material, as before.
pub(super) fn resolve_mesh_paths_with_pre_merge(
    world: &mut World,
    imported: &[byroredux_nif::import::ImportedMesh],
    pre_merge: &[Option<byroredux_nif::import::ImportedMaterial>],
    refr_overlay: Option<&RefrTextureOverlay>,
    mut mat_provider: Option<&mut MaterialProvider>,
    tex_provider: Option<&crate::asset_provider::TextureProvider>,
) -> Vec<ResolvedMeshPaths> {
    let ov = refr_overlay;
    let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
    imported
        .iter()
        .enumerate()
        .map(|(sub_mesh_index, mesh)| {
            // #973 / FO4-D4-NEW-08-followup — apply the REFR's XMSP
            // material-swap table per shape. `build_refr_texture_overlay`
            // only substitutes ONE shape's `material_path` (whichever the
            // overlay's own XATO/XTNM already carries); every other shape
            // in a multi-shape mesh (e.g. a Raider armour's separate body /
            // arm / leg pieces) was silently left on its NIF-authored BGSM.
            //
            // Re-evaluates the FNAM path-prefix filter against THIS
            // shape's own source path (not the overlay's shared one), then
            // — only when a substitution actually changes the path — walks
            // the swapped target's BGSM/BGEM chain via a shape-scoped clone
            // of the overlay. Cloning first means any slot the REFR-level
            // XATO/XTNM/XTXR already set stays put (`fill_from_bgsm`'s
            // first-empty-wins policy never overwrites it); only slots the
            // per-shape swap actually contributes get filled. When no swap
            // fires, `shape_ov` stays `None` and behaviour is identical to
            // pre-#973.
            let mut shape_ov: Option<RefrTextureOverlay> = None;
            if let Some(refr_ov) = ov {
                if !refr_ov.material_swaps.is_empty() {
                    let base_path_sym = refr_ov.material_path.or(mesh.material.material_path);
                    if let Some(current) = resolve_to_owned(&pool, base_path_sym) {
                        let filter_ok = refr_ov
                            .material_swaps_filter
                            .and_then(|f| pool.resolve(f).map(str::to_owned))
                            .is_none_or(|f| {
                                current
                                    .to_ascii_lowercase()
                                    .starts_with(&f.to_ascii_lowercase())
                            });
                        if filter_ok {
                            let mut swapped = current.clone();
                            // Authoring-order, later-wins: matches the MSWP
                            // file format (later `(BNAM, SNAM)` pair for the
                            // same source overrides an earlier one).
                            //
                            // #3242 — compare against `current` (never
                            // mutated), not the running `swapped` output.
                            // Comparing against `swapped` broke later-wins
                            // for a duplicate `source` (only the first
                            // matching entry ever fired — the reverse of
                            // "later entry overrides") and could silently
                            // chain unrelated entries (A→B→C) whenever one
                            // entry's `target` happened to equal a later
                            // entry's `source`. Mirrors the sibling
                            // reference implementation, `refr.rs`'s
                            // `build_refr_texture_overlay` loop, which
                            // already compared against a fixed value.
                            for entry in &refr_ov.material_swaps {
                                if entry.source.eq_ignore_ascii_case(&current)
                                    && !entry.target.is_empty()
                                {
                                    swapped = entry.target.clone();
                                }
                            }
                            if swapped != current {
                                let mut ov2 = refr_ov.clone();
                                ov2.material_path = Some(pool.intern(&swapped));
                                if let Some(provider) = mat_provider.as_deref_mut() {
                                    ov2.fill_from_bgsm(provider, &mut pool);
                                }
                                shape_ov = Some(ov2);
                            }
                        }
                    }
                }
            }
            let ov = shape_ov.as_ref().or(ov);

            // #4290 — a material swap replaces the whole material, not just
            // its textures. `mesh.material` was merged at cache-fill time
            // with the shape's OWN sidecar, and that merge is fill-if-unset
            // with one-way `two_sided` / `alpha_test` / `is_decal` flags, so
            // the target merged over it would keep the source's values. Merge
            // the target onto the pre-merge snapshot instead: the same
            // precedence rule (NIF-authored fields win), the swapped sidecar.
            let swapped_material = ov
                .and_then(|o| o.material_path)
                .filter(|&target| Some(target) != mesh.material.material_path)
                .and_then(|target| {
                    let base = match pre_merge.get(sub_mesh_index) {
                        Some(Some(snapshot)) => snapshot,
                        // No sidecar of its own: the merge never ran, so the
                        // cached material already is the pre-merge state.
                        _ if mesh.material.material_path.is_none() => &mesh.material,
                        _ => return None,
                    };
                    let provider = mat_provider.as_deref_mut()?;
                    let mut material = base.clone();
                    material.material_path = Some(target);
                    // #2709 — outcome discarded; no per-shape tally sink.
                    let _ = crate::asset_provider::merge_external_material(
                        &mut material,
                        provider,
                        &mut pool,
                    );
                    Some(material)
                });
            let material = swapped_material.as_ref().unwrap_or(&mesh.material);

            // #4400 — seed from the SWAPPED material, not the pre-swap
            // `mesh.material`: every role `resolve_effective` covers below
            // re-reads `material.textures.<role>` anyway, but the roles it
            // does NOT (BGEM glass `glass_roughness_scratch` /
            // `glass_dirt_overlay`) rode the initial seed and kept the
            // source sidecar's maps while `sources` (seeded from the
            // swapped material since #4290) reported the target's
            // provenance.
            let mut textures = material
                .textures
                .map_ref(|path| resolve_to_owned(&pool, *path));
            let mut sources =
                material
                    .textures
                    .zip_map_ref(&material.texture_sources, |path, source| {
                        if path.is_some() {
                            (*source).into()
                        } else {
                            MaterialTextureSource::Absent
                        }
                    });
            let resolve_effective =
                |override_path: Option<FixedString>,
                 mesh_path: Option<FixedString>,
                 mesh_source: MaterialTextureSource| {
                    if let Some(path) = override_path {
                        (
                            resolve_to_owned(&pool, Some(path)),
                            MaterialTextureSource::TxstOverride,
                        )
                    } else if let Some(path) = mesh_path {
                        (resolve_to_owned(&pool, Some(path)), mesh_source)
                    } else {
                        (None, MaterialTextureSource::Absent)
                    }
                };
            // Effective texture slot paths. REFR overlay
            // (XATO/XTNM/XTXR) wins over the NIF-authored paths
            // when present; for slots the overlay left empty the
            // cached NIF's texture rides through. `None` on both
            // sides means the slot has no texture. See #584.
            //
            // `ov` above is per-shape: either `shape_ov` (this shape's own
            // MSWP-swapped BGSM textures, filling slots the REFR-level
            // overlay left empty) or the plain REFR overlay when no swap
            // fired for this shape. Both report through the same
            // `TxstOverride` source label at `mesh.info` — a per-shape MSWP
            // fill is, from the debug command's point of view, exactly
            // that: a REFR-scoped override winning over the mesh's own
            // authored material. See #973.
            //
            // #4229 — capture the mesh's own un-overlaid base-color path
            // BEFORE the overlay resolve below overwrites it, so
            // `translate_material` can detect an actual overlay swap.
            let source_base_color = textures.base_color.clone();
            (textures.base_color, sources.base_color) = resolve_effective(
                ov.and_then(|o| o.diffuse),
                material.textures.base_color,
                sources.base_color,
            );
            // Oblivion/FO3 ship normal maps via the `<base>_n.dds`
            // load-time convention, not an explicit NIF slot. When the
            // mesh left both normal/bump slots empty, derive the sibling
            // from the (effective) diffuse path (#1303 / OBL-D4-NEW-01).
            //
            // #3551 — but only when the sibling actually exists. The derive
            // used to fire on every game, and FO4 / Skyrim author normals
            // explicitly (BGSM, `BSLightingShaderProperty`) with no `_n.dds`
            // convention at all, so an empty normal slot there is a mesh
            // that genuinely has no normal map. Every fabricated path was a
            // wasted archive lookup per shape and a phantom `src=derived-
            // normal` row in `tex.missing`, where it drowned the real
            // misses (measured: 13/16 of FO4's misses, 8/10 of Skyrim's).
            //
            // `has_texture` is the same archive index lookup `resolve_texture`
            // would have done before failing, so the existence check is
            // strictly cheaper than the probe it replaces — and it needs no
            // per-game answer, which is what a `GameKind` gate would have
            // required for FNV. A mesh on any game that really does ship the
            // sibling still gets it.
            (textures.normal, sources.normal) = resolve_effective(
                ov.and_then(|o| o.normal),
                material.textures.normal,
                sources.normal,
            );
            if textures.normal.is_none() {
                textures.normal = textures.base_color.as_deref().and_then(|base| {
                    match tex_provider {
                        Some(provider) => derive_present_normal_map_path(provider, base),
                        // No provider is the bare-`World` unit-test path,
                        // which has no archives to answer with; keep deriving
                        // there so the string transform stays exercised.
                        None => Some(derive_normal_map_path(base)),
                    }
                });
                if textures.normal.is_some() {
                    sources.normal = MaterialTextureSource::DerivedNormal;
                }
            }
            // #2695 — the overlay stores its slots under NIF-slot names
            // (`glow` IS slot 2, `height` IS slot 3, `inner` IS slot 6,
            // `specular` IS slot 7), but which canonical role a slot means
            // depends on the host mesh's `BSLightingShaderType`. Resolve
            // through the SAME table the importer used, with the shader type
            // the importer recorded, so an XTXR swap lands where the mesh's own
            // texture set would have landed.
            //
            // Pre-fix the overlay used a flat shader-type-agnostic table
            // (0→diffuse, 1→normal, 2→glow, 3→height, 4→env, 5→env_mask,
            // 6→inner, 7→specular) and the two disagreed on slots 2, 3, 4/5 and
            // 7 — so an override on a FaceTint / SkinTint / MultiLayerParallax
            // placement changed shading *semantics*, not just the texture.
            //
            // `slot_role_pick` yields the overlay path only when this slot
            // really does carry the role being filled. A slot with no canonical
            // role for this shader type resolves to `None`, and the override is
            // dropped rather than guessed at — matching the importer, which
            // parks the same slots.
            // Computed before the slot routing because slot 7's role depends on
            // it (alternate specular vs. nothing).
            let effective_model_space_normals =
                material.model_space_normals || ov.is_some_and(|o| o.model_space_normals);
            let slot_context = TextureSlotContext {
                layout: material.texture_slot_layout,
                shader_type: material.shader_type,
                glow_map: material.slot2_glow_enabled,
                model_space_normals: effective_model_space_normals,
                soft_lighting: material.soft_lighting,
                rim_lighting: material.rim_lighting,
                back_lighting: material.back_lighting,
            };
            // #3732 (NIFAL-2026-08-30-D8-01) — `slot_to_role` alone cannot
            // express slot colocation: #3458 established that Skyrim/
            // Starfield tint-family slot 2 is genuinely two roles at once
            // (`Tint` AND `LightingMask`, both riding the one `*_sk.dds`
            // texture) and introduced `slot_to_colocated_role` to return
            // the second role. The NIF import loop
            // (`dedicated_shader.rs`) already consults both, first-wins;
            // this overlay path only consulted `slot_to_role`, so the
            // `lighting_mask` pick below could never match on the exact
            // population #3458 was about — a REFR TXST override on a
            // tint-family mesh with soft/rim lighting silently dropped its
            // lighting-mask override while the shader gate crossed anyway.
            let pick = |slot: u32, raw: Option<FixedString>, role: TextureRole| {
                raw.filter(|_| {
                    slot_to_role(slot_context, slot) == Some(role)
                        || slot_to_colocated_role(slot_context, slot) == Some(role)
                })
            };

            // #4434 — the four `bgsm_*` overlay fields carry their ROLE
            // directly (fill_from_bgsm no longer routes through wire-slot
            // fields), so they bind without `slot_to_role`; the wire-slot
            // picks below remain the TXST/XTXR path and keep precedence,
            // matching the old first-wins fill order between an XATO/XTXR
            // override and the MNAM BGSM chain.
            (textures.emissive, sources.emissive) = resolve_effective(
                ov.and_then(|o| {
                    pick(2, o.glow, TextureRole::Emissive).or(o.bgsm_emissive)
                }),
                material.textures.emissive,
                sources.emissive,
            );
            // Slot 2 on the tint family (FaceTint / SkinTint / HairTint) is the
            // `*_sk.dds` skin-tint mask, not a glow map. (#4434 — a BGSM glow
            // map no longer lands in `o.glow`, so it can never arrive here.)
            (textures.tint, sources.tint) = resolve_effective(
                ov.and_then(|o| pick(2, o.glow, TextureRole::Tint)),
                material.textures.tint,
                sources.tint,
            );
            (textures.height, sources.height) = resolve_effective(
                ov.and_then(|o| pick(3, o.height, TextureRole::Height).or(o.bgsm_height)),
                material.textures.height,
                sources.height,
            );

            // #3596 — Oblivion's `APPLY_HILIGHT2` parallax route. The
            // importer records the *rule* ("this material's height lives in
            // the height texture's alpha") but cannot bind the texture,
            // because Oblivion authors no normal slot: the normal map only
            // exists once `derive_normal_map_path` has synthesized it, which
            // is here. Bind the derived normal into the height slot so the
            // route can actually fire. `parallax_height_in_alpha` is only
            // ever set with `parallax_map` absent, so this cannot displace an
            // authored height map.
            //
            // The shader-side alpha-presence gate (#3562) still applies: a
            // BC1/BC5 normal with no real alpha is not treated as height.
            if textures.height.is_none() && material.parallax_height_in_alpha {
                textures.height = textures.normal.clone();
                if textures.height.is_some() {
                    sources.height = sources.normal;
                }
            }
            (textures.greyscale_lut, sources.greyscale_lut) = resolve_effective(
                ov.and_then(|o| {
                    pick(3, o.height, TextureRole::GreyscaleLut).or(o.bgsm_greyscale_lut)
                }),
                material.textures.greyscale_lut,
                sources.greyscale_lut,
            );
            // Slot 3 on FaceTint is a complexion detail map; routing it to
            // `height` made the shader ray-march POM over a face.
            (textures.detail, sources.detail) = resolve_effective(
                ov.and_then(|o| pick(3, o.height, TextureRole::Detail)),
                material.textures.detail,
                sources.detail,
            );
            // BGSM authors smoothness/specular-strength separately from its
            // standalone specular-colour map (#3234). Neither is a raw TXST
            // slot here, so preserve the canonical role directly.
            // #4424 — FO4 slot 7 routes to SmoothSpec (it names the same
            // `_s.dds` file BGSM calls `smooth_spec_texture`), so a TXST
            // override sitting in the overlay's raw slot-7 field lands in
            // the smooth-spec lane on FO4 shapes. Skyrim keeps its own
            // slot-7 readings (BackLighting / MSN specular) below.
            (textures.smooth_spec, sources.smooth_spec) = resolve_effective(
                ov.and_then(|o| {
                    o.smooth_spec
                        .or_else(|| pick(7, o.specular, TextureRole::SmoothSpec))
                }),
                material.textures.smooth_spec,
                sources.smooth_spec,
            );
            (textures.environment, sources.environment) = resolve_effective(
                ov.and_then(|o| pick(4, o.env, TextureRole::Environment)),
                material.textures.environment,
                sources.environment,
            );
            (textures.environment_mask, sources.environment_mask) = resolve_effective(
                ov.and_then(|o| pick(5, o.env_mask, TextureRole::EnvironmentMask)),
                material.textures.environment_mask,
                sources.environment_mask,
            );
            (textures.inner_layer, sources.inner_layer) = resolve_effective(
                ov.and_then(|o| {
                    pick(6, o.inner, TextureRole::InnerLayer).or(o.bgsm_inner_layer)
                }),
                material.textures.inner_layer,
                sources.inner_layer,
            );
            // Specular comes from Skyrim MSN slot 7 or FO76 slot 6. FO4 slot
            // 7 is the smooth-spec role since #4424, so its pick moved to
            // the smooth-spec resolve above. The overlay field names remain
            // raw-slot names; the table chooses the source (#2998/#3085).
            let specular_override = ov.and_then(|o| {
                o.external_specular
                    .or_else(|| pick(6, o.inner, TextureRole::Specular))
                    .or_else(|| pick(7, o.specular, TextureRole::Specular))
            });
            (textures.specular, sources.specular) = resolve_effective(
                specular_override,
                material.textures.specular,
                sources.specular,
            );
            (textures.lighting_mask, sources.lighting_mask) = resolve_effective(
                ov.and_then(|o| pick(2, o.glow, TextureRole::LightingMask)),
                material.textures.lighting_mask,
                sources.lighting_mask,
            );
            (textures.back_lighting, sources.back_lighting) = resolve_effective(
                ov.and_then(|o| pick(7, o.specular, TextureRole::BackLighting)),
                material.textures.back_lighting,
                sources.back_lighting,
            );
            // `wrinkle` is an FO4/FO76 TX02 role, not a BSShaderTextureSet slot
            // index, so `merge_from_texture_set`'s direct `o.wrinkle` fill
            // does not go through the slot table — support.rs already made
            // the game-level TX02 routing decision, unconditionally correct
            // regardless of shader type.
            //
            // #3187 — an XTXR slot-5 swap is a SECOND, ambiguous source:
            // `apply_slot_swap` cannot know at ESM-parse time whether the
            // TXST it swapped in stored TX02's value under `env_mask` or
            // `wrinkle` (game-dependent), so it always lands in the
            // overlay's `env_mask` field. Route it through `pick` here,
            // the same way the `EnvironmentMask` pick above does for its
            // own reading of slot 5 — `slot_to_role` resolves the real
            // per-shape role (`Wrinkle` only on the FO4 tint family today;
            // `EnvironmentMask` everywhere else), so exactly one of the two
            // picks can ever accept a given slot-5 swap.
            (textures.wrinkle, sources.wrinkle) = resolve_effective(
                ov.and_then(|o| {
                    o.wrinkle
                        .or_else(|| pick(5, o.env_mask, TextureRole::Wrinkle))
                }),
                material.textures.wrinkle,
                sources.wrinkle,
            );
            // #2594 — `lighting` / `flow` are BGSM-only roles with no
            // BSShaderTextureSet wire-slot analog either (same shape as
            // `wrinkle` above): a raw TXST/XTXR override can never
            // supply them, only a `.bgsm`/`.bgem` `material_path`
            // override via `fill_from_bgsm` can.
            (textures.lighting, sources.lighting) = resolve_effective(
                ov.and_then(|o| o.lighting),
                material.textures.lighting,
                sources.lighting,
            );
            (textures.flow, sources.flow) = resolve_effective(
                ov.and_then(|o| o.flow),
                material.textures.flow,
                sources.flow,
            );
            let material_path = resolve_to_owned(
                &pool,
                ov.and_then(|o| o.material_path)
                    .or(mesh.material.material_path),
            );
            // Intern the mesh name in the same lock — see #882's
            // second hotspot. `mesh.name: Option<Arc<str>>`. The
            // `pool.intern` call must follow the resolves so the
            // `&pool` borrows from `resolve_to_owned` end before
            // the `&mut pool` re-borrow.
            let name_sym = mesh.name.as_deref().map(|n| pool.intern(n));
            ResolvedMeshPaths {
                textures,
                sources,
                material_path,
                name_sym,
                source_base_color,
                swapped_material,
            }
        })
        .collect()
    // pool guard dropped here at end of block.
}

/// Immutable per-placement context shared by `spawn_mesh_instance` —
/// bundles the REFR transform + render/overlay inputs so the helper's
/// signature stays legible. Split out of `spawn_placed_instances`
/// (#2057). All fields are `Copy`.
#[derive(Clone, Copy)]
pub(super) struct PlacementCtx<'a> {
    pub(super) tex_provider: &'a TextureProvider,
    pub(super) ref_pos: Vec3,
    pub(super) ref_rot: Quat,
    pub(super) ref_scale: f32,
    pub(super) base_layer: byroredux_core::ecs::components::RenderLayer,
    pub(super) mesh_cache_key: Option<&'a str>,
    /// #3510 — per-mesh geometry representative (see
    /// [`crate::cell_loader::nif_import_registry::CachedNifImport`]). Empty
    /// means the identity mapping, which is every path but FO4 precombines.
    pub(super) geometry_dedup: &'a [u32],
    pub(super) refr_overlay: Option<&'a RefrTextureOverlay>,
    pub(super) light_data: Option<&'a esm::cell::LightData>,
    pub(super) light_animation_flags: u32,
    pub(super) light_shadow_flags: u32,
    // #2439 (NIFAL-D2-01) — see `spawn_placed_instances`'s matching params.
    pub(super) light_kind: byroredux_core::ecs::LightKind,
    pub(super) light_direction: [f32; 3],
    pub(super) light_outer_angle: f32,
    // REN-D10-2026-09-20-01 (#4514) — LIGH falloff exponent, already resolved
    // through `canonical_light_falloff_exponent` by the caller (which has
    // `game`) exactly like the flags/geometry lanes above. This file's
    // ESM-light fallback is the third `from_legacy_world_units` spawn site;
    // it used to read the raw LIGH record field, letting the pre-Skyrim
    // 32-byte 0.0 sentinel reach `Emitter`'s non-ESM `1.0` net instead of
    // the quadratic `2.0` pre-Skyrim layouts mean.
    pub(super) light_falloff_exponent: f32,
    pub(super) placement_root: byroredux_core::ecs::EntityId,
    pub(super) collision_fallback: MissingCollisionFallback,
    pub(super) spawned_nif_lights: usize,
}

/// Replace one authored alpha-over fog/smoke mesh with an analytic local
/// medium before any texture upload, raster entity, or BLAS work occurs.
fn prepare_fog_mesh_instance(
    pc: &PlacementCtx,
    mesh: &byroredux_nif::import::ImportedMesh,
    paths: &ResolvedMeshPaths,
) -> Option<byroredux_core::ecs::FogVolume> {
    let texture_path = paths.textures.base_color.as_deref();
    let fog_semantics = pc.mesh_cache_key.is_some_and(crate::fog::has_fog_token)
        || texture_path.is_some_and(crate::fog::has_fog_token)
        || mesh.name.as_deref().is_some_and(crate::fog::has_fog_token);
    let Some(fog_volume) = crate::fog::fog_volume_from_mesh(pc.mesh_cache_key, texture_path, mesh)
    else {
        if fog_semantics {
            log::debug!(
                target: "byroredux::fog",
                "authored fog mesh candidate kept on legacy path: model={:?} texture={:?} \
                 name={:?} has_alpha={} dst_blend={} material_kind={}",
                pc.mesh_cache_key,
                texture_path,
                mesh.name,
                mesh.material.has_alpha,
                mesh.material.dst_blend_mode,
                mesh.material.material_kind,
            );
        }
        return None;
    };
    log::debug!(
        target: "byroredux::fog",
        "replaced authored fog mesh with local volume: model={:?} texture={:?} name={:?}",
        pc.mesh_cache_key,
        texture_path,
        mesh.name,
    );

    Some(fog_volume)
}

fn spawn_fog_mesh_instance(
    world: &mut World,
    pc: &PlacementCtx,
    mesh: &byroredux_nif::import::ImportedMesh,
    paths: &ResolvedMeshPaths,
    fog_volume: byroredux_core::ecs::FogVolume,
) {
    use byroredux_core::ecs::{Name, Parent};

    let nif_rotation = Quat::from_xyzw(
        mesh.rotation[0],
        mesh.rotation[1],
        mesh.rotation[2],
        mesh.rotation[3],
    );
    let nif_position = Vec3::from_array(mesh.translation);
    let (world_position, world_rotation, world_scale) = GlobalTransform::compose_trs(
        pc.ref_pos,
        pc.ref_rot,
        pc.ref_scale,
        nif_position,
        nif_rotation,
        mesh.scale,
    );

    let entity = world.spawn();
    world.insert(
        entity,
        Transform::new(nif_position, nif_rotation, mesh.scale),
    );
    world.insert(
        entity,
        GlobalTransform::new(world_position, world_rotation, world_scale),
    );
    world.insert(entity, fog_volume);
    world.insert(entity, Parent(pc.placement_root));
    crate::helpers::add_child(world, pc.placement_root, entity);
    if let Some(symbol) = paths.name_sym {
        world.insert(entity, Name(symbol));
    }
}

#[derive(Clone, Copy)]
pub(super) enum PreparedMeshUpload {
    Fog(byroredux_core::ecs::FogVolume),
    Ready { handle: u32, fresh_for_rt: bool },
    Failed,
}

struct FreshMeshUpload {
    sub_mesh_index: usize,
    vertices: Vec<byroredux_renderer::Vertex>,
    for_rt: bool,
}

/// Resolve cache hits up front, then upload every fresh submesh through one
/// packed transfer submission. Entity creation remains in the original
/// submesh order after this preparation step, preserving hierarchy and stable
/// placement semantics while eliminating two fence waits per fresh submesh.
pub(super) fn prepare_mesh_uploads(
    ctx: &mut VulkanContext,
    pc: &PlacementCtx,
    imported: &[byroredux_nif::import::ImportedMesh],
    paths: &[ResolvedMeshPaths],
) -> Vec<PreparedMeshUpload> {
    let mut prepared = vec![PreparedMeshUpload::Failed; imported.len()];
    let mut fresh = Vec::new();
    // #3510 — indices whose geometry belongs to an earlier mesh. They are
    // resolved after the batch upload below, by acquiring the
    // representative's cache entry, so N instances of one FO4 precombine
    // object share one upload, one BLAS and (because the draw batcher keys
    // on mesh handle) one instanced draw. Deduping is only possible with a
    // cache key to acquire through; without one the mapping is ignored and
    // behaviour is exactly as before.
    let dedup_active = geometry_dedup_active(
        pc.mesh_cache_key.is_some(),
        pc.geometry_dedup.len(),
        imported.len(),
    );
    let mut shared: Vec<usize> = Vec::new();

    for (sub_mesh_index, mesh) in imported.iter().enumerate() {
        if let Some(fog_volume) = prepare_fog_mesh_instance(pc, mesh, &paths[sub_mesh_index]) {
            prepared[sub_mesh_index] = PreparedMeshUpload::Fog(fog_volume);
            continue;
        }
        if dedup_active && pc.geometry_dedup[sub_mesh_index] as usize != sub_mesh_index {
            shared.push(sub_mesh_index);
            continue;
        }
        let sub_mesh_index_u32 = sub_mesh_index as u32;
        if let Some(handle) = pc
            .mesh_cache_key
            .and_then(|key| ctx.mesh_registry.acquire_cached(key, sub_mesh_index_u32))
        {
            prepared[sub_mesh_index] = PreparedMeshUpload::Ready {
                handle,
                fresh_for_rt: false,
            };
            continue;
        }
        if !ctx.mesh_registry.scene_geometry_admission_open() {
            // The registry has already reached its bounded resident geometry
            // allowance. Keep cache hits above usable, but do not decode,
            // batch, and fail every remaining fresh mesh in this cell.
            continue;
        }

        let material = paths[sub_mesh_index].material(mesh);
        let for_rt = ctx.device_caps.ray_query_supported
            && material.material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
            && !material.is_decal;
        fresh.push(FreshMeshUpload {
            sub_mesh_index,
            vertices: super::super::lod_support::imported_mesh_to_vertices(mesh),
            for_rt,
        });
    }

    if fresh.is_empty() {
        resolve_shared_geometry(ctx, pc, &shared, &mut prepared);
        return prepared;
    }

    let uploads = fresh
        .iter()
        .map(|fresh_mesh| {
            let mesh = &imported[fresh_mesh.sub_mesh_index];
            SceneMeshUpload {
                vertices: &fresh_mesh.vertices,
                indices: &mesh.indices,
                rt_enabled: fresh_mesh.for_rt,
                cache_key: pc
                    .mesh_cache_key
                    .map(|key| (key, fresh_mesh.sub_mesh_index as u32)),
            }
        })
        .collect::<Vec<_>>();
    let allocator = ctx.allocator.as_ref().expect("renderer allocator missing");
    let upload_ctx = GpuUploadCtx {
        device: &ctx.device,
        allocator,
        queue: &ctx.graphics_queue,
        command_pool: ctx.transfer_pool,
    };
    let transfer_fence = std::sync::Arc::clone(&ctx.transfer_fence);
    match ctx.mesh_registry.upload_scene_meshes_batched(
        upload_ctx,
        &uploads,
        transfer_fence.as_ref(),
    ) {
        Ok(handles) => {
            for (fresh_mesh, handle) in fresh.iter().zip(handles) {
                prepared[fresh_mesh.sub_mesh_index] = PreparedMeshUpload::Ready {
                    handle,
                    fresh_for_rt: fresh_mesh.for_rt,
                };
            }
        }
        Err(batch_error) => {
            if !ctx.mesh_registry.scene_geometry_admission_open() {
                log::warn!(
                    "Scene geometry admission limit reached while uploading {} submeshes: \
                     {batch_error:#}; skipping remaining uncached geometry for this cell",
                    fresh.len(),
                );
                resolve_shared_geometry(ctx, pc, &shared, &mut prepared);
                return prepared;
            }
            // Preserve the scalar path as a compatibility fallback. A single
            // malformed/empty submesh must not suppress every valid sibling
            // just because they shared a proposed transfer transaction.
            log::warn!(
                "Batched mesh upload failed for {} submeshes: {batch_error:#}; \
                 falling back to individual uploads",
                fresh.len(),
            );
            for fresh_mesh in &fresh {
                let mesh = &imported[fresh_mesh.sub_mesh_index];
                let allocator = ctx.allocator.as_ref().expect("renderer allocator missing");
                let upload_ctx = GpuUploadCtx {
                    device: &ctx.device,
                    allocator,
                    queue: &ctx.graphics_queue,
                    command_pool: ctx.transfer_pool,
                };
                let upload_result = match pc.mesh_cache_key {
                    Some(key) => ctx.mesh_registry.register_scene_mesh_keyed(
                        upload_ctx,
                        &fresh_mesh.vertices,
                        &mesh.indices,
                        fresh_mesh.for_rt,
                        None,
                        (key, fresh_mesh.sub_mesh_index as u32),
                    ),
                    None => ctx.mesh_registry.upload_scene_mesh(
                        upload_ctx,
                        &fresh_mesh.vertices,
                        &mesh.indices,
                        fresh_mesh.for_rt,
                        None,
                    ),
                };
                match upload_result {
                    Ok(handle) => {
                        prepared[fresh_mesh.sub_mesh_index] = PreparedMeshUpload::Ready {
                            handle,
                            fresh_for_rt: fresh_mesh.for_rt,
                        };
                    }
                    // #3406 — `{error:#}` keeps anyhow's source chain; `{error}`
                    // printed only the outermost context.
                    Err(error) => log::warn!("Failed to upload mesh: {error:#}"),
                }
            }
        }
    }

    resolve_shared_geometry(ctx, pc, &shared, &mut prepared);
    prepared
}

/// #3510 — whether the geometry-dedup mapping can be applied to this
/// placement.
///
/// Both conditions are load-bearing. Without a `mesh_cache_key` there is no
/// cache entry to acquire a shared handle through, so the mapping has
/// nothing to redirect to. A length mismatch means the mapping did not come
/// from this mesh list — indexing it would silently pair an instance with an
/// unrelated geometry, which is worse than not deduping.
pub(crate) fn geometry_dedup_active(
    has_cache_key: bool,
    dedup_len: usize,
    mesh_count: usize,
) -> bool {
    has_cache_key && dedup_len == mesh_count
}

/// #3510 — hand every deduped instance its representative's mesh handle.
///
/// Goes through `acquire_cached` rather than copying the handle directly
/// because the refcount has to rise once per holder: each instance spawns
/// its own entity carrying a `MeshHandle`, and cell unload calls
/// `drop_mesh` once per entity. Copying the handle would under-count and
/// free the shared geometry while the remaining instances still reference
/// it. A representative that failed to upload leaves its instances
/// `Failed`, exactly as the un-deduped path would have.
fn resolve_shared_geometry(
    ctx: &mut VulkanContext,
    pc: &PlacementCtx,
    shared: &[usize],
    prepared: &mut [PreparedMeshUpload],
) {
    if shared.is_empty() {
        return;
    }
    let Some(key) = pc.mesh_cache_key else {
        return;
    };
    for &index in shared {
        let representative = pc.geometry_dedup[index];
        match ctx.mesh_registry.acquire_cached(key, representative) {
            // `fresh_for_rt: false` — the representative already queued its
            // BLAS build this placement; a second request for the same mesh
            // handle would be redundant work against the acceleration
            // manager's reserve floors, which is half of what #3510 is about.
            Some(handle) => {
                prepared[index] = PreparedMeshUpload::Ready {
                    handle,
                    fresh_for_rt: false,
                }
            }
            None => {
                log::debug!(
                    "precombine dedup: representative sub-mesh {representative} of '{key}' is \
                     not cached, so instance {index} has no geometry to share"
                );
            }
        }
    }
}

/// Spawn the render entity (+ optional physics ghost + ESM light
/// fallback) for one imported sub-mesh. Returns `true` when an entity
/// was spawned (the caller then increments its placement count); a
/// failed GPU upload returns `false`, mirroring the pre-split `continue`.
/// Split out of `spawn_placed_instances` (#2057).
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_mesh_instance(
    world: &mut World,
    ctx: &mut VulkanContext,
    pc: &PlacementCtx,
    cached: &CachedNifImport,
    mesh: &byroredux_nif::import::ImportedMesh,
    paths: &ResolvedMeshPaths,
    count: usize,
    prepared: PreparedMeshUpload,
    blas_specs: &mut Vec<(u32, u32, u32)>,
    synthesized_collision_proxy: &mut bool,
) -> bool {
    use byroredux_core::ecs::{Name, Parent};
    let PlacementCtx {
        // #3510 — consumed in `prepare_mesh_uploads`, which runs before this
        // per-instance spawn; nothing here needs it.
        geometry_dedup: _,
        tex_provider,
        ref_pos,
        ref_rot,
        ref_scale,
        base_layer,
        mesh_cache_key: _,
        refr_overlay,
        light_data,
        light_animation_flags,
        light_shadow_flags,
        light_kind,
        light_direction,
        light_outer_angle,
        light_falloff_exponent,
        placement_root,
        collision_fallback,
        spawned_nif_lights,
    } = *pc;
    let (mesh_handle, fresh_for_rt) = match prepared {
        PreparedMeshUpload::Fog(fog_volume) => {
            spawn_fog_mesh_instance(world, pc, mesh, paths, fog_volume);
            return true;
        }
        PreparedMeshUpload::Ready {
            handle,
            fresh_for_rt,
        } => (handle, fresh_for_rt),
        PreparedMeshUpload::Failed => return false,
    };
    if fresh_for_rt {
        blas_specs.push((
            mesh_handle,
            mesh.positions.len() as u32,
            mesh.indices.len() as u32,
        ));
    }

    // Pre-resolved texture slot paths from the single-lock
    // pre-pass above (#882). Cloned per-mesh because the Material
    // ECS component owns its `Option<String>` fields and the
    // resolved-paths Vec stays alive across this iteration; the
    // alternative — moving paths out of `resolved_paths[i]` — would
    // need a swap-with-default to keep the Vec indexable for the
    // texture-handle resolves below. Per-slot clone is one
    // allocation per populated slot per mesh, same as the pre-fix
    // `resolve_owned(...).clone()` pattern at the Material struct
    // construction site.
    let eff_textures = paths.textures.clone();
    let eff_texture_path = eff_textures.base_color.clone();
    let eff_material_path = paths.material_path.clone();
    // #4290 — every raw-material read below sees the swap target's merged
    // material when a REFR material swap fired for this shape.
    let source_material = paths.material(mesh);

    // Canonical material translation — the single boundary that
    // resolves a raw `ImportedMesh` into the engine `Material`
    // (PBR resolved, glass classified once, flag union packed).
    // The cell path contributes the REFR-overlay model-space-normals
    // bit as `extra_material_flags`; everything else is shared with
    // the loose-NIF path. See `material_translate.rs`.
    //
    // #2571 / OBL-D5-01 — computed here, ahead of the texture-clamp
    // resolve just below, so every `texture_clamp_mode`/`src_blend_mode`/
    // `dst_blend_mode` read for the rest of this function goes through
    // this one canonical `Material` instead of re-reading the raw
    // `mesh.material` tier at each use site.
    let extra_material_flags = refr_overlay
        .filter(|o| o.model_space_normals)
        .map(|_| byroredux_renderer::vulkan::material::material_flag::MODEL_SPACE_NORMALS)
        .unwrap_or(0);
    let material = crate::material_translate::translate_material(
        source_material,
        mesh.name.as_deref(),
        crate::material_translate::ResolvedPaths {
            textures: eff_textures.clone(),
            material_path: eff_material_path.clone(),
            source_base_color: paths.source_base_color.clone(),
        },
        extra_material_flags,
    );

    // Load texture (shared resolve: cache → BSA → fallback).
    // #610 — pass the diffuse-slot `TexClampMode` so the bindless
    // descriptor's sampler picks the matching `VkSamplerAddressMode`
    // pair. CLAMP-authored decals / scope reticles / Oblivion
    // architecture trim no longer render with the legacy
    // REPEAT/REPEAT bleed.
    let tex_handle = resolve_texture_with_clamp(
        ctx,
        tex_provider,
        eff_texture_path.as_deref(),
        material.texture_clamp_mode,
    );

    // #544 — mesh entities now sit in the NIF-local frame and
    // descend from the placement root. The transform-propagation
    // system composes `placement_root` (the REFR transform) onto
    // them each frame to produce the world-space `GlobalTransform`
    // the renderer / BLAS / lighting consume. Pre-#544 every mesh
    // pre-baked the REFR composition into its own `Transform`,
    // which left it anchored to nothing the embedded animation
    // clip could walk to.
    //
    // The composed `final_*` values are still computed up front
    // because the `GlobalTransform` we seed on the mesh has to
    // match what the propagation pass will compute on the first
    // tick — anything that reads `GlobalTransform` before then
    // (renderer's per-frame data collection, BLAS build below)
    // gets a correctly-placed value in the meantime.
    let nif_quat = Quat::from_xyzw(
        mesh.rotation[0],
        mesh.rotation[1],
        mesh.rotation[2],
        mesh.rotation[3],
    );
    let nif_pos = Vec3::new(
        mesh.translation[0],
        mesh.translation[1],
        mesh.translation[2],
    );

    // World-space placement — used only to seed the initial
    // `GlobalTransform`. `Transform` itself stays NIF-local so
    // the propagation pass produces the same value next tick. The
    // parent→child composition order lives in `compose_trs`.
    let (final_pos, final_rot, final_scale) =
        GlobalTransform::compose_trs(ref_pos, ref_rot, ref_scale, nif_pos, nif_quat, mesh.scale);

    // Diagnostic: log meshes with significant NIF-internal offsets
    // (these are wall/structural pieces most likely to show positioning issues)
    let nif_offset_len = nif_pos.length();
    if nif_offset_len > 50.0 {
        log::debug!(
            "  NIF offset {:.0} for mesh {:?}: nif_pos=({:.0},{:.0},{:.0}) \
                 final=({:.0},{:.0},{:.0})",
            nif_offset_len,
            mesh.name,
            nif_pos.x,
            nif_pos.y,
            nif_pos.z,
            final_pos.x,
            final_pos.y,
            final_pos.z,
        );
    }

    let entity = world.spawn();
    // NIF-local Transform for hierarchy propagation; world-space
    // GlobalTransform for first-tick consumers. See #544.
    world.insert(entity, Transform::new(nif_pos, nif_quat, mesh.scale));
    world.insert(
        entity,
        GlobalTransform::new(final_pos, final_rot, final_scale),
    );
    // #3231 / #4399 — GPU morph-target slots are created only where the
    // canonical read gate (`bone_offset != 0`, which requires a
    // `SkinnedMesh` on the entity) can actually fire. This cell path
    // attaches no `SkinnedMesh` (#2440 tracks that gap), so the slots it
    // used to create behind the raw `mesh.skin.is_some()` gate were
    // staged into by `AnimatedMorphWeights` yet read by no draw — dead
    // GPU weight/delta buffers on arrival, kept warm by the #4294 LRU
    // and #3661 residency work for nothing. Creation now lives in
    // [`try_spawn_morph_slot`], called from the loose-NIF / NPC path in
    // `scene/nif_loader.rs` where the `SkinnedMesh` is actually built;
    // when #2440 lands a cell-path `SkinnedMesh`, call it there too.
    // #1213 / D1-NEW-02 — seed LocalBound from the mesh-local
    // bounding sphere (`ImportedMesh.local_bound_center`,
    // `.local_bound_radius`, both extracted by the NIF importer
    // from `NiTriShapeData.center` / `BsTriShape.center` or
    // computed from vertex positions). The bounds-propagation
    // system at `byroredux/src/systems/bounds.rs:43-66` reads
    // this row and produces a world-space `WorldBound` each
    // frame; pre-#1213 no LocalBound row was ever inserted, so
    // every WorldBound stayed at the component default (zero
    // sphere) and downstream culling / RT-budget / cell-bounds
    // consumers fell through to coarser approximations.
    world.insert(
        entity,
        LocalBound::new(
            Vec3::new(
                mesh.local_bound_center[0],
                mesh.local_bound_center[1],
                mesh.local_bound_center[2],
            ),
            mesh.local_bound_radius,
        ),
    );
    // Sibling to the LocalBound insert above. `bounds.rs` Pass 1 at
    // line 61-63 only *updates* a pre-existing `WorldBound` row —
    // it does not insert one — so a missing seed row means the
    // entity stays at `WorldBound::default()` (zero sphere) and is
    // invisible to ray-cast picking, frustum culling, and the
    // skinned-LRU bounds heuristic. The propagation pass overwrites
    // this `ZERO` with the real value on the next tick.
    world.insert(entity, WorldBound::ZERO);
    // #1235 / LC-D1-NEW-01 — attach SceneFlags for parity with the
    // loose-NIF loader (`scene/nif_loader.rs:789-791`). APP_CULLED
    // shapes never reach this point (filtered import-side in
    // `walk/mod.rs`); the remaining NiAVObject bits ride through
    // for downstream consumers.
    if mesh.flags != 0 {
        world.insert(entity, SceneFlags::from_nif(mesh.flags));
    }
    // #2206 / NIFAL-D4-02 — per-mesh parity with the loose-NIF loader
    // (`scene/nif_loader.rs`'s `node.billboard_mode` attach). The flat
    // cell-loader walk spawns one entity per mesh with no node entities
    // at all, so the nearest-ancestor `NiBillboardNode` mode set by
    // `walk_node_flat` rides on the mesh itself instead of a node.
    if let Some(raw) = mesh.billboard_mode {
        world.insert(entity, Billboard::new(BillboardMode::from_nif(raw)));
    }
    // SpeedTree's neutral runtime response is cached on the placement import,
    // while the billboard mode is carried by each placeholder mesh. Attach
    // both components to the render child consumed by the billboard system.
    if let Some((response, stiffness)) = cached.speedtree_wind {
        world.insert(entity, SpeedTreeWind::new(response, stiffness));
    }
    // Parent/Children edge → embedded animation clip's subtree
    // walk discovers this mesh through `placement_root`.
    world.insert(entity, Parent(placement_root));
    crate::helpers::add_child(world, placement_root, entity);
    // Name from `ImportedMesh.name` so the clip's node-keyed
    // channels (`FixedString` interned at parse time, #340)
    // resolve through `build_subtree_name_map` to this entity.
    // Pre-#544 the cell-loader path skipped this insert, so even
    // if `Parent` had been wired the channels would have failed
    // their name lookup and silently no-op'd.
    //
    // Pre-#882 this site re-acquired a `world.resource_mut::<
    // StringPool>()` write lock per mesh. The intern is now done
    // in the pre-pass above; this site only consumes the cached
    // `FixedString`.
    if let Some(sym) = paths.name_sym {
        world.insert(entity, Name(sym));
    }
    world.insert(entity, MeshHandle(mesh_handle));
    world.insert(entity, TextureHandle(tex_handle));
    // `material` was computed above, ahead of the texture-clamp resolve.
    // `world.insert` below moves it, so pull copies of the small Copy
    // fields the rest of this function still reads (#2571).
    let material_kind = material.material_kind;
    let mesh_water = material.is_water_shader;
    let canonical_clamp_mode = material.texture_clamp_mode;
    let canonical_src_blend_mode = material.src_blend_mode;
    let canonical_dst_blend_mode = material.dst_blend_mode;
    // #3073 (NIFAL-D1) — read the already-resolved canonical values
    // instead of re-deriving `.unwrap_or(0.04)` / `.unwrap_or(4.0)` from
    // the raw `mesh.material` tier at the `MaterialTextureHandles` insert
    // below.
    let canonical_parallax_height_scale = material.parallax_height_scale;
    let canonical_parallax_max_passes = material.parallax_max_passes;
    world.insert(entity, material);
    // PERF-D3-NEW-02 / #1136 — classify FX-decoration meshes at spawn
    // time so build_render_data can skip them via a component query
    // instead of running 6 substring scans per draw per frame.
    if let Some(ref tp) = eff_texture_path {
        if texture_path_is_fx_mesh(tp, material_kind) {
            world.insert(entity, IsFxMesh);
        }
    }
    // Resolve every secondary semantic role with the SAME authored clamp mode
    // as base colour, and derive the channel-presence flags, through the one
    // shared producer (#4529) so structures, clutter, actors, and exterior
    // statics cannot drift against the loose-NIF spawn path.
    let texture_handles = build_material_texture_handles(
        ctx,
        tex_provider,
        &eff_textures,
        tex_handle,
        canonical_clamp_mode,
        canonical_parallax_height_scale,
        canonical_parallax_max_passes,
    );
    world.insert(entity, texture_handles);
    if mesh_water {
        crate::material_translate::attach_mesh_water(
            world,
            entity,
            texture_handles.textures.normal,
            texture_handles.textures.flow,
            crate::material_translate::MeshWaterSource {
                name: mesh.name.as_deref(),
                positions: &mesh.positions,
                position: final_pos,
                rotation: final_rot,
                scale: final_scale,
                local_bound_center: Vec3::new(
                    mesh.local_bound_center[0],
                    mesh.local_bound_center[1],
                    mesh.local_bound_center[2],
                ),
                local_bound_radius: mesh.local_bound_radius,
                phantom_bounds: cached.phantom_bounds,
                root_transform: (ref_pos, ref_rot, ref_scale),
            },
        );
    }
    world.insert(
        entity,
        MaterialTextureDebugInfo {
            paths: eff_textures,
            sources: paths.sources,
            clamp_mode: canonical_clamp_mode,
        },
    );
    // #1480 / REN-D22-NEW-01 — resolve the normal-alpha-as-spec roughness
    // ONCE into the canonical Material now that MaterialTextureHandles is
    // attached, instead of recomputing it per
    // draw in the render path. Reads the same components the renderer
    // reads, so the value is identical — only canonical + tooling-visible.
    // #2606 — pass the "a real BGSM authored the PBR scalars" signal so the
    // legacy fallback cannot clobber them.
    crate::material_translate::resolve_normal_alpha_spec_roughness(
        world,
        entity,
        source_material.bgsm_pbr_scalars_authored,
    );
    // #2826 (REN-D19-02) — same "resolve once from MaterialTextureHandles"
    // pattern, for whether the model-space normal map's blue channel
    // carries authored Z.
    crate::material_translate::resolve_msn_z_source(world, entity);
    // #3905 (NIFAL-2026-09-05-D1-01) — same pattern again, for a BGSM
    // pinned at the near-mirror clamp floor whose authored gloss map did
    // not resolve. Must run AFTER MaterialTextureHandles is attached:
    // the whole point is to use the shader's resolved-handle predicate
    // rather than the merge boundary's authored-path one.
    crate::material_translate::resolve_unresolved_gloss_neutral_roughness(
        world,
        entity,
        source_material.bgsm_pbr_scalars_authored,
    );
    // #2490 — the blend/decal/facing markers derive from the raw
    // `ImportedMaterial` at the same single boundary the `Material`
    // literal does, so this path cannot diverge from the loose-NIF path.
    crate::material_translate::attach_blend_and_facing_markers(
        world,
        entity,
        source_material,
        canonical_src_blend_mode,
        canonical_dst_blend_mode,
    );
    // #renderlayer — derive the per-entity content-class layer.
    // Base layer comes from the REFR's record type
    // (`stat.record_type.render_layer()`); two per-mesh signals then
    // escalate it, so a coplanar overlay wins its z-fight against the
    // surface beneath:
    //   - `mesh.material.is_decal` (NIF-flagged decals — blood splats,
    //     scorch marks) → `RenderLayer::Decal`, the strongest bias;
    //   - `mesh.material.alpha_test` (alpha-tested rugs / posters / fences /
    //     cutout foliage) → `RenderLayer::Clutter`, a gentle bias, and only
    //     when the base was Architecture.
    // Architecture (zero bias) is the safe default for the rare "neither
    // base nor mesh hints an overlay" path.
    //
    // #2446 (MAT-D3-04) — this comment used to name `alpha_test_func != 0`
    // as the second signal and `Decal` as its output. Both wrong, and the
    // field half is the one that matters: `alpha_test_func` defaults to
    // `6` (GREATEREQUAL) on every imported material whether or not testing
    // is on, so gating on it would escalate every architectural mesh in the
    // cell — the exact bug `render_layer_with_decal_escalation`'s doc and
    // its `alpha_test_disabled_does_not_escalate_regardless_of_default_func`
    // test exist to keep out.
    //
    // Pre-#renderlayer this site also inserted a `Decal` marker
    // component when `mesh.material.is_decal` — that marker is retired now
    // that `RenderLayer::Decal` carries the same signal end-to-end.
    {
        use byroredux_core::ecs::components::{
            escalate_small_static_to_clutter, render_layer_with_decal_escalation,
        };
        // Small-STAT escalation runs first so decorative clutter
        // authored as STAT (paper piles, folders, clipboards on
        // desks — Bethesda's record-type classifier can't tell
        // these from architectural STATs without spatial extent)
        // gets the Clutter bias before the decal gate sees it.
        // Decal escalation still wins for alpha-tested overlays
        // and NIF-flagged decals regardless of size.
        //
        // The post-escalation layer is the RENDER-z-bias signal
        // (`RenderLayer` ECS component). #1294 moved the collision
        // trimesh-fallback gate below off this post-escalation
        // layer onto `base_layer` so SF sub-decomposed architecture
        // (per-LOD per-material sub-meshes < 50 units each, but
        // composing into a 1000-unit wall) doesn't get its
        // collider stripped on a render-side optimization.
        let layer =
            escalate_small_static_to_clutter(base_layer, mesh.local_bound_radius * ref_scale);
        let layer = render_layer_with_decal_escalation(
            layer,
            source_material.is_decal,
            source_material.alpha_test,
        );
        world.insert(entity, layer);
    }

    // F3 (2026-05-27) — synthesize a static TriMesh collider from
    // the render geometry when the NIF authored NO bhk collision.
    // This is the FO4+ case: those games moved static architecture
    // collision into the Havok content-system blob
    // (`bhkNPCollisionObject` → `bhkPhysicsSystem`), which our
    // `extract_collision` doesn't deserialize yet (a multi-day
    // project — see docs/audits/FALLOUT_SYMPTOMS F3). Without any
    // static collider the M28.5 character controller has nothing
    // to ground against and the player falls through the floor.
    //
    // The render mesh is a coarse but serviceable stand-in for the
    // authored collision hull on structural architecture (floors,
    // walls, ramps). Gated tightly so we don't turn clutter, decals,
    // or skinned actors into expensive trimesh colliders:
    //   - `collisions_empty` — the NIF gave us no bhk shape, so
    //     we're not double-covering FNV/FO3/Skyrim (which parse bhk).
    //   - `RenderLayer::Architecture` — structural only; clutter and
    //     decals are escalated away from this layer above.
    //   - `!mesh.skinned` — never synthesize for animated bodies.
    //   - `!mesh.material.is_decal && !mesh.material.alpha_test` — skip overlay planes.
    //   - ≥ 1 triangle of geometry.
    // Scale: the physics sync places bodies by GlobalTransform
    // translation+rotation only (it ignores scale — bhk shapes bake
    // havok_scale into their verts at extract time). So we bake the
    // composed `final_scale` into the trimesh verts here to match
    // the rendered geometry.
    // #1294 — gate on `base_layer` (pre-escalation REFR record-type
    // classification), NOT `final_layer` (post-escalation render
    // layer). The small-STAT-to-Clutter escalation
    // (`escalate_small_static_to_clutter`) is a RENDER z-bias
    // optimization that demotes architecturally-classified meshes
    // with a small bounding-sphere radius (< 50 units) to the
    // Clutter render layer so decorative STATs (papers / folders
    // / clipboards) win the coplanar z-fight against desks. It was
    // never intended to gate collision generation.
    //
    // For Starfield content the gate-on-`final_layer` site rejected
    // every wall / floor / ramp on Cydonia because SF NIFs are
    // heavily decomposed into per-material per-LOD sub-meshes
    // (an industrial platform = 6 BSGeometry blocks each with 4
    // LOD slots = 24 sub-meshes), each individual sub-mesh smaller
    // than the 50-unit threshold. Even though the COMPOSITE REFR
    // is a giant wall, the per-mesh radius escalates to Clutter
    // and the trimesh fallback skips it → zero static colliders →
    // character free-falls indefinitely from frame 0 (`rapier_bodies=1`
    // diagnostic warn at `character.rs:290`).
    //
    // `base_layer` reflects the REFR's base record type
    // (STAT/MSTT/FURN/DOOR/… → Architecture; NPC_ → Actor; etc.).
    // That's the correct "should this be a static collider?" signal
    // — independent of per-mesh sub-decomposition. NPC actors (Actor
    // base) and small clutter (record-type-classified Clutter) both
    // skip the fallback as before; only the misclassified
    // sub-decomposed architecture changes behaviour.
    if collision_fallback == MissingCollisionFallback::ArchitectureTriMesh
        && mesh.skin.is_none()
        && !source_material.is_decal
        && !source_material.alpha_test
        && source_material.material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
        && mesh.positions.len() >= 3
        && mesh.indices.len() >= 3
    {
        // Shared with the exterior LAND path — see
        // `spawn_trimesh_collider_ghost`. The render `entity` keeps its
        // MeshHandle and enters BLAS+TLAS normally (RT shadows/GI on
        // FO4/Starfield architecture); the ghost is physics-only.
        let source_form = world
            .get::<FormIdComponent>(placement_root)
            .map(|form_id| form_id.0);
        *synthesized_collision_proxy |= spawn_trimesh_collider_ghost(
            world,
            &mesh.positions,
            &mesh.indices,
            final_pos,
            final_rot,
            final_scale,
            source_form,
        );
    }
    // Attach ESM light_data ONLY if the NIF didn't actually spawn
    // any lights (avoids duplicates) and only on the first mesh
    // (avoids N copies when a lamp NIF has multiple sub-meshes).
    //
    // Pre-#632 this gated on `nif_lights.is_empty()` — wrong
    // because zero-colour placeholders take a slot in the array
    // but get filtered out at the spawn loop above. Cells with
    // light-bulb meshes (Prospector Saloon) rendered dark even
    // though both the NIF placeholder and the ESM LIGH record
    // agreed there should be a light. Track real spawns instead.
    if let Some(ld) = light_data {
        if spawned_nif_lights == 0 && count == 0 {
            // Phase 18 (reverted in Phase 19.7) — the
            // flame-node offset spawn lived here, but the
            // substring-based pattern match
            // (`flame` / `fire` / `attachlight`) hit
            // false-positives on at least one Skyrim candle
            // NIF (upper shelf in Sleeping Giant Inn — visible
            // as "no light emitted at all" from that REFR's
            // light placement). Restoring the pre-Phase-18
            // attach-to-mesh-entity-at-ref_pos behaviour.
            //
            // The Phase 18 *capture* path stays — every cached
            // NIF still records `flame_attach_offset` at parse
            // time. A future re-enable with tighter pattern
            // matching (e.g. `^Flame[0-9]+$` regex, or
            // requiring an `AttachFire` block specifically)
            // can consume the captured offset without
            // re-walking the NIF.
            let _ = cached.flame_attach_offset;

            world.insert(
                entity,
                LightSource::from_legacy_world_units(
                    light_radius_or_default(ld.radius),
                    ld.color,
                    ld.flags,
                    // REN-D10-2026-09-20-01 (#4514) — the canonicalized
                    // lane, NOT the raw LIGH record field: pre-Skyrim
                    // 32-byte LIGH carries the 0.0 "field absent" sentinel,
                    // and the raw value resolved k=1.0 through `Emitter`'s
                    // non-ESM last-resort net instead of the quadratic
                    // k=2.0.
                    light_falloff_exponent,
                    light_kind,
                    light_direction,
                    light_outer_angle,
                    light_shadow_flags,
                ),
            );
            // Phase 17 — animation companion at the placement root,
            // same position as the mesh entity. The caller has already
            // decoded source-game LIGH flags into shared behavior.
            attach_light_flicker_if_needed(world, entity, ld, ref_pos, light_animation_flags);
        }
    }
    true
}

/// Build the target-major GPU buffer without compacting source morph indices.
/// Filtered/malformed targets leave an all-zero slot so an animation weight
/// resolved against `NiMorphData.morphs[i]` still deforms target `i` (#3233).
fn flatten_morph_targets(
    targets: &[byroredux_nif::import::ImportedMorphTarget],
    vertex_count: usize,
) -> (Vec<[f32; 4]>, u32) {
    let target_count = targets
        .iter()
        .map(|target| target.original_index + 1)
        .max()
        .unwrap_or(0);
    let mut deltas = vec![[0.0; 4]; target_count as usize * vertex_count];
    for target in targets {
        let start = target.original_index as usize * vertex_count;
        for (dst, delta) in deltas[start..start + vertex_count]
            .iter_mut()
            .zip(&target.deltas)
        {
            *dst = [delta[0], delta[1], delta[2], 0.0];
        }
    }
    (deltas, target_count)
}

/// #3231 / #4399 — create the GPU morph-target slot for one spawned skinned
/// mesh entity. v1-scoped to entities that carry a canonical `SkinnedMesh`:
/// the draw-time `GpuInstance` lookup in `context/draw.rs` and the
/// `skin_vertices.comp` dispatch in `skinned_blas_refit.rs` both gate on
/// `bone_offset != 0`, which only a `SkinnedMesh` produces — so callers must
/// invoke this ONLY where a `SkinnedMesh` was actually attached (the
/// loose-NIF / NPC path in `scene/nif_loader.rs`; the cell path cannot until
/// #2440). Pre-#4399 the cell loader also created slots behind the raw
/// `mesh.skin.is_some()` gate, which no read gate could ever reach. Created
/// once at spawn (not lazily per-frame like `SkinSlot`) because morph delta
/// data is only known at NIF-import/mesh-spawn time — see `MorphSlot`'s own
/// doc comment.
pub(crate) fn try_spawn_morph_slot(
    ctx: &mut VulkanContext,
    entity: byroredux_core::ecs::EntityId,
    mesh: &byroredux_nif::import::ImportedMesh,
    mesh_handle: u32,
) {
    let Some(morph_targets) = mesh.morph_targets.as_ref().filter(|t| !t.is_empty()) else {
        return;
    };
    let vertex_count = mesh.positions.len() as u32;
    let (deltas, target_count) = flatten_morph_targets(morph_targets, mesh.positions.len());
    match ctx.create_morph_slot_for_mesh(mesh_handle, &deltas, target_count, vertex_count) {
        Ok(slot) => {
            ctx.morph_slots.insert(entity, slot);
        }
        Err(e) => {
            log::warn!(
                "Failed to create MorphSlot for entity {entity} ({:?}): {e:#}",
                mesh.name,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use byroredux_core::ecs::World;
    use byroredux_core::string::StringPool;
    use byroredux_nif::import::{ImportedMesh, ImportedMorphTarget, TextureSlotLayout};
    use std::sync::Arc;

    /// The pre-#4290 entry point, kept for the tests that exercise path
    /// resolution without pre-merge snapshots.
    fn resolve_mesh_paths(
        world: &mut World,
        imported: &[byroredux_nif::import::ImportedMesh],
        refr_overlay: Option<&RefrTextureOverlay>,
        mat_provider: Option<&mut MaterialProvider>,
        tex_provider: Option<&crate::asset_provider::TextureProvider>,
    ) -> Vec<ResolvedMeshPaths> {
        resolve_mesh_paths_with_pre_merge(
            world,
            imported,
            &[],
            refr_overlay,
            mat_provider,
            tex_provider,
        )
    }

    fn empty_mesh() -> ImportedMesh {
        ImportedMesh::from_geometry(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn morph_gpu_buffer_preserves_filtered_source_index_holes() {
        let targets = vec![
            ImportedMorphTarget {
                original_index: 0,
                name: Some(Arc::from("first")),
                deltas: vec![[1.0, 2.0, 3.0]],
            },
            ImportedMorphTarget {
                original_index: 2,
                name: Some(Arc::from("after malformed")),
                deltas: vec![[4.0, 5.0, 6.0]],
            },
        ];

        let (deltas, target_count) = flatten_morph_targets(&targets, 1);

        assert_eq!(target_count, 3);
        assert_eq!(deltas[0], [1.0, 2.0, 3.0, 0.0]);
        assert_eq!(
            deltas[1], [0.0; 4],
            "filtered source index must remain inert"
        );
        assert_eq!(deltas[2], [4.0, 5.0, 6.0, 0.0]);
    }

    #[test]
    fn morph_spawn_uses_mesh_handle_shared_delta_cache() {
        let source = include_str!("mesh_instance.rs");
        let production = &source[..source
            .find("\n#[cfg(test)]")
            .expect("mesh_instance.rs must retain its test module")];
        assert!(
            production.contains("ctx.create_morph_slot_for_mesh("),
            "spawn must route morph creation through the mesh-keyed cache"
        );
        assert!(
            !production.contains("MorphSlot::create("),
            "spawn must not upload a delta buffer once per entity"
        );
    }

    /// #4399 — the #3231 creation site and the `bone_offset != 0` read gate
    /// used to live on paths that never intersect: creation was gated on the
    /// raw `mesh.skin.is_some()` (cell loader, which attaches no
    /// `SkinnedMesh` — #2440), while the read gate requires the canonical
    /// `SkinnedMesh` only `scene/nif_loader.rs` builds. Driving either path
    /// live needs a `VulkanContext`, so reachability is pinned at source
    /// level — the same shape as `morph_spawn_uses_mesh_handle_shared_delta_cache`
    /// above:
    ///
    /// * creation exists exactly ONCE, in the shared `try_spawn_morph_slot`
    ///   helper, whose doc states the canonical gate;
    /// * the loose-NIF / NPC path calls that helper only when the skin
    ///   binding actually attached a `SkinnedMesh`;
    /// * the cell path no longer creates slots (the raw `mesh.skin.is_some()`
    ///   creation gate is gone from this file's production half).
    #[test]
    fn morph_slot_creation_is_reachable_end_to_end() {
        let here = include_str!("mesh_instance.rs");
        let production = &here[..here
            .find("\n#[cfg(test)]")
            .expect("mesh_instance.rs must retain its test module")];
        let nif_loader = include_str!("../../scene/nif_loader.rs");

        assert_eq!(
            production.matches("create_morph_slot_for_mesh(").count(),
            1,
            "creation must live only in the shared try_spawn_morph_slot helper"
        );
        assert!(
            production.contains("pub(crate) fn try_spawn_morph_slot("),
            "the helper must stay crate-visible for the loose-NIF path to call"
        );
        assert!(
            nif_loader.contains("if skin_attached {\n")
                && nif_loader.contains("try_spawn_morph_slot(ctx, entity, mesh, mesh_handle);"),
            "the loose-NIF path must create the slot exactly where its \
             SkinnedMesh was attached (#4399)"
        );
        assert!(
            !production.contains("if mesh.skin.is_some() {"),
            "the cell path must not create slots behind the raw skin gate — \
             it attaches no SkinnedMesh (#2440), so those slots are read by \
             no draw (#4399)"
        );
    }

    /// REN-D10-2026-09-20-01 (#4514) — all three LIGH (ESM) spawn sites of
    /// `LightSource::from_legacy_world_units` must consume the
    /// `canonical_light_falloff_exponent` lane, never the raw record field.
    /// The pre-Skyrim 32-byte LIGH layout carries the 0.0 "field absent"
    /// sentinel; 6b4e6252c canonicalized the two `synth_child.rs` branches
    /// but missed this file's ESM-light fallback (the dominant meshed-lamp
    /// path), which resolved k=1.0 where pre-Skyrim layouts mean k=2.0.
    /// Driving any site live needs a `VulkanContext`, so the wiring is
    /// pinned at source level — the same shape as
    /// `morph_spawn_uses_mesh_handle_shared_delta_cache` above. The fourth
    /// `from_legacy_world_units` site (NIF-authored lights in
    /// `spawn_nif_lights`) is a non-ESM producer with no sentinel to
    /// resolve; `Emitter`'s own `1.0` net is its documented contract, so it
    /// is deliberately out of scope here.
    #[test]
    fn every_ligh_spawn_site_consumes_the_canonical_falloff_lane() {
        let here = include_str!("mesh_instance.rs");
        // Production-only: this test's own prose quotes the forbidden
        // literal, and the split at the test module keeps it out of the
        // scanned half.
        let production = &here[..here
            .find("\n#[cfg(test)]")
            .expect("mesh_instance.rs must retain its test module")];
        let spawn = include_str!("../spawn.rs");
        let synth = include_str!("../references/synth_child.rs");

        // Site 3 — this file's ESM-light fallback. The lane must arrive via
        // `PlacementCtx` (destructure + call argument) and the raw field
        // must not be read anywhere in this file's production half.
        assert_eq!(
            production.matches("light_falloff_exponent,").count(),
            2,
            "the canonicalized falloff lane must be destructured from \
             PlacementCtx and passed to from_legacy_world_units (#4514)"
        );
        assert!(
            !production.contains("ld.falloff_exponent"),
            "the ESM-light fallback must consume the canonicalized lane, \
             not the raw LIGH record field (#4514)"
        );

        // The lane is threaded through `spawn_placed_instances` — one
        // parameter in the signature, one forward into `PlacementCtx`.
        assert!(
            spawn.contains("light_falloff_exponent: f32,")
                && spawn.matches("light_falloff_exponent,").count() == 1,
            "spawn_placed_instances must thread the falloff lane from its \
             caller into PlacementCtx (#4514)"
        );
        assert!(
            !spawn.contains("ld.falloff_exponent"),
            "spawn_placed_instances must not read the raw LIGH record field (#4514)"
        );

        // Sites 1 + 2 — the LIGH-only and fxlight branches in synth_child,
        // plus the lane producer feeding `spawn_placed_instances`: three
        // canonicalizer calls for two direct sites and one lane.
        assert_eq!(
            synth
                .matches("canonical_light_falloff_exponent(game, ld.falloff_exponent)")
                .count(),
            3,
            "both synth_child from_legacy_world_units sites and the \
             spawn_placed_instances lane must route through the \
             canonicalizer (#4514)"
        );
        assert_eq!(
            synth.matches("LightSource::from_legacy_world_units(").count(),
            2,
            "the synth_child site census drifted — re-classify any new \
             ESM-light spawn site before extending this guard (#4514)"
        );
        assert_eq!(
            spawn.matches("LightSource::from_legacy_world_units(").count(),
            1,
            "spawn.rs must keep exactly the non-ESM NIF-light site (#4514)"
        );
        assert_eq!(
            production
                .matches("LightSource::from_legacy_world_units(")
                .count(),
            1,
            "mesh_instance.rs must keep exactly the ESM-fallback site (#4514)"
        );
    }

    /// REN-6-2026-09-20-02 (#4529) — both static spawn paths must build the
    /// `MaterialTextureHandles` component through the one shared producer,
    /// and the channel-presence derivation must live only in that producer.
    /// The duplicated resolve → normal_has_alpha/tint_has_alpha → insert
    /// block that used to sit in this file and `scene/nif_loader.rs` is the
    /// #2444/#2300 duplicate-construction class: a presence lane added to
    /// one copy and not the other lands only on NIF-loaded or only on
    /// REFR-overlaid meshes. Driving either path live needs a
    /// `VulkanContext`, so the wiring is pinned at source level — the same
    /// shape as `morph_spawn_uses_mesh_handle_shared_delta_cache` above.
    #[test]
    fn both_spawn_paths_build_material_texture_handles_through_one_producer() {
        let here = include_str!("mesh_instance.rs");
        // Production-only: this test's own source quotes the scanned
        // literals, and the split at the test module keeps them out of the
        // scanned half.
        let production = &here[..here
            .find("\n#[cfg(test)]")
            .expect("mesh_instance.rs must retain its test module")];
        let nif_loader = include_str!("../../scene/nif_loader.rs");
        let producer = include_str!("../../asset_provider/texture.rs");

        // Each site constructs through the producer exactly once —
        // `build_material_texture_handles(` can only match the call, not
        // either file's import line.
        assert_eq!(
            production.matches("build_material_texture_handles(").count(),
            1,
            "the cell-loader spawn path must construct MaterialTextureHandles \
             through the shared producer (#4529)"
        );
        assert_eq!(
            nif_loader.matches("build_material_texture_handles(").count(),
            1,
            "the loose-NIF spawn path must construct MaterialTextureHandles \
             through the shared producer (#4529)"
        );
        // …and neither retains an inline presence derivation.
        assert!(
            !production.contains("handle_has_alpha(") && !nif_loader.contains("handle_has_alpha("),
            "channel-presence flags are derived only inside \
             build_material_texture_handles; an inline derivation at a spawn \
             site is the divergence #4529 closed"
        );

        // The producer keeps owning both presence lanes, derived from the
        // handles it resolved itself.
        assert!(
            producer.contains("handle_has_alpha(texture_handles.normal)")
                && producer.contains("handle_has_alpha(texture_handles.tint)"),
            "the shared producer must derive normal + tint presence from the \
             handles it resolved (#4529)"
        );
    }

    /// #3596 — the Oblivion `APPLY_HILIGHT2` parallax route only becomes
    /// reachable here.
    ///
    /// Oblivion does not put normal maps in NIF texture slots at all; it
    /// resolves them by the `<base>_n.dds` filename convention (#1303),
    /// which happens at THIS boundary, downstream of `MaterialInfo`. #3530
    /// gated its binding on `info.normal_map` being `Some`, so it never
    /// fired: measured over the two vanilla mesh archives, 0 of 1,430
    /// `APPLY_HILIGHT2` properties carry a normal or bump slot, and
    /// `parallax_height_in_alpha` was true on 0 of 35,322 imported meshes.
    ///
    /// The importer now records the rule; the derived normal is bound into
    /// the height slot here.
    #[test]
    fn oblivion_apply_hilight2_binds_the_derived_normal_as_its_height_map() {
        let mut pool = StringPool::new();
        let diffuse = pool.intern(r"textures\dungeons\caves\crm01.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        // The vanilla shape: a diffuse slot, no normal slot, no height slot,
        // and the parser's `APPLY_HILIGHT2` decision recorded on the material.
        let mut mesh = empty_mesh();
        mesh.material.textures.base_color = Some(diffuse);
        mesh.material.parallax_height_in_alpha = true;

        let resolved = resolve_mesh_paths(&mut world, &[mesh], None, None, None);
        assert_eq!(
            resolved[0].textures.normal.as_deref(),
            Some(r"textures\dungeons\caves\crm01_n.dds"),
            "the `_n.dds` convention still derives the normal"
        );
        assert_eq!(
            resolved[0].textures.height.as_deref(),
            Some(r"textures\dungeons\caves\crm01_n.dds"),
            "and the height slot must bind that same derived texture — \
             Oblivion ships no `_p.dds`, so the height lives in its alpha"
        );
        assert_eq!(
            resolved[0].sources.height,
            MaterialTextureSource::DerivedNormal,
            "provenance must say the height path was synthesized, not authored"
        );
    }

    /// The flag is the only trigger: a material without it must not acquire
    /// a height binding from the derived normal, however the normal arrived.
    #[test]
    fn a_derived_normal_alone_does_not_bind_a_height_map() {
        let mut pool = StringPool::new();
        let diffuse = pool.intern(r"textures\clutter\barrel01.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut mesh = empty_mesh();
        mesh.material.textures.base_color = Some(diffuse);

        let resolved = resolve_mesh_paths(&mut world, &[mesh], None, None, None);
        assert_eq!(
            resolved[0].textures.normal.as_deref(),
            Some(r"textures\clutter\barrel01_n.dds")
        );
        assert_eq!(
            resolved[0].textures.height, None,
            "only the APPLY_HILIGHT2 decision may synthesize a height binding"
        );
    }

    #[test]
    fn xtxr_slot_six_reaches_skyrim_inner_layer_consumer() {
        let mut pool = StringPool::new();
        let base = pool.intern(r"textures\ice\base_inner.dds");
        let replacement = pool.intern(r"textures\ice\override_inner.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut mesh = empty_mesh();
        mesh.material.texture_slot_layout = TextureSlotLayout::Skyrim;
        mesh.material.shader_type = 11; // MultiLayerParallax
        mesh.material.textures.inner_layer = Some(base);
        let overlay = RefrTextureOverlay {
            inner: Some(replacement),
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].textures.inner_layer.as_deref(),
            Some(r"textures\ice\override_inner.dds"),
            "the populated RefrTextureOverlay.inner field must reach its live consumer (#2713)"
        );
        assert_eq!(
            resolved[0].sources.inner_layer,
            MaterialTextureSource::TxstOverride
        );
    }

    #[test]
    fn xtxr_fo4_palette_and_specular_follow_the_fo4_table() {
        let mut pool = StringPool::new();
        let palette = pool.intern(r"textures\fo4\palette_lgrad.dds");
        let specular = pool.intern(r"textures\fo4\surface_s.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut mesh = empty_mesh();
        mesh.material.texture_slot_layout = TextureSlotLayout::Fallout4;
        let overlay = RefrTextureOverlay {
            height: Some(palette),
            specular: Some(specular),
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].textures.greyscale_lut.as_deref(),
            Some(r"textures\fo4\palette_lgrad.dds")
        );
        assert!(resolved[0].textures.height.is_none());
        // #4424 — FO4 slot 7 is the smooth-spec role (the same `_s.dds`
        // file BGSM names `smooth_spec_texture`), so the raw slot-7
        // override lands in `smooth_spec`, and the specular-colour lane
        // stays empty.
        assert_eq!(
            resolved[0].textures.smooth_spec.as_deref(),
            Some(r"textures\fo4\surface_s.dds"),
            "FO4 slot 7 must reach the smooth-spec lane without the MSN flag (#2998/#4424)"
        );
        assert!(resolved[0].textures.specular.is_none());
    }

    /// #3187 — an XTXR slot-5 swap on an FO4 tint-family shape (FaceTint /
    /// SkinTint / HairTint) must reach the wrinkle lane, not silently no-op.
    /// `apply_slot_swap` always lands slot 5 in the overlay's `env_mask`
    /// field (it has no shader-type context to know the role in advance);
    /// `slot_to_role` is what actually decides FO4 tint-family slot 5 is
    /// `Wrinkle`, not `EnvironmentMask`, and the `pick(5, o.env_mask,
    /// TextureRole::Wrinkle)` this test exercises is what lets that
    /// decision reach the resolved mesh.
    #[test]
    fn xtxr_fo4_tint_family_slot_five_reaches_wrinkle_not_environment_mask() {
        let mut pool = StringPool::new();
        let wrinkle = pool.intern(r"textures\fo4\headwrinkles_n.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut mesh = empty_mesh();
        mesh.material.texture_slot_layout = TextureSlotLayout::Fallout4;
        mesh.material.shader_type = 4; // bs_lighting::FACE_TINT — tint family
        let overlay = RefrTextureOverlay {
            env_mask: Some(wrinkle), // where apply_slot_swap always lands slot 5
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].textures.wrinkle.as_deref(),
            Some(r"textures\fo4\headwrinkles_n.dds"),
            "#3187: slot 5 on an FO4 tint-family shape must resolve to \
             Wrinkle, the role slot_to_role actually assigns it"
        );
        assert!(
            resolved[0].textures.environment_mask.is_none(),
            "#3187: the same slot-5 value must NOT also bind as \
             EnvironmentMask on a tint-family shape — exactly one pick \
             may accept it"
        );
    }

    /// Sibling of the test above on the OTHER side of the tint-family
    /// gate: a non-tint FO4 shape's slot 5 is ordinary `EnvironmentMask`
    /// (`slot_to_role`'s existing, unchanged behaviour), so the new
    /// `Wrinkle` pick added under #3187 must NOT also claim it.
    #[test]
    fn xtxr_fo4_non_tint_slot_five_stays_environment_mask() {
        let mut pool = StringPool::new();
        let mask = pool.intern(r"textures\fo4\surface_m.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut mesh = empty_mesh();
        mesh.material.texture_slot_layout = TextureSlotLayout::Fallout4;
        // Default shader_type (0) is not in the tint family.
        let overlay = RefrTextureOverlay {
            env_mask: Some(mask),
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].textures.environment_mask.as_deref(),
            Some(r"textures\fo4\surface_m.dds"),
            "#3187: a non-tint-family FO4 shape's slot 5 must still bind \
             as EnvironmentMask — the new Wrinkle pick must not regress it"
        );
        assert!(resolved[0].textures.wrinkle.is_none());
    }

    #[test]
    fn xtxr_fo76_slot_six_reaches_specular_not_inner_layer() {
        let mut pool = StringPool::new();
        let specular = pool.intern(r"textures\fo76\surface_s.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut mesh = empty_mesh();
        mesh.material.texture_slot_layout = TextureSlotLayout::Fallout76;
        let overlay = RefrTextureOverlay {
            inner: Some(specular),
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].textures.specular.as_deref(),
            Some(r"textures\fo76\surface_s.dds")
        );
        assert!(
            resolved[0].textures.inner_layer.is_none(),
            "FO76 slot 6 is measured specular, not Skyrim inner-layer (#3085)"
        );
    }

    /// #3732 (NIFAL-2026-08-30-D8-01) — a REFR TXST override on a Skyrim
    /// tint-family (FaceTint/SkinTint/HairTint) shape with soft/rim
    /// lighting set must reach BOTH roles slot 2 colocates: `Tint` (via
    /// `slot_to_role`) AND `LightingMask` (via `slot_to_colocated_role`,
    /// #3458). Pre-fix `pick` consulted only `slot_to_role`, so the
    /// `lighting_mask` arm could never match on this exact population —
    /// the override updated `tint` but left `lighting_mask` bound to the
    /// base mesh's original texture while the `SLSF2_Soft_Lighting` gate
    /// crossed regardless.
    #[test]
    fn xtxr_skyrim_tint_family_slot_two_reaches_both_colocated_roles() {
        let mut pool = StringPool::new();
        let base_tint = pool.intern(r"textures\actors\character\base_sk.dds");
        let override_tex = pool.intern(r"textures\actors\character\override_sk.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut mesh = empty_mesh();
        mesh.material.texture_slot_layout = TextureSlotLayout::Skyrim;
        mesh.material.shader_type = 5; // SkinTint
        mesh.material.soft_lighting = true; // SLSF2_Soft_Lighting gate set
        mesh.material.textures.tint = Some(base_tint);
        mesh.material.textures.lighting_mask = Some(base_tint);
        let overlay = RefrTextureOverlay {
            glow: Some(override_tex),
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].textures.tint.as_deref(),
            Some(r"textures\actors\character\override_sk.dds"),
            "the Tint role must still pick up the override"
        );
        assert_eq!(
            resolved[0].textures.lighting_mask.as_deref(),
            Some(r"textures\actors\character\override_sk.dds"),
            "the colocated LightingMask role must ALSO pick up the same override — \
             pre-fix this stayed bound to the base mesh's texture"
        );
    }

    /// #2594 — `lighting` / `flow` have no `BSShaderTextureSet` wire-slot
    /// analog (unlike every other case in this module), so they don't go
    /// through `slot_to_role` — a direct override, same shape as `wrinkle`.
    /// Pins that the overlay fields `fill_from_bgsm` populates actually
    /// reach `ImportedMesh`'s resolved texture set.
    #[test]
    fn overlay_lighting_and_flow_reach_the_resolved_mesh() {
        let mut pool = StringPool::new();
        let lighting = pool.intern(r"textures\fo4\surface_lighting.dds");
        let flow = pool.intern(r"textures\fo4\surface_flow.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mesh = empty_mesh();
        let overlay = RefrTextureOverlay {
            lighting: Some(lighting),
            flow: Some(flow),
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].textures.lighting.as_deref(),
            Some(r"textures\fo4\surface_lighting.dds")
        );
        assert_eq!(
            resolved[0].sources.lighting,
            MaterialTextureSource::TxstOverride
        );
        assert_eq!(
            resolved[0].textures.flow.as_deref(),
            Some(r"textures\fo4\surface_flow.dds")
        );
        assert_eq!(
            resolved[0].sources.flow,
            MaterialTextureSource::TxstOverride
        );
    }

    #[test]
    fn bgsm_smooth_spec_and_specular_remain_distinct_roles() {
        let mut pool = StringPool::new();
        let smooth_spec = pool.intern(r"textures\fo4\surface_smoothspec.dds");
        let specular = pool.intern(r"textures\fo4\surface_specular.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mesh = empty_mesh();
        let overlay = RefrTextureOverlay {
            smooth_spec: Some(smooth_spec),
            external_specular: Some(specular),
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].textures.smooth_spec.as_deref(),
            Some(r"textures\fo4\surface_smoothspec.dds")
        );
        assert_eq!(
            resolved[0].textures.specular.as_deref(),
            Some(r"textures\fo4\surface_specular.dds")
        );
    }

    /// When the overlay leaves `lighting`/`flow` empty, the mesh's own
    /// authored values must ride through unchanged (same as every other
    /// role) — this was already true pre-#2594 by construction (the
    /// initial `map_ref` copies every `MaterialTextureSet` field
    /// verbatim), but pinning it here documents the contract now that
    /// these two roles have a real overlay-side producer to fall through.
    #[test]
    fn mesh_lighting_and_flow_survive_when_overlay_has_none() {
        let mut pool = StringPool::new();
        let lighting = pool.intern(r"textures\mesh\lighting.dds");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut mesh = empty_mesh();
        mesh.material.textures.lighting = Some(lighting);

        let resolved = resolve_mesh_paths(&mut world, &[mesh], None, None, None);
        assert_eq!(
            resolved[0].textures.lighting.as_deref(),
            Some(r"textures\mesh\lighting.dds")
        );
        assert_eq!(
            resolved[0].sources.lighting,
            MaterialTextureSource::NifTextureSet
        );
        assert!(resolved[0].textures.flow.is_none());
    }

    fn swap_entry(source: &str, target: &str) -> esm::records::MaterialSwapEntry {
        esm::records::MaterialSwapEntry {
            source: source.to_string(),
            target: target.to_string(),
            color_intensity: None,
        }
    }

    /// #973 / FO4-D4-NEW-08-followup — a multi-shape mesh (e.g. Raider
    /// armour body + arm shapes) where NO XATO/XTNM overrides
    /// `overlay.material_path` (the common vanilla shape for a plain XMSP
    /// swap). Pre-fix only the overlay's own single `material_path` could
    /// ever swap, and it was `None` here — every shape's swap was silently
    /// dropped. Each shape must resolve its OWN authored `material_path`
    /// against `material_swaps` independently.
    #[test]
    fn mswp_swaps_apply_per_shape_not_just_the_overlay_material_path() {
        let mut pool = StringPool::new();
        let body_src = pool.intern(r"materials\armor\raider\body01.bgsm");
        let arm_src = pool.intern(r"materials\armor\raider\arm01.bgsm");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut body = empty_mesh();
        body.material.material_path = Some(body_src);
        let mut arm = empty_mesh();
        arm.material.material_path = Some(arm_src);

        let overlay = RefrTextureOverlay {
            material_swaps: vec![
                swap_entry(
                    r"materials\armor\raider\body01.bgsm",
                    r"materials\armor\raider\body01_variant04.bgsm",
                ),
                swap_entry(
                    r"materials\armor\raider\arm01.bgsm",
                    r"materials\armor\raider\arm01_variant04.bgsm",
                ),
            ],
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[body, arm], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].material_path.as_deref(),
            Some(r"materials\armor\raider\body01_variant04.bgsm"),
            "the body shape's own authored material must swap"
        );
        assert_eq!(
            resolved[1].material_path.as_deref(),
            Some(r"materials\armor\raider\arm01_variant04.bgsm"),
            "the arm shape's own authored material must ALSO swap — pre-fix this was left \
             on its NIF-authored BGSM because build_refr_texture_overlay only ever \
             substitutes one shared material_path"
        );
    }

    /// #4290 — an MSWP swap replaces the whole material. The cached mesh was
    /// merged with its SOURCE sidecar (two-sided, alpha-tested, emissive);
    /// the swap target authors none of that. Merging the target over the
    /// cached material would keep all three (the merge is fill-if-unset with
    /// one-way flags), so the spawn side must see the target merged onto the
    /// pre-merge snapshot.
    #[test]
    fn mswp_swap_merges_the_target_material_onto_the_pre_merge_snapshot() {
        use crate::asset_provider::MaterialProvider;
        use byroredux_bgsm::template::ResolvedMaterial;
        use byroredux_bgsm::{BaseMaterial, BgsmFile};

        let source_path = r"materials\tests\wall_source.bgsm";
        let target_path = r"materials\tests\wall_target.bgsm";
        let mut provider = MaterialProvider::new();
        let source_file = BgsmFile {
            base: BaseMaterial {
                two_sided: true,
                alpha_test: true,
                ..Default::default()
            },
            emit_enabled: true,
            emittance_mult: 4.0,
            ..Default::default()
        };
        provider.insert_bgsm_for_test(
            source_path,
            ResolvedMaterial {
                file: source_file,
                parent: None,
            },
        );
        let target_file = BgsmFile {
            // #4654 — the fixture authors a specular_mult it expects to be
            // forwarded, so the specular block must be authored ON (the
            // bool default false now means "disabled", which zeroes it).
            specular_enabled: true,
            specular_mult: 0.25,
            ..Default::default()
        };
        provider.insert_bgsm_for_test(
            target_path,
            ResolvedMaterial {
                file: target_file,
                parent: None,
            },
        );

        let mut pool = StringPool::new();
        let mut mesh = empty_mesh();
        mesh.material.material_path = Some(pool.intern(source_path));
        let pre_merge = crate::cell_loader::nif_import_registry::merge_external_materials(
            std::slice::from_mut(&mut mesh),
            &mut provider,
            &mut pool,
        );
        assert!(
            mesh.material.two_sided && mesh.material.alpha_test,
            "fixture: the cache-fill merge must have applied the source sidecar"
        );
        let mut world = World::new();
        world.insert_resource(pool);

        let overlay = RefrTextureOverlay {
            material_swaps: vec![swap_entry(source_path, target_path)],
            ..Default::default()
        };
        let resolved = resolve_mesh_paths_with_pre_merge(
            &mut world,
            std::slice::from_ref(&mesh),
            &pre_merge,
            Some(&overlay),
            Some(&mut provider),
            None,
        );
        let swapped = resolved[0]
            .swapped_material
            .as_ref()
            .expect("a swap that changed the material path must produce a swapped material");
        assert!(
            !swapped.two_sided,
            "the source sidecar's two-sided flag must not survive the swap"
        );
        assert!(
            !swapped.alpha_test,
            "the source sidecar's alpha test must not survive the swap"
        );
        assert_ne!(
            swapped.emissive_mult, 4.0,
            "the source sidecar's emittance must not survive the swap"
        );
        assert_eq!(
            swapped.specular_strength, 0.25,
            "the target sidecar's scalars must apply"
        );
        assert!(
            std::ptr::eq(resolved[0].material(&mesh), swapped),
            "spawn-side consumers must read the swapped material"
        );
    }

    /// #4400 — the roles `resolve_effective` does NOT cover (the BGEM v21+
    /// glass-overlay suite) ride the initial `textures` seed, which #4290
    /// left on the pre-swap cached `mesh.material`: a swapped BGEM kept the
    /// SOURCE sidecar's scratch/dirt overlay maps while `sources` (seeded
    /// from the swapped material) reported the TARGET's provenance. The
    /// seed must follow the swap like every covered role does.
    #[test]
    fn mswp_swap_seeds_the_glass_overlay_roles_from_the_swapped_material() {
        use crate::asset_provider::MaterialProvider;
        use byroredux_bgsm::{BaseMaterial, BgemFile};

        let source_path = r"materials\tests\glass_source.bgem";
        let target_path = r"materials\tests\glass_target.bgem";
        let mut provider = MaterialProvider::new();
        for (path, scratch, dirt) in [
            (
                source_path,
                r"textures\glass\source_scratch.dds",
                r"textures\glass\source_dirt.dds",
            ),
            (
                target_path,
                r"textures\glass\target_scratch.dds",
                r"textures\glass\target_dirt.dds",
            ),
        ] {
            provider.insert_bgem_for_test(
                path,
                BgemFile {
                    base: BaseMaterial::default(),
                    glass_enabled: true,
                    glass_roughness_scratch: scratch.to_string(),
                    glass_dirt_overlay: dirt.to_string(),
                    ..Default::default()
                },
            );
        }

        let mut pool = StringPool::new();
        let mut mesh = empty_mesh();
        mesh.material.material_path = Some(pool.intern(source_path));
        let pre_merge = crate::cell_loader::nif_import_registry::merge_external_materials(
            std::slice::from_mut(&mut mesh),
            &mut provider,
            &mut pool,
        );
        assert_eq!(
            mesh.material.textures.glass_roughness_scratch,
            Some(pool.intern(r"textures\glass\source_scratch.dds")),
            "fixture: the cache-fill merge must have applied the source sidecar's overlays"
        );
        let mut world = World::new();
        world.insert_resource(pool);

        let overlay = RefrTextureOverlay {
            material_swaps: vec![swap_entry(source_path, target_path)],
            ..Default::default()
        };
        let resolved = resolve_mesh_paths_with_pre_merge(
            &mut world,
            std::slice::from_ref(&mesh),
            &pre_merge,
            Some(&overlay),
            Some(&mut provider),
            None,
        );
        assert_eq!(
            resolved[0].textures.glass_roughness_scratch,
            Some(r"textures\glass\target_scratch.dds".to_string()),
            "the glass scratch overlay must follow the swap target, not ride the \
             pre-swap cached material (#4400)"
        );
        assert_eq!(
            resolved[0].textures.glass_dirt_overlay,
            Some(r"textures\glass\target_dirt.dds".to_string()),
            "same for the dirt overlay (#4400)"
        );
    }

    /// No swap, no re-merge: the shape keeps the cached material and pays no
    /// clone.
    #[test]
    fn a_shape_without_a_swap_reads_its_cached_material() {
        let mut pool = StringPool::new();
        let mut mesh = empty_mesh();
        mesh.material.material_path = Some(pool.intern(r"materials\tests\plain.bgsm"));
        let mut world = World::new();
        world.insert_resource(pool);

        let resolved = resolve_mesh_paths_with_pre_merge(
            &mut world,
            std::slice::from_ref(&mesh),
            &[Some(mesh.material.clone())],
            None,
            None,
            None,
        );
        assert!(resolved[0].swapped_material.is_none());
        assert!(std::ptr::eq(resolved[0].material(&mesh), &mesh.material));
    }

    /// The FNAM path-prefix filter on an MSWP must be re-evaluated against
    /// EACH shape's own source path, not the overlay's shared one — a
    /// shape outside the filtered prefix keeps its authored material even
    /// though the REFR carries a matching swap table.
    #[test]
    fn mswp_filter_is_re_evaluated_per_shape() {
        let mut pool = StringPool::new();
        let filter = pool.intern(r"materials\armor\raider\");
        let in_scope = pool.intern(r"materials\armor\raider\body01.bgsm");
        let out_of_scope = pool.intern(r"materials\clutter\crate01.bgsm");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut body = empty_mesh();
        body.material.material_path = Some(in_scope);
        let mut crate_mesh = empty_mesh();
        crate_mesh.material.material_path = Some(out_of_scope);

        let overlay = RefrTextureOverlay {
            material_swaps_filter: Some(filter),
            material_swaps: vec![
                swap_entry(
                    r"materials\armor\raider\body01.bgsm",
                    r"materials\armor\raider\body01_variant04.bgsm",
                ),
                // Would match verbatim if the filter weren't re-checked
                // per shape — the filter must block it.
                swap_entry(
                    r"materials\clutter\crate01.bgsm",
                    r"materials\clutter\crate01_damaged.bgsm",
                ),
            ],
            ..Default::default()
        };

        let resolved =
            resolve_mesh_paths(&mut world, &[body, crate_mesh], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].material_path.as_deref(),
            Some(r"materials\armor\raider\body01_variant04.bgsm"),
            "in-prefix shape swaps"
        );
        assert_eq!(
            resolved[1].material_path.as_deref(),
            Some(r"materials\clutter\crate01.bgsm"),
            "out-of-prefix shape must keep its authored material unchanged"
        );
    }

    /// #3242 — two `material_swaps` entries sharing the same `source` (a
    /// duplicate BNAM→SNAM pair, legal in the MSWP format) must resolve
    /// to the LAST entry, matching the format's documented later-wins
    /// semantics (`refr.rs`'s own comment: "the spawn path applies them
    /// per shape with later-wins semantics matching the MSWP file
    /// format"). Pre-fix, comparing against the running `swapped`
    /// output instead of the fixed original source meant only the FIRST
    /// matching entry ever fired — the reverse of later-wins.
    #[test]
    fn mswp_duplicate_source_entries_resolve_to_the_last_one() {
        let mut pool = StringPool::new();
        let body_src = pool.intern(r"materials\armor\raider\body01.bgsm");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut body = empty_mesh();
        body.material.material_path = Some(body_src);

        let overlay = RefrTextureOverlay {
            material_swaps: vec![
                swap_entry(
                    r"materials\armor\raider\body01.bgsm",
                    r"materials\armor\raider\body01_variant02.bgsm",
                ),
                // Same source as above — later entry must win.
                swap_entry(
                    r"materials\armor\raider\body01.bgsm",
                    r"materials\armor\raider\body01_variant04.bgsm",
                ),
            ],
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[body], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].material_path.as_deref(),
            Some(r"materials\armor\raider\body01_variant04.bgsm"),
            "the LAST matching entry for a duplicate source must win, not the first"
        );
    }

    /// #3242 — companion regression: an entry's `target` incidentally
    /// equal to a LATER entry's `source` (a string collision, not an
    /// authored duplicate) must NOT chain (A→B→C). Only entries whose
    /// `source` matches the shape's ORIGINAL authored material can ever
    /// fire.
    #[test]
    fn mswp_incidental_target_source_collision_does_not_chain() {
        let mut pool = StringPool::new();
        let body_src = pool.intern(r"materials\armor\raider\body01.bgsm");
        let mut world = World::new();
        world.insert_resource(pool);

        let mut body = empty_mesh();
        body.material.material_path = Some(body_src);

        let overlay = RefrTextureOverlay {
            material_swaps: vec![
                // body01 -> variant02
                swap_entry(
                    r"materials\armor\raider\body01.bgsm",
                    r"materials\armor\raider\body01_variant02.bgsm",
                ),
                // Incidental collision: this entry's `source` equals the
                // PREVIOUS entry's `target`, not the shape's own authored
                // material. Must never fire for this shape.
                swap_entry(
                    r"materials\armor\raider\body01_variant02.bgsm",
                    r"materials\armor\raider\body01_variant99_should_not_apply.bgsm",
                ),
            ],
            ..Default::default()
        };

        let resolved = resolve_mesh_paths(&mut world, &[body], Some(&overlay), None, None);
        assert_eq!(
            resolved[0].material_path.as_deref(),
            Some(r"materials\armor\raider\body01_variant02.bgsm"),
            "an incidental target/source string collision must not chain \
             into a swap the shape's own authored material never matched"
        );
    }
}
