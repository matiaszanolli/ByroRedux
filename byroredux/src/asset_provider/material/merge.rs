//! The NIFAL sidecar merge boundary (#3857, split from
//! `asset_provider/material.rs`).
//!
//! `merge_external_material` is the ONE place a BGSM/BGEM/CDB sidecar may
//! touch a `&mut ImportedMaterial`, and #2412 closed asking that the
//! invariant not be weakened by a split. It is preserved literally: this
//! module has exactly one public entry point, and the per-kind arms below
//! are private siblings that only it calls.

use super::*;

use super::cdb::apply_cdb_pbr_fallback;
use byroredux_nif::import::{ImportedMaterial, ImportedTextureSource, MaterialTextureSet};

/// What [`merge_external_material`] actually did, for the caller and for
/// diagnostics.
///
/// #2709 (SF-D9-03) — replaces a bare `bool` whose doc claimed it "flips
/// to `true` on any merged field". That was false for the Starfield `.mat`
/// arm, which returns success having set only `is_pbr` and forwarded no
/// texture, scalar, or alpha state at all. The two cases are visually
/// identical downstream (a mesh with engine defaults), so collapsing them
/// into one `true` left no signal anywhere distinguishing "this cell's
/// materials resolved" from "this cell's materials resolved to nothing" —
/// precisely the state the dominant Starfield population is in, and the
/// reason a total material blackout produces no actionable diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeOutcome {
    /// No `material_path`, an unresolvable pool symbol, a path in no
    /// loaded archive, a parse failure, or an unrecognised extension.
    /// Nothing was written; the mesh keeps its NIF-derived material.
    Unresolved,
    /// The sidecar was recognised and confirmed present, but the merge
    /// forwarded no authored field — only a routing flag. Today this is
    /// exactly the Starfield `.mat` arm: the CDB-presence gate flips
    /// `is_pbr` so the mesh takes the Disney lobe, and per-field
    /// extraction from the Component Database is the deferred Phase 2
    /// (#1289 / #2359). A caller counting "materials that actually
    /// supplied data" must NOT count this.
    PresenceOnly,
    /// At least one authored field (texture slot, scalar, alpha/blend
    /// state, or shader flag) was forwarded onto the `ImportedMaterial`.
    Merged,
}

// Consumed by the merge regression tests today; no production caller
// reads the outcome yet (all four discard it explicitly — see their
// `let _ =` sites). Kept as the type's API rather than inlined into the
// tests so the deferred per-cell "materials resolved / of which
// presence-only" telemetry sink #2709 asks for has something to call,
// and so a future reader doesn't reach for `== MergeOutcome::Merged`
// spelled out at each site.
#[cfg_attr(not(test), allow(dead_code))]
impl MergeOutcome {
    /// True when the sidecar resolved at all, whether or not it supplied
    /// any authored field. This is the old `bool` return's meaning —
    /// use it only where "did the path resolve" is genuinely the
    /// question, not as a proxy for "did the mesh get material data".
    pub(crate) fn resolved(self) -> bool {
        !matches!(self, MergeOutcome::Unresolved)
    }

    /// True only when authored data actually landed on the material.
    pub(crate) fn merged(self) -> bool {
        matches!(self, MergeOutcome::Merged)
    }
}

/// Mark only the texture roles an external material actually filled. Inline
/// NIF values win the merge and retain `NifTextureSet`; this before/after
/// comparison records that precedence instead of guessing from the final
/// material path in `mat.dump`.
///
/// #3903 — this was the fifth hand-written role walk in the codebase and the
/// only one left unguarded: it listed all 22 canonical roles by name, so a
/// 23rd compiled clean and silently dropped out of provenance reporting. Its
/// three siblings were given `map_ref`/`roles()`/`values()` cross-check tests
/// by #3349, #3734 and #2697.
///
/// Rather than add a fourth such test, the hand-list is gone: both passes go
/// through [`MaterialTextureSet::zip_map_ref`], which builds the result as a
/// struct literal and therefore touches every field *by construction*. A new
/// role is now a compile error in `zip_map_ref` itself — one place, enforced
/// by the type system rather than by a test that has to be remembered. The
/// `decals` array rides along through `zip_map_ref`'s `from_fn`, which is
/// where the old loop's separate index walk lived.
fn record_external_texture_sources(
    material: &mut ImportedMaterial,
    before: &MaterialTextureSet<Option<byroredux_core::string::FixedString>>,
    source: ImportedTextureSource,
) {
    // A role counts as externally sourced only if the merge is what filled
    // it: empty before, populated after. A slot the NIF already carried keeps
    // its existing `NifTextureSet` provenance.
    let newly_filled = before.zip_map_ref(&material.textures, |was, now| {
        was.is_none() && now.is_some()
    });
    material.texture_sources =
        newly_filled.zip_map_ref(&material.texture_sources, |filled, existing| {
            if *filled {
                source
            } else {
                *existing
            }
        });
}

// #3857 — hoisted out of `merge_external_material` when the two arms
// were extracted; all three call it.
// `fill` populates an `Option<FixedString>` slot only when it's
// None and the incoming value is non-empty. Routes through the
// engine's `StringPool` so the BGSM/BGEM-resolved paths share the
// same intern table as the NIF-side paths (#609 / D6-NEW-01).
fn fill(
    slot: &mut Option<byroredux_core::string::FixedString>,
    value: &str,
    touched: &mut bool,
    pool: &mut byroredux_core::string::StringPool,
) {
    if slot.is_none() && !value.is_empty() {
        *slot = Some(pool.intern(value));
        *touched = true;
    }
}

/// Merge a BGSM, BGEM, or Starfield `.mat` sidecar into the
/// source-normalized NIF material payload.
///
/// NIF fields take precedence — only empty slots are filled from the
/// resolved material chain, matching Bethesda's runtime behaviour where
/// the shader property overrides template defaults per-material. For BGSM
/// the template chain is walked child-first (first non-empty value for a
/// given field wins); BGEM has no inheritance (the format carries no
/// `root_material_path`) so the single parsed file is read.
///
/// This boundary deliberately accepts [`ImportedMaterial`] rather than an
/// [`byroredux_nif::import::ImportedMesh`]: external formats can patch material
/// semantics, but cannot mutate geometry, transforms, skinning, or scene
/// ownership.
///
/// See [`MergeOutcome`] for what the return value distinguishes and why
/// it is not a `bool` (#2709 / SF-D9-03).
#[must_use = "a PresenceOnly merge resolved the sidecar but forwarded no authored \
              field — discarding the outcome erases the only signal distinguishing \
              it from a fully-populated merge (#2709)"]

pub(crate) fn merge_external_material(
    material: &mut ImportedMaterial,
    provider: &mut MaterialProvider,
    pool: &mut byroredux_core::string::StringPool,
) -> MergeOutcome {
    let textures_before = material.textures;
    let Some(path_sym) = material.material_path else {
        return MergeOutcome::Unresolved;
    };
    // `StringPool::resolve` returns the canonical lowercased form, so
    // we own the string for the BGSM dispatch + suffix matching here
    // without an extra `to_ascii_lowercase` allocation. See #609.
    let path: String = match pool.resolve(path_sym) {
        Some(s) => s.to_string(),
        None => return MergeOutcome::Unresolved,
    };

    // `touched` flips to `true` on any merged AUTHORED field — it is what
    // separates `MergeOutcome::Merged` from `PresenceOnly` at the returns
    // below, so it must NOT be set by a routing-only flag flip. Allowed
    // unused assignment: the BGSM / BGEM success branches set it
    // unconditionally alongside `material.from_bgsm = true`, so the
    // `false` initializer is overwritten before any read there — but the
    // initializer is load-bearing for the failure / unknown-kind path.
    #[allow(unused_assignments)]
    let mut touched = false;

    // #1289 / SF-D3-NEW-01 — Starfield `.mat` arm. Starfield material
    // paths captured by the NIF stopcond (`crates/nif/src/blocks/
    // shader.rs::is_material_path`) end in `.mat`. The actual material
    // data lives in the binary Component Database at
    // `materials\materialsbeta.cdb` inside `Starfield - Materials.ba2`,
    // discovered once at provider init by `discover_starfield_cdbs`, which
    // header-probes each payload through `probe_starfield_cdb`.
    //
    // Phase 1 (this commit): flip `material.is_pbr = true` so
    // `pack_imported_material_flags` packs `MAT_FLAG_PBR_BSDF` and
    // `triangle.frag` routes Starfield content through the Disney BSDF
    // path instead of the legacy Lambert + simple-GGX path (the audit
    // FAIL closure). Defaults for metalness / roughness / textures
    // stay at the NIF-derived values — better than Lambert but still
    // approximate; Phase 2 will walk the CDB to extract authored values.
    //
    // The CDB-presence check prevents accidental PBR routing for modded
    // sidecars against non-Starfield archives. Starfield's shipped NIFs use
    // `.bgsm`/`.bgem`-named references even though no such files exist in its
    // archives (#3053), so those names are in scope for it too — but as a
    // *fallback* reached on a resolve miss, not as a gate that pre-empts the
    // resolvers (#3230). Only `.mat` short-circuits.
    let starfield_named_material =
        path.ends_with(".mat") || path.ends_with(".bgsm") || path.ends_with(".bgem");
    let starfield_cdb_gate = starfield_named_material && provider.has_starfield_cdb();

    // `.mat` short-circuits: vanilla Starfield ships no `.mat`/`.bgsm`/
    // `.bgem` sidecars, but an installed Creation/mod archive can — 20 JSON
    // `.mat` exports measured across 129 installed archives (2026-08-30).
    // The short-circuit is retained anyway because no JSON `.mat` resolver
    // exists yet, not because the files cannot exist. See
    // [`apply_cdb_pbr_fallback`] for the full rationale and for why the
    // `.bgsm`/`.bgem` names do NOT short-circuit here any more (#3230).
    //
    // `from_bgsm` is deliberately left unset by that helper — the flag
    // gates BGSM spec-glossiness translation (an FO4 format convention).
    // Starfield `.mat` authors metalness/roughness directly, and this arm
    // forwards neither: NIF import (`bs_geometry.rs` / `bs_tri_shape.rs` /
    // `import::material::classify_legacy_pbr`) already ran the keyword
    // classifier on `metalness_override` / `roughness_override` before
    // this function was called.
    //
    // #2707 (SF-D8-01) fixed what those overrides actually are for the
    // DOMINANT Starfield case (a `material_reference` stub — 97.9% of
    // sampled meshes): pre-fix, `classify_legacy_pbr` ran on an
    // all-defaults `MaterialInfo` (the walker returns before writing any
    // field for a stub) and unconditionally stamped its terminal
    // `Some(0.0)/Some(0.85)` fallback anyway, permanently disabling
    // `Material::resolve_pbr`'s NaN-sentinel backstop. Post-fix, a stub
    // with no classifier signal at all leaves both overrides `None`, so
    // the NaN sentinel DOES reach `resolve_pbr` — which re-runs the same
    // classifier against whatever real texture / normal-map / env-map-scale
    // data has been merged in BY THEN (this function's own BGSM/BGEM/`.mat`
    // resolution included), rather than the empty snapshot the importer
    // saw. Non-stub Starfield meshes (inline shader data present) still
    // arrive with real `Some(...)` overrides, unchanged.
    //
    // Phase 2 (#3398, CDB per-field extraction) should *overwrite* whichever
    // value is present here with CDB-authored data when a lookup succeeds; a
    // lookup MISS correctly falls through to the sentinel/classifier fallback
    // instead of silently keeping a fabricated constant.
    //
    // #2709 (SF-D9-03) — `PresenceOnly`, not `Merged`: this arm sets exactly
    // one routing flag and forwards no authored field. Phase 2 should return
    // `Merged` once a CDB lookup actually supplies data.
    if starfield_cdb_gate && path.ends_with(".mat") {
        return apply_cdb_pbr_fallback(material, &path);
    }

    // #3230 — for `.bgsm`/`.bgem` names the CDB flip is a *fallback*, not a
    // gate. `starfield_cdb_gate` is non-`.mat` by construction past the
    // early return above, so this is exactly "a Starfield session named a
    // sidecar we should try to parse first". Consumed at each resolve-miss
    // site below.
    let cdb_pbr_fallback = starfield_cdb_gate;

    // Determine dispatch kind from magic (authoritative) with extension as
    // fallback. Warn once per path when they disagree — e.g. a mod shipping a
    // `.bgsm`-named file that carries BGEM magic (wrong-extension footgun).
    use byroredux_bgsm::MaterialKind;
    let ext_kind = if path.ends_with(".bgsm") {
        Some(MaterialKind::Bgsm)
    } else if path.ends_with(".bgem") {
        Some(MaterialKind::Bgem)
    } else {
        None
    };
    let magic_kind = provider.peek_magic(&path);
    if let (Some(ext), Some(magic)) = (ext_kind, magic_kind) {
        if ext != magic {
            log::warn!(
                "material '{}': extension implies {:?} but file magic implies {:?}; \
                 dispatching on magic to avoid wrong override semantics",
                path,
                ext,
                magic
            );
        }
    }
    // Magic wins when present; extension is the fallback for files not (yet)
    // in any loaded archive (caller already got None from peek_magic).
    let dispatch_kind = magic_kind.or(ext_kind);

    if dispatch_kind == Some(MaterialKind::Bgsm) {
        if let Some(outcome) = merge_bgsm_arm(
            material,
            provider,
            pool,
            &path,
            cdb_pbr_fallback,
            &mut touched,
        ) {
            return outcome;
        }
    } else if dispatch_kind == Some(MaterialKind::Bgem) {
        if let Some(outcome) = merge_bgem_arm(
            material,
            provider,
            pool,
            &path,
            cdb_pbr_fallback,
            &mut touched,
        ) {
            return outcome;
        }
    } else {
        // Unknown extension — most likely a Starfield .mat JSON path that
        // SF-D3-01's suffix gate now correctly routes here. The .mat format
        // is not yet parsed (tracked in SF-D6-03). Log once per path so the
        // absence of material data is visible without spamming every frame.
        //
        // A `.mat` path only falls through to this generic arm when the
        // CDB-presence gate above (`has_starfield_cdb`) found no CDB
        // loaded — that's a real degradation (e.g. a future patch bumps
        // CDB fileVersion past the #1569 pins, or `--materials-ba2` was
        // omitted) already logged once, far earlier, in
        // `probe_starfield_cdb`. SF3-02 / #1831 — name that cause
        // explicitly instead of the generic "unsupported format" message,
        // so an operator sees one clear degradation line rather than
        // per-mesh spam disconnected from the upstream CDB failure.
        static WARNED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
            std::sync::OnceLock::new();
        let mut set = WARNED
            .get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if set.insert(path.to_owned()) {
            log::warn!(
                "{}",
                unresolved_material_warning(&path, provider.has_starfield_cdb())
            );
        }
        // #3230 SIBLING — not reachable for a `.bgsm`/`.bgem` path today
        // (`ext_kind` is always `Some` for those, so `dispatch_kind` is too,
        // and both its variants are handled above). It becomes reachable the
        // moment `peek_magic` learns a third `MaterialKind`, and the answer
        // for a Starfield session would be the same as the two arms above,
        // so wire it now rather than leave a hole for that change to fall in.
        if cdb_pbr_fallback {
            return apply_cdb_pbr_fallback(material, &path);
        }
        return MergeOutcome::Unresolved;
    }

    // Both the BGSM and BGEM arms above set `touched` unconditionally
    // alongside `material.from_bgsm = true`, so reaching here with
    // `touched == false` is not currently possible — the `PresenceOnly`
    // arm is deliberate rather than defensive, and stays correct if a
    // future arm resolves without forwarding anything.
    let source = match dispatch_kind {
        Some(MaterialKind::Bgsm) => ImportedTextureSource::Bgsm,
        Some(MaterialKind::Bgem) => ImportedTextureSource::Bgem,
        // #3906 — unreachable: the block above returns for every dispatch_kind
        // that is neither BGSM nor BGEM, `None` included. This used to invent
        // an `ImportedTextureSource::Mat` here, which was the only "producer"
        // of that arm anywhere and produced nothing — see the enum's own doc.
        // Returning what the earlier arm returns for the same condition keeps
        // behaviour identical without fabricating a provenance no texture has.
        None => return MergeOutcome::Unresolved,
    };
    record_external_texture_sources(material, &textures_before, source);

    if touched {
        MergeOutcome::Merged
    } else {
        MergeOutcome::PresenceOnly
    }
}

/// The BGSM (shader-material) arm of [`merge_external_material`] (#3857).
///
/// `Some(outcome)` means the merge is finished and the caller returns it
/// verbatim — the two resolve-failure paths; `None` means the arm merged what
/// it had and the caller continues to the shared tail.
///
/// The ~16 `set_*` sentinels move in here with it. The issue proposed lifting
/// them into a `MergeSentinels` struct threaded through both arms, on the
/// assumption they were shared state — they are not. Every one is written and
/// read only within this arm (the BGEM arm has its own `authored_*` locals),
/// so they are plain locals and the struct is unnecessary. They track "has a
/// BGSM in THIS resolver chain already claimed this slot", which is why they
/// cannot be inferred from the field values: the NIF side defaults several of
/// them to concrete values.
fn merge_bgsm_arm(
    material: &mut ImportedMaterial,
    provider: &mut MaterialProvider,
    pool: &mut byroredux_core::string::StringPool,
    path: &str,
    cdb_pbr_fallback: bool,
    touched: &mut bool,
) -> Option<MergeOutcome> {
    // BGSM/BGEM scalar-override state. The `Option<String>` slots use
    // `is_none()` to detect "NIF left this empty", but scalar PBR fields
    // default to concrete values on the NIF side (e.g. emissive_mult = 0.0,
    // specular_strength = 1.0), so we can't key off the default.
    // Instead we track per-field "has a BGSM entry already overridden
    // this slot" flags — BGSM resolver chain is walked child-first so
    // the first authored value wins, matching the texture-slot policy.
    // Pre-#583 every scalar the BGSM parser decoded was silently dropped
    // and the mesh rendered on NIF-fallback PBR.
    let mut set_emissive = false;
    let mut set_specular = false;
    let mut set_glossiness = false;
    let mut set_alpha = false;
    let mut set_uv = false;
    // #3507 — `tile_u`/`tile_v` (BGSM's texture-address authoring channel)
    // dropped on the floor, same shape as the NIF-side arm this issue
    // paired with.
    let mut set_clamp_mode = false;
    let mut set_blend = false;
    let mut set_fresnel = false;
    let mut set_palette_scale = false;
    // #2607 — the v<8 rim / backlight / subsurface group. Backlight shares
    // `set_rim`: the format gives it no enable bit of its own.
    let mut set_rim = false;
    let mut set_subsurface = false;
    // #2608 — env-map mask scale, authored only on the v<10 base layout.
    let mut set_env_map_scale = false;
    // #2212 (NIFAL-D8-01) — chain-local, unlike `material.alpha_test`
    // itself. The NIF F4SF2 bit-25 path (`dedicated_shader.rs`) can
    // pre-set `material.alpha_test = true` before this loop ever runs, so
    // gating the threshold payload on `!material.alpha_test` (as the code
    // used to) let that lower-priority NIF-synthesized default block the
    // authored BGSM `alpha_test_ref` from ever landing. Track "has a BGSM
    // in THIS chain already set the threshold" separately from the
    // OR'd boolean, matching every other payload-carrying field above.
    let mut set_alpha_test = false;

    let Some(resolved) = provider.resolve_bgsm(&path) else {
        // #3230 — a Starfield session reaches here having genuinely
        // tried and missed, which is the state the CDB flip describes.
        // Taking it BEFORE the diagnostic below is deliberate: that
        // warning's whole premise ("keeps its NIF-native keyword-
        // classified material") is false once the flip runs.
        if cdb_pbr_fallback {
            return Some(apply_cdb_pbr_fallback(material, &path));
        }
        // #2601 — `resolve_bgsm` already logged WHY the resolve failed
        // (missing archive entry, parse error, template-cycle recovery
        // failure — see its own `log::warn!` sites) and recorded `path`
        // into `failed_paths` so repeat failures don't re-spam the log.
        // What neither of those says is the CONSEQUENCE decided right
        // here: this mesh keeps whatever `into_imported_material`
        // (crates/nif) already guessed from the NIF-native keyword
        // classifier — visually indistinguishable from a material that
        // was deliberately authored non-PBR. That's the documented root
        // cause of the recurring "chrome/posterized FO4 surface" class
        // (see feedback_chrome_means_missing_textures) — a broken BGSM
        // reference silently looks identical to intentional legacy
        // authoring. Logging the causal link at the point it's decided
        // means grepping for "keeps its keyword-classified fallback"
        // finds every mesh affected in one search, instead of having to
        // correlate `resolve_bgsm`'s low-level reason against which
        // REFRs actually dispatched as BGSM.
        log::warn!(
            "material '{}': BGSM resolve failed — mesh keeps its NIF-native \
             keyword-classified material (legacy Lambert guess) instead of \
             the authoritative BGSM PBR data",
            path
        );
        return Some(MergeOutcome::Unresolved);
    };
    // BGSM resolution succeeded — telemetry-only flag (no renderer
    // branch); the substantive work happens in the spec-glossiness
    // → metallic-roughness translation below.
    material.from_bgsm = true;
    *touched = true;
    // #1352 — any successful BGSM resolve routes the material through
    // the Disney/PBR diffuse lobe, unconditionally. #2700 (FO4-D2-01):
    // `a0f75fc5` narrowed this to `resolved.walk().any(|s| s.file.pbr)`
    // — "BGSM is a container, not a BRDF declaration" — measured at 0
    // of 6,616 vanilla FO4 BGSMs, so the narrower gate silently
    // reverted #1352 across every vanilla FO4 surface with a green
    // test suite covering the divergence. It also went unnoticed that
    // this makes `forward_bgsm_rim_subsurface`'s rim/backlight/
    // subsurface scalars (#2607) dead weight for the same 100% of
    // content: that lobe is the only consumer of those fields, and it
    // was never selected. The metalness/roughness/F0 derivation just
    // below already treats every BGSM material as physically based
    // regardless of the `pbr` bit — that bit only changes HOW
    // metalness is derived (spec-color-as-F0 vs spec-color
    // chromaticity), never WHETHER the material is PBR — so gating
    // only the diffuse lobe on a bit real content essentially never
    // sets was an internally inconsistent narrower contract, not a
    // deliberate one (the `a0f75fc5` commit message never mentions
    // PBR routing). Restored to #1352's original, still-documented
    // intent (see the sibling tests in `tests/bgsm_merge.rs`).
    material.is_pbr = true;

    // ── Translation layer (BGSM spec-glossiness → standard PBR) ──
    //
    // The renderer consumes a single PBR contract: `albedo`,
    // `metalness`, `roughness`, `F0 = mix(0.04, albedo, metalness)`.
    // BGSM authors a DIFFERENT contract; how `specular_color * mult`
    // relates to metalness depends on the BGSM's `pbr` flag:
    //
    // * `pbr == true` (rare — 0 of 793 sampled vanilla FO4 BGSMs set
    //   it; almost exclusively modded content): the material was
    //   authored in a metallic-roughness workflow and `spec_color *
    //   mult` IS F0 directly (dielectric ≈ 0.04, conductor ≈ tinted).
    //   Luminance → metalness is correct here.
    //
    // * `pbr == false` (legacy spec-glossiness — essentially all
    //   vanilla FO4 architecture/clutter): `spec_color` is the Blinn
    //   highlight TINT, not F0. It is ~white `[1,1,1]` for every
    //   dielectric (concrete, wood, plaster, painted metal) and the
    //   `mult` only scales highlight strength. Keying metalness off
    //   luminance is not just wrong but BACKWARDS: vanilla
    //   `paintpeelingconcrete` authors `spec=[1,1,1] mult=1.0`
    //   (lum 1.0 → metalness 1.0, mirror-chrome concrete) while real
    //   metals author LOWER, often TINTED spec — `metalrubberductpipe`
    //   `[1,1,1] mult=0.73`, `metallocker` `[1,0.85,0.70] mult=0.45`.
    //   The only legacy signal that actually distinguishes a conductor
    //   is spec CHROMATICITY (conductor F0 is tinted; dielectric F0 is
    //   achromatic grey), so we derive metalness from spec-color
    //   saturation, which is invariant to `mult`. White spec → 0
    //   (concrete is dielectric); tinted spec → metallic (brass/gold/
    //   copper keep their look). Pure-white-spec steel reads dielectric
    //   — a minor under-read, but never the pervasive chrome the old
    //   luminance path produced. (Per-texel metalness from the spec
    //   map would recover white-spec steel; deferred — needs a
    //   metalness-map shader binding. See `feedback_format_translation`.)
    //
    // Roughness is `1 - smoothness` either way (the per-texel
    // `gloss_map` then modulates it in-shader: `mix(1, roughness,
    // glossSample)`), so the scalar is only the smooth-end of the lobe.
    //
    // Derivation is LEAF-only — the leaf author's choice is
    // authoritative; template parents are background defaults the
    // artist explicitly overrode if they set a different value.
    //
    // For metallic materials, also tint `material.diffuse_color` toward
    // the authored spec_color so the per-pixel `F0 = mix(0.04,
    // albedo, metalness)` lands on the right conductor tint when
    // the diffuse texture is BC1-desaturated (a known FO4 issue —
    // raw_metal_diff DDS textures lose saturation vs the authored
    // spec RGB). Pure dielectric materials keep `diffuse_color`
    // untouched so painted-plastic textures aren't shifted.
    let leaf = &resolved.file;
    let spec_r = leaf.specular_color[0] * leaf.specular_mult;
    let spec_g = leaf.specular_color[1] * leaf.specular_mult;
    let spec_b = leaf.specular_color[2] * leaf.specular_mult;
    // pbr: spec*mult is F0. Legacy: mult-free specular_color, since
    // `mult` only scales highlight strength, not F0 — see
    // `bgsm_metalness` doc comment (#1476).
    let metalness = if leaf.pbr {
        bgsm_metalness([spec_r, spec_g, spec_b], true)
    } else {
        bgsm_metalness(leaf.specular_color, false)
    };
    let roughness = (1.0 - leaf.smoothness).clamp(NEAR_MIRROR_ROUGHNESS_FLOOR, 1.0);
    material.metalness_override = Some(metalness);
    material.roughness_override = Some(roughness);
    // #2609 — the flag whose meaning is "authoritative PBR scalars were
    // merged", set at the exact site that merges them. `from_bgsm` above
    // cannot serve that role: the BGEM arm sets it too while leaving both
    // overrides `None` (BGEM authors no smoothness/specular), so a
    // consumer reading `from_bgsm` as "scalars present" is wrong on every
    // effect material. Keep this write adjacent to the two it describes.
    material.bgsm_pbr_scalars_authored = true;
    if metalness > 0.5 {
        // #1591 — blend toward the mult-free `specular_color`, NOT
        // `spec_*` (= specular_color × specular_mult); the mult-bearing
        // `spec_*` stays for the pbr F0-luminance path above where
        // mult-as-scale is correct. See `conductor_diffuse_tint`.
        material.diffuse_color =
            conductor_diffuse_tint(material.diffuse_color, leaf.specular_color);
    }
    // #3898 — whether the NIF itself supplied the greyscale LUT before any
    // BGSM in this chain got a turn. `fill` is first-non-empty-wins, so a
    // NIF-supplied slot 3 (#2997) means every BGSM's own greyscale texture
    // LOSES the role — but the BGSM's palette *enable* bit is a statement
    // about the material, not about which texture resource is sampled, and
    // dropping it silently is what left the remap off. Captured before the
    // walk so a closer BGSM winning the slot mid-walk is distinguishable
    // from the NIF having won it outright.
    let nif_supplied_greyscale_lut = material.textures.greyscale_lut.is_some();
    for step in resolved.walk() {
        let bgsm = &step.file;
        fill(
            &mut material.textures.base_color,
            &bgsm.diffuse_texture,
            touched,
            pool,
        );
        fill(
            &mut material.textures.normal,
            &bgsm.normal_texture,
            touched,
            pool,
        );
        fill(
            &mut material.textures.emissive,
            &bgsm.glow_texture,
            touched,
            pool,
        );
        // Smoothness/spec mask — .r encodes per-texel specular
        // strength in the engine's existing gloss_map slot. #453.
        fill(
            &mut material.textures.smooth_spec,
            &bgsm.smooth_spec_texture,
            touched,
            pool,
        );
        // #1353 / FO4-D8-07 — BGSM greyscale-to-palette LUT path
        // (`SLSF1::Greyscale_To_PaletteColor`, used by FO4 NPC /
        // creature colour variants; the palette slot is authored on
        // v<=2 BGSMs). First non-empty in the template chain wins, to
        // match the texture fills above. Routed through the common
        // greyscale_lut role and flagged via EFFECT_PALETTE_COLOR in
        // `pack_imported_material_flags` so the lit-path remap samples it.
        //
        // #2108 (SF-D9-01) — the greyscale slot is a legal, always-
        // serialized field; its presence alone does NOT mean the
        // material wants the remap. Capture the authoritative
        // `grayscale_to_palette_color` enable bit from THIS SAME BGSM
        // (not `fill`'s generic helper, and not OR'd across the whole
        // chain) at the exact step that supplies the texture — an
        // ancestor's own enable bit is irrelevant once a closer BGSM
        // already won the texture slot.
        //
        // #3898 — the `is_none()` half above is precedence among BGSMs and
        // stays exactly as #2108 wrote it. What it silently also did was
        // drop the enable bit whenever the NIF's own slot 3 had already
        // filled the role (#2997 made that the common case on FO4), so a
        // BGSM asking for the remap was ignored. Split the two situations:
        //
        //   - this BGSM wins the slot  -> it is authoritative for both the
        //     texture and the enable bit (assignment, unchanged)
        //   - the NIF won the slot     -> the LUT sampled is the NIF's, but
        //     the BGSM still describes this material, so OR its enable bit
        //     in. OR, not assignment: neither source may silently disable
        //     the remap the other asked for, and the NIF's own SLSF1 bit
        //     (#3897) has already been forwarded onto these same fields.
        //   - a closer BGSM won the slot -> unchanged: an ancestor's bit is
        //     irrelevant, which is why this keys on `nif_supplied_greyscale_lut`
        //     rather than on `is_some()`.
        if !bgsm.greyscale_texture.is_empty() {
            if material.textures.greyscale_lut.is_none() {
                material.bgsm_greyscale_lut_enabled = bgsm.base.grayscale_to_palette_color;
                // #2643 — BGSM has no alpha-variant field, so the color
                // bit is the only one this format can author.
                material.bgsm_greyscale_lut_color = bgsm.base.grayscale_to_palette_color;
            } else if nif_supplied_greyscale_lut {
                material.bgsm_greyscale_lut_enabled |= bgsm.base.grayscale_to_palette_color;
                material.bgsm_greyscale_lut_color |= bgsm.base.grayscale_to_palette_color;
            }
        }
        fill(
            &mut material.textures.greyscale_lut,
            &bgsm.greyscale_texture,
            touched,
            pool,
        );
        // Legacy v <= 2 environment cube; newer BGSMs drop the slot.
        fill(
            &mut material.textures.environment,
            &bgsm.envmap_texture,
            touched,
            pool,
        );
        fill(
            &mut material.textures.height,
            &bgsm.displacement_texture,
            touched,
            pool,
        );
        // #2627 / SF-D9-2026-08-07-02 — the v<=2 legacy texture list
        // reads envmap, glow, inner_layer, wrinkles, displacement (see
        // `bgsm.rs`'s parser comment); this was the one slot in that
        // set the merge never forwarded, even though the sink is a
        // live, populated role — the NIF `BSLightingShaderProperty`
        // multi-layer-parallax path already resolves
        // `MaterialTextureSet::inner_layer` to a real texture handle.
        // A BGSM authoring its inner layer externally (Skyrim SE
        // ice/glass, FO4 layered panes) rendered with the layer
        // silently absent.
        fill(
            &mut material.textures.inner_layer,
            &bgsm.inner_layer_texture,
            touched,
            pool,
        );
        // #1076 / FO4-D6-002 — BGSM v>2 standalone slots that
        // pre-fix were parsed but dropped on the floor. Each is
        // empty on the v<=2 path (the parser leaves the String
        // default) so the `fill` no-op suffices to gate the
        // forward without an explicit version check.
        fill(
            &mut material.textures.specular,
            &bgsm.specular_texture,
            touched,
            pool,
        );
        fill(
            &mut material.textures.lighting,
            &bgsm.lighting_texture,
            touched,
            pool,
        );
        fill(
            &mut material.textures.flow,
            &bgsm.flow_texture,
            touched,
            pool,
        );
        fill(
            &mut material.textures.wrinkle,
            &bgsm.wrinkles_texture,
            touched,
            pool,
        );
        // #2642 (SF-D9-2026-08-07-03) — `bgsm.distance_field_alpha_texture`
        // (v>=17, FO76/Starfield-era) is deliberately NOT forwarded here.
        // `MaterialTextureSet` has no dedicated role for it — a genuine
        // deferred-consumer gap, not a wiring bug, unlike every other
        // texture slot in this block. Signage/decal cutouts authored
        // with distance-field alpha fall back to plain alpha test until
        // a role + shader consumer exist.
        // #1077 / FO4-D6-003 (Phase 1: data propagation) — BGSM-only
        // shader flags, extracted to `forward_bgsm_phase1_flags`
        // (#2702 / FO4-D2-03) so its regression tests exercise this
        // exact call, not a hand-copied mirror.
        forward_bgsm_phase1_flags(material, bgsm, touched);

        // #1147 Phase 2b — BGSM v>=8 translucency suite. Same
        // child-first precedence as the flags above. The
        // `has_translucency` flag is the gate; if the child
        // already set it, the corresponding subsurface params
        // also came from the child and we don't overwrite them.
        // If `has_translucency` is set by this chain entry but
        // the params are still at default-zero, propagate them.
        if bgsm.translucency
            && material.translucency_transmissive_scale == 0.0
            && material.translucency_subsurface_color == [0.0; 3]
        {
            material.translucency_subsurface_color = bgsm.translucency_subsurface_color;
            material.translucency_transmissive_scale = bgsm.translucency_transmissive_scale;
            material.translucency_turbulence = bgsm.translucency_turbulence;
            material.translucency_thick_object = bgsm.translucency_thick_object;
            material.translucency_mix_albedo = bgsm.translucency_mix_albedo_with_subsurface_color;
            *touched = true;
        }

        // Scalar PBR forwarding (#583). Child-first: first authored
        // value wins. Parser already decodes these fields; the
        // pre-fix merge dropped them on the floor.
        if !set_emissive && bgsm.emit_enabled {
            material.emissive_color = bgsm.emittance_color;
            material.emissive_mult = bgsm.emittance_mult;
            set_emissive = true;
            *touched = true;
        }
        if !set_specular {
            material.specular_color = bgsm.specular_color;
            material.specular_strength = bgsm.specular_mult;
            set_specular = true;
            *touched = true;
        }
        if !set_glossiness {
            // BGSM authors `smoothness` 0–1 (Bethesda Material Editor
            // convention); `Material::glossiness` is on the 0–100 NIF
            // scale (`classify_pbr_keyword` divides by 100). Multiply
            // by 100 to normalize — without this, BGSM-driven FO4
            // materials that don't keyword-match the metal/wood/glass
            // arms in `classify_pbr_keyword` fall through to the
            // glossiness fallback
            // with `roughness=0.95`, killing direct specular and the
            // RT-reflection metalness/roughness gate (Med-Tek floors).
            material.glossiness = bgsm.smoothness * 100.0;
            set_glossiness = true;
            *touched = true;
        }
        // #1454 — BGSM authors Fresnel power (Schlick exponent for the
        // rim Fresnel term). Child-first: first BGSM in the template
        // chain wins. Vanilla FO4 defaults to 5.0, matching the
        // `ImportedMesh` default, so no vanilla regression; mod-authored
        // non-default values (power armor, shiny metals) were silently
        // falling back to 5.0 before this fix.
        if !set_fresnel {
            material.fresnel_power = bgsm.fresnel_power;
            set_fresnel = true;
            *touched = true;
        }
        // #1455 — BGSM authors greyscale-to-palette scale. Child-first.
        // Modulates the LUT remap intensity for NPC creature colour
        // variants (deathclaw, supermutant). Default 1.0 = no change.
        if !set_palette_scale {
            material.grayscale_to_palette_scale = bgsm.grayscale_to_palette_scale;
            set_palette_scale = true;
            *touched = true;
        }
        // #2607 / #2608 (FO4-D7-02 / FO4-D7-03) — two more BGSM field
        // groups that decoded correctly and were dropped at this exact
        // hop. Both are enable-bit gated; see the fn docs for why
        // unconditional forwarding would be fabrication rather than
        // translation.
        forward_bgsm_rim_subsurface(material, bgsm, &mut set_rim, &mut set_subsurface, touched);
        forward_bgsm_env_map_scale(material, bgsm, &mut set_env_map_scale, touched);
        if !set_alpha {
            material.mat_alpha = bgsm.base.alpha;
            set_alpha = true;
            *touched = true;
        }
        if !set_uv {
            material.uv_offset = [bgsm.base.u_offset, bgsm.base.v_offset];
            material.uv_scale = [bgsm.base.u_scale, bgsm.base.v_scale];
            set_uv = true;
            *touched = true;
        }
        // #3507 — `tile_u`/`tile_v` are BGSM's own texture-address-mode
        // authoring channel (`base.rs:174-175` decodes them from the
        // same bit-packed byte nif.xml's `TexClampMode` enum uses:
        // bit 1 = S-axis wrap, bit 0 = T-axis wrap), separate from and
        // in addition to the NIF shader property's `texture_clamp_mode`
        // field this issue's sibling fix restores.
        if !set_clamp_mode {
            material.texture_clamp_mode =
                ((bgsm.base.tile_u as u8) << 1) | (bgsm.base.tile_v as u8);
            set_clamp_mode = true;
            *touched = true;
        }
        // Boolean gameplay flags OR across the template chain — if
        // ANY ancestor marks the material as two-sided / decal /
        // alpha-test, the concrete instance is too.
        if bgsm.base.two_sided {
            material.two_sided = true;
            *touched = true;
        }
        if bgsm.base.decal {
            material.is_decal = true;
            *touched = true;
        }
        // #2212 (NIFAL-D8-01) — the boolean itself stays a pure OR
        // across the chain (matches the `two_sided` / `decal` siblings
        // above and the doc comment's stated policy), but the
        // threshold payload uses the chain-local `set_alpha_test`
        // sentinel, not `material.alpha_test`'s value, so a NIF
        // F4SF2-bit-25-synthesized default (pre-set before this loop
        // runs) can never outrank the authored BGSM `alpha_test_ref`.
        if bgsm.base.alpha_test {
            material.alpha_test = true;
            if !set_alpha_test {
                material.alpha_threshold = f32::from(bgsm.base.alpha_test_ref) / 255.0;
                set_alpha_test = true;
            }
            *touched = true;
        }
        // BGSM alpha-blend forwarding. FO4+ moved per-material blend
        // state out of NiAlphaProperty into BGSM, so a BGSM-only
        // glass / decal authored with `alpha_blend_mode.function == 1`
        // (Standard) leaves the NIF-side `has_alpha` at false and
        // every Institute / lab pane renders fully opaque
        // (`INSTANCE_FLAG_ALPHA_BLEND` never sets → MATERIAL_KIND_GLASS
        // never classifies → opaque path).
        //
        // Child-first precedence (matches the texture / scalar walks):
        // first authored function > 0 wins. function == 0 (None)
        // intentionally does NOT clear an already-set blend — a leaf
        // that opts out shouldn't erase a parent's blend authoring.
        //
        // BGSM `src_blend` / `dst_blend` are already Gamebryo-native
        // values — `bgsm_blend_to_gamebryo` just narrows the `u32`
        // to the `u8` the renderer's blend-factor field expects, no
        // translation. See its doc for why (#1823, regression of a
        // wrong #1651 fix that assumed a GL-style enum requiring a
        // swap).
        if !set_blend && bgsm.base.alpha_blend_mode.function > 0 {
            material.has_alpha = true;
            material.src_blend_mode = bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.src_blend);
            material.dst_blend_mode = bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.dst_blend);
            set_blend = true;
            *touched = true;
        }
        // #2704 (FO4-D7-02) — Deferred: no consumer. These eleven BGSM
        // scalars decode correctly on the parser side (`bgsm.rs`) but
        // have no `ImportedMaterial` sink here, same deferred-consumer
        // class as the BGEM v21+/v22 glass-overlay suite above:
        //   * the entire wetness-control suite — `wetness_control_spec_scale`,
        //     `wetness_control_spec_power_scale`, `wetness_control_spec_min_var`,
        //     `wetness_control_env_map_scale`, `wetness_control_fresnel_power`,
        //     `wetness_control_metalness` — the authored input the ROADMAP
        //     M61 wet-surface feature would need
        //   * `custom_porosity`, `porosity_value` (v>=9 porosity pair)
        //   * `adaptive_emissive_exposure_offset` (v>=13 adaptive-emissive tuning)
        //   * `aniso_lighting`
        //   * `external_emittance`
        // No runtime effect today; flagging so the next completeness
        // sweep can tell "not yet wired" from "overlooked".
    }
    // #3639 — `smoothness == 1.0` lowers `roughness` to the 0.04 clamp
    // floor above (near-mirror dielectric). `triangle.frag` only
    // modulates that back up via the per-texel gloss map (`mat.
    // glossMapIndex != 0u` branch: `roughness = mix(1.0, roughness,
    // glossTexel.r)`); with no map there is nothing to modulate with,
    // and the material is stuck at the floor with no per-pixel escape.
    // The walk above is the full template-chain resolution (a parent
    // can still supply the slot the leaf doesn't), so only fall back
    // once it's had every chance to fill `smooth_spec`. `0.5` matches
    // the neutral roughness `classify_pbr_keyword`'s arms already use
    // elsewhere (`crates/core/src/ecs/components/material.rs`) rather
    // than inventing a second "no data" convention.
    //
    // #3905 (NIFAL-2026-09-05-D1-01) — this predicate is the AUTHORED
    // path; the shader escape it restores is gated on the RESOLVED
    // bindless index (`glossMapIndex != 0u`). An authored-but-
    // unresolvable gloss map (missing from the archive, failed load)
    // satisfies `is_some()` here and still resolves to handle 0 there, so
    // it stayed pinned at the floor. That gap cannot be closed here:
    // `MaterialProvider.archives` is the MATERIALS pool
    // (`--materials-ba2` + the mesh archives scanned for CDBs), while
    // textures resolve out of `TextureProvider`'s separate
    // `texture_archives` pool — this boundary has no way to ask whether a
    // texture path will resolve. What it does have, and spawn does not,
    // is the full template chain. So the split is by what each site can
    // see: this arm owns "no gloss map authored anywhere in the chain",
    // and `material_translate::resolve_unresolved_gloss_neutral_roughness`
    // owns "authored, but resolved to handle 0", using the shader's own
    // predicate. Both apply `NEAR_MIRROR_NEUTRAL_ROUGHNESS`, and the
    // spawn pass is a no-op on materials this arm already neutralised.
    if leaf.smoothness >= 1.0 && material.textures.smooth_spec.is_none() {
        material.roughness_override = Some(NEAR_MIRROR_NEUTRAL_ROUGHNESS);
    }

    None
}

/// The BGEM (effect-material) arm of [`merge_external_material`] (#3857).
///
/// Same contract as [`merge_bgsm_arm`]: `Some(outcome)` finishes the merge,
/// `None` continues to the shared tail.
fn merge_bgem_arm(
    material: &mut ImportedMaterial,
    provider: &mut MaterialProvider,
    pool: &mut byroredux_core::string::StringPool,
    path: &str,
    cdb_pbr_fallback: bool,
    touched: &mut bool,
) -> Option<MergeOutcome> {
    let Some(bgem) = provider.resolve_bgem(&path) else {
        // #3230 — sibling of the BGSM arm's fallback above.
        if cdb_pbr_fallback {
            return Some(apply_cdb_pbr_fallback(material, &path));
        }
        // #2601 — sibling of the BGSM arm's diagnostic above. Same
        // consequence: this mesh keeps the NIF-native keyword-
        // classified fallback instead of authoritative BGEM data.
        log::warn!(
            "material '{}': BGEM resolve failed — mesh keeps its NIF-native \
             keyword-classified material (legacy Lambert guess) instead of \
             the authoritative BGEM data",
            path
        );
        return Some(MergeOutcome::Unresolved);
    };
    // BGEM (effect material) has no smoothness/specular authoring —
    // metalness and roughness are left as NaN sentinels so resolve_pbr
    // runs the keyword classifier. glass_enabled surfaces get the glass
    // roughness override from classify_glass_into_material downstream.
    material.from_bgsm = true;
    // #2366 — v20+ BGEMs can explicitly opt into the PBR specular
    // workflow. Preserve an existing true value and promote false only
    // when the parsed effect-material flag requests it.
    material.is_pbr |= bgem.effect_pbr_specular;
    *touched = true;
    fill(
        &mut material.textures.base_color,
        &bgem.base_texture,
        touched,
        pool,
    );
    fill(
        &mut material.textures.normal,
        &bgem.normal_texture,
        touched,
        pool,
    );
    fill(
        &mut material.textures.emissive,
        &bgem.glow_texture,
        touched,
        pool,
    );
    // #1453 — BGEM's grayscale_texture is the palette/gradient LUT for
    // effect materials (fire-gradient, electricity-gradient, magic VFX).
    // Forward it to the same common greyscale_lut role BGSM uses — both
    // resolve through MaterialTextureHandles and the
    // `EFFECT_PALETTE_COLOR` flag.
    if material.textures.greyscale_lut.is_none() && !bgem.grayscale_texture.is_empty() {
        material.textures.greyscale_lut = Some(pool.intern(&bgem.grayscale_texture));
        // #1580 / #2643 — BGEM's own alpha-variant bool and the shared
        // color bit are independent authoring (the format permits
        // setting both at once), so track them as two separate flags
        // and let `pack_imported_material_flags` OR both
        // EFFECT_PALETTE_COLOR / EFFECT_PALETTE_ALPHA in independently
        // — see `pack_imported_material_flags` in `cell_loader.rs`.
        // Previously `bgsm_greyscale_lut_is_alpha` alone decided
        // COLOR-vs-ALPHA, which silently dropped the color variant
        // whenever a BGEM authored both bits.
        material.bgsm_greyscale_lut_is_alpha = bgem.grayscale_to_palette_alpha;
        material.bgsm_greyscale_lut_color = bgem.base.grayscale_to_palette_color;
        // #2108 (SF-D9-01) — either enable bit (the shared
        // `grayscale_to_palette_color`, or BGEM's alpha-variant
        // `grayscale_to_palette_alpha`) turns the remap on; which of
        // COLOR/ALPHA the packer sets is decided separately, above, by
        // `bgsm_greyscale_lut_color` / `bgsm_greyscale_lut_is_alpha`.
        // The texture slot being filled is not itself an enable signal.
        material.bgsm_greyscale_lut_enabled =
            bgem.base.grayscale_to_palette_color || bgem.grayscale_to_palette_alpha;
        *touched = true;
    }
    // #2643 (SF-D9-2026-08-07-04) — gate the envmap texture fill on
    // the authored `env_mapping_enabled()` bit, the version-aware
    // accessor `bgem_uses_glass_behavior` above already consults
    // (`reflective_surface_maps`, #2358). Previously this filled
    // unconditionally from `envmap_texture`/`envmap_mask_texture`
    // regardless of whether the material actually enabled env
    // mapping, so the same authored bit was honoured for glass
    // classification and ignored for texture binding within one
    // file — a BGEM with a stale/unused envmap slot but the enable
    // bit off would still bind it.
    if bgem.env_mapping_enabled() {
        fill(
            &mut material.textures.environment,
            &bgem.envmap_texture,
            touched,
            pool,
        );
        fill(
            &mut material.textures.environment_mask,
            &bgem.envmap_mask_texture,
            touched,
            pool,
        );
    }
    // #1076 / FO4-D6-002 SIBLING — BGEM also exposes
    // `specular_texture` + `lighting_texture` (the two BGSM v>2
    // slots that exist on the BGEM side too; BGEM does NOT
    // author `flow_texture` or `wrinkles_texture` per
    // `crates/bgsm/src/bgem.rs`). Forward them here so the BGEM
    // path has the same coverage as the BGSM path.
    fill(
        &mut material.textures.specular,
        &bgem.specular_texture,
        touched,
        pool,
    );
    fill(
        &mut material.textures.lighting,
        &bgem.lighting_texture,
        touched,
        pool,
    );

    // BGEM has no inheritance so there's no child-first chain.
    // `base_color × base_color_scale` is the primary effect tint —
    // the same authoring the NIF-side walker reads from
    // `BSEffectShaderProperty.base_color` / `base_color_scale`. Set
    // EmissiveSource::Effect so the renderer knows this slot is an
    // effect-diffuse tint, not a genuine emissive scalar. #1358.
    // `emittance_color` (v≥11 additive glow) is deferred until a
    // second emissive slot exists on `ImportedMesh`.
    // #3371 (SKY-2026-08-27-D7-03) — the fourth writer of
    // `emissive_source`, and the one #2591 missed. Gate it on the
    // same `emissive_contribution_is_authored` predicate as the three
    // NIF-side sites: a BGEM with `base_color == [0,0,0]` or
    // `base_color_scale == 0.0` authored no contribution, and tagging
    // it `Effect` regardless degenerates the discriminator to "has an
    // effect shader" — exactly what #2591 fixed elsewhere.
    material.emissive_color = bgem.base_color;
    material.emissive_mult = bgem.base_color_scale;
    if byroredux_core::ecs::components::material::emissive_contribution_is_authored(
        material.emissive_color,
        material.emissive_mult,
    ) {
        material.emissive_source =
            byroredux_core::ecs::components::material::EmissiveSource::Effect;
    }
    material.mat_alpha = bgem.base.alpha;
    material.uv_offset = [bgem.base.u_offset, bgem.base.v_offset];
    material.uv_scale = [bgem.base.u_scale, bgem.base.v_scale];
    // #3507 — same `tile_u`/`tile_v` → `texture_clamp_mode` mapping as
    // the BGSM chain above; BGEM shares `BaseMaterial` and has no
    // inheritance, so this is an unconditional write like its
    // `uv_offset`/`uv_scale` neighbours.
    material.texture_clamp_mode = ((bgem.base.tile_u as u8) << 1) | (bgem.base.tile_v as u8);
    if bgem.base.two_sided {
        material.two_sided = true;
    }
    if bgem.base.decal {
        material.is_decal = true;
    }
    if bgem.base.alpha_test {
        material.alpha_test = true;
        material.alpha_threshold = f32::from(bgem.base.alpha_test_ref) / 255.0;
    }
    // BGEM alpha-blend — same GL→Gamebryo translation as the BGSM
    // branch above, applied to the BSEffectShaderProperty path.
    // This is the path that hit #1651: additive glow/effect cards
    // author `(One, One)` = `(1, 1)` which, forwarded raw, the
    // renderer reads as `(ZERO, ZERO)` and renders invisible.
    // BGEM has no inheritance so no child-first guard needed.
    if bgem.base.alpha_blend_mode.function > 0 {
        material.has_alpha = true;
        material.src_blend_mode = bgsm_blend_to_gamebryo(bgem.base.alpha_blend_mode.src_blend);
        material.dst_blend_mode = bgsm_blend_to_gamebryo(bgem.base.alpha_blend_mode.dst_blend);
    }
    // #1280 sub-step 3b — forward BGEM glass semantics so the
    // spawn-time classifier in `helpers::classify_glass_into_material`
    // can fire the glass path even when neither the texture path nor
    // the mesh name carries a glass keyword. v21+ files expose the
    // direct `glass_enabled` field; older FO4 files use the equivalent
    // blend/depth/falloff/environment-map feature bundle recognized by
    // `bgem_uses_glass_behavior` (Port-A-Diner's v2 dome).
    if bgem_uses_glass_behavior(&bgem) {
        material.bgem_glass = true;
        // `non_occluder` is the authored behavioral distinction between
        // a thin transmissive shell (display dome/window sheet) and a
        // closed glass volume. Preserve it independently of BGEM so the
        // shared glass shader can choose a surface-consistent base path;
        // texture maps remain ordinary overlays either way.
        material.thin_glass = bgem_uses_thin_glass_behavior(&bgem);
        // BGEM v21+/v22 glass-overlay suite. These are semantic glass
        // inputs, not generic detail/specular maps: preserve dedicated
        // roles so a material that authors both cannot overwrite one.
        material.glass_fresnel_color = bgem.glass_fresnel_color;
        material.glass_refraction_scale = bgem.glass_refraction_scale_base;
        material.glass_blur_scale = bgem.glass_blur_scale_base;
        material.glass_blur_scale_factor = bgem.glass_blur_scale_factor;
        fill(
            &mut material.textures.glass_roughness_scratch,
            &bgem.glass_roughness_scratch,
            touched,
            pool,
        );
        fill(
            &mut material.textures.glass_dirt_overlay,
            &bgem.glass_dirt_overlay,
            touched,
            pool,
        );
        //
        // #2608 correction: `environment_mapping_mask_scale` was listed
        // above as sink-less, and no longer is —
        // `ImportedMaterial.env_map_scale` is now wired on the BGSM arm
        // (`forward_bgsm_env_map_scale`), and `env_mapping_enabled()` is
        // the exact authored gate on this side. It is left unforwarded
        // DELIBERATELY rather than overlooked: BGEM leaves the PBR
        // overrides `None`, so `Material::resolve_pbr` runs its keyword
        // classifier, whose `env_map_scale > 0.3` arm would then start
        // moving roughness on every env-mapped FO4 effect material. That
        // is a shading change to evaluate on its own evidence, not a
        // drop to fix in passing.
    }
    // Soft-particle depth fade + view-angle falloff cone. The NIF
    // `BSEffectShaderProperty` path fills `material.effect_shader` from the
    // block; the BGEM path is the FO4+ equivalent and must mirror it so
    // `material_translate` can build `Material.{effect_falloff,
    // effect_shader_flags}` (soft_falloff_depth + MAT_FLAG_EFFECT_SOFT)
    // the same way. Without this every FO4 BGEM mist/steam/beam volume
    // (`soft = true` in the authored file) rendered with no depth feather
    // and stacked to an opaque white-out (HalluciGen labs). `lighting_influence`
    // is authored 0..1 in BGEM but carried 0..255 on the shared payload.
    material.effect_shader = Some(byroredux_nif::import::BsEffectShaderData {
        falloff_start_angle: bgem.falloff_start_angle,
        falloff_stop_angle: bgem.falloff_stop_angle,
        falloff_start_opacity: bgem.falloff_start_opacity,
        falloff_stop_opacity: bgem.falloff_stop_opacity,
        soft_falloff_depth: bgem.soft_depth,
        effect_soft: bgem.soft_enabled,
        effect_lit: bgem.effect_lighting_enabled,
        lighting_influence: (bgem.lighting_influence.clamp(0.0, 1.0) * 255.0).round() as u8,
        ..Default::default()
    });
    *touched = true;

    None
}

/// #2412 / #3857 — pin the invariant the split had to preserve.
///
/// #2412 examined `merge_external_material` at 678 LOC and closed with an
/// explicit *no-action* recommendation: *"it is a deliberate single NIFAL
/// boundary and should not be split in a way that weakens that invariant."*
/// #3857 then had to split it anyway — it had grown to 989 LOC and 46 % of a
/// file that had itself crossed the file-level threshold — so the invariant
/// stopped being a property of "we didn't touch it" and became a property
/// that needs asserting.
///
/// The boundary is a *visibility* claim, and visibility is exactly what a
/// behavioural test cannot see: `merge_bgsm_arm` would keep passing every
/// existing merge test on the day someone marks it `pub(crate)` and calls it
/// directly from a per-game path, which is the failure #2412 was guarding
/// against.
#[cfg(test)]
mod single_boundary_tests {
    /// Exactly one exported function in this file, and it is the merge entry
    /// point. The two per-kind arms are private siblings that only it calls.
    #[test]
    fn merge_external_material_is_the_only_exported_fn_in_this_file() {
        let src = include_str!("merge.rs");
        let exported: Vec<&str> = src
            .lines()
            .filter(|l| {
                l.starts_with("pub fn ")
                    || l.starts_with("pub(crate) fn ")
                    || l.starts_with("pub(super) fn ")
            })
            .collect();
        assert_eq!(
            exported,
            vec!["pub(crate) fn merge_external_material("],
            "`merge_external_material` must stay the ONE way a sidecar reaches a \
             `&mut ImportedMaterial` (#2412). `merge_bgsm_arm` / `merge_bgem_arm` are \
             private siblings on purpose: exporting either lets a per-game path merge \
             one kind directly, which is the single-boundary invariant gone — and \
             every existing merge test would still pass. If a second entry point is \
             genuinely wanted, change this test deliberately and say why."
        );
    }

    /// Anti-vacuity: the arms must still exist, and still be the things taking
    /// `&mut ImportedMaterial`. A rename or a reformat that empties the scan
    /// above must fail loudly rather than pass silently.
    #[test]
    fn the_private_arms_are_still_present_and_take_the_material() {
        let src = include_str!("merge.rs");
        for arm in ["fn merge_bgsm_arm(", "fn merge_bgem_arm("] {
            let start = src
                .find(arm)
                .unwrap_or_else(|| panic!("{arm} must exist — the split is what #3857 did"));
            let sig = &src[start..start + src[start..].find(") -> ").expect("signature")];
            assert!(
                sig.contains("material: &mut ImportedMaterial"),
                "{arm} must still take the material it merges into"
            );
        }
    }
}

#[cfg(test)]
mod texture_source_provenance_tests {
    use super::record_external_texture_sources;
    use byroredux_nif::import::{ImportedMaterial, ImportedTextureSource, MaterialTextureSet};

    fn interned(text: &str) -> byroredux_core::string::FixedString {
        byroredux_core::string::StringPool::new().intern(text)
    }

    /// #3903 — every role the merge fills must be labelled with the merge's
    /// source, with no role left behind.
    ///
    /// The walk is exhaustive by construction now (it goes through
    /// `zip_map_ref`, which builds a struct literal), so a missed role is a
    /// compile error rather than a silent drop. This pins the *behaviour* that
    /// construction is supposed to produce, and counts the roles it actually
    /// observed against `values()` so the test cannot pass by inspecting
    /// fewer slots than the set has.
    #[test]
    fn every_role_filled_by_the_merge_is_labelled_with_its_source() {
        let empty = MaterialTextureSet::<Option<byroredux_core::string::FixedString>>::default();
        let mut material = ImportedMaterial::default();
        // Fill every role, the way a sidecar that populated everything would.
        let path = interned("textures/probe.dds");
        material.textures = empty.map_ref(|_| Some(path));

        record_external_texture_sources(&mut material, &empty, ImportedTextureSource::Bgsm);

        let mut seen = 0usize;
        for (role, source) in material.texture_sources.roles() {
            assert_eq!(
                *source,
                ImportedTextureSource::Bgsm,
                "role `{role}` was filled by the merge but kept its default                  provenance — the walk skipped it"
            );
            seen += 1;
        }
        assert_eq!(
            seen,
            material.texture_sources.values().count(),
            "the provenance walk must visit every slot the set carries"
        );
        assert!(
            seen >= 26,
            "expected at least 22 roles + 4 decals, saw {seen}"
        );
    }

    /// A role the NIF already carried keeps `NifTextureSet`: inline values win
    /// the merge, and provenance has to say so. Without this, the test above
    /// would also pass for a walk that blindly stamped every slot.
    #[test]
    fn roles_the_nif_already_filled_keep_their_own_provenance() {
        let path = interned("textures/from_nif.dds");
        let mut before =
            MaterialTextureSet::<Option<byroredux_core::string::FixedString>>::default();
        before.base_color = Some(path);
        before.decals[2] = Some(path);

        let mut material = ImportedMaterial::default();
        material.textures = before.map_ref(|slot| slot.or(Some(path)));

        record_external_texture_sources(&mut material, &before, ImportedTextureSource::Bgem);

        assert_eq!(
            material.texture_sources.base_color,
            ImportedTextureSource::NifTextureSet,
            "a role the NIF filled must not be relabelled by the merge"
        );
        assert_eq!(
            material.texture_sources.decals[2],
            ImportedTextureSource::NifTextureSet,
            "the decals array follows the same precedence as the named roles"
        );
        assert_eq!(
            material.texture_sources.normal,
            ImportedTextureSource::Bgem,
            "a role only the sidecar filled must carry the sidecar's source"
        );
        assert_eq!(
            material.texture_sources.decals[0],
            ImportedTextureSource::Bgem,
            "an unfilled decal slot the sidecar populated takes the sidecar's source"
        );
    }
}
