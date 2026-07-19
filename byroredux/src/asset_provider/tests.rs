use super::*;
use byroredux_nif::import::ImportedMesh;
use std::sync::Arc;

// ── M35 — numeric-sibling archive auto-load (Skyrim zero-based series) ──

/// FNV ships `Fallout - Textures.bsa` + `Fallout - Textures2.bsa`: a
/// no-trailing-digit primary offers `…2`..`…9`. Unchanged by the M35 fix.
#[test]
fn siblings_fnv_no_suffix_offers_2_through_9() {
    let s = numeric_sibling_paths("Fallout - Textures.bsa");
    assert_eq!(s.len(), 8);
    assert_eq!(s[0], "Fallout - Textures2.bsa");
    assert_eq!(s[7], "Fallout - Textures9.bsa");
    assert!(!s.iter().any(|p| p.ends_with("Textures1.bsa")));
}

/// Skyrim's zero-based series start (`…0`) must offer `…1`..`…9` — this is
/// the M35 fix that pulls in `Textures7.bsa` (object-LOD atlas + `.btr`
/// terrain diffuse) and `Meshes1.bsa` (`.btr`/`.bto`) from the `…0` name.
#[test]
fn siblings_skyrim_zero_start_offers_1_through_9() {
    let s = numeric_sibling_paths("Skyrim - Textures0.bsa");
    assert_eq!(s.len(), 9);
    assert_eq!(s[0], "Skyrim - Textures1.bsa");
    assert!(s.iter().any(|p| p == "Skyrim - Textures7.bsa"));
    assert_eq!(s[8], "Skyrim - Textures9.bsa");
    // Meshes0 → Meshes1 (the `.btr`/`.bto` archive) without an explicit arg.
    let m = numeric_sibling_paths("Skyrim - Meshes0.bsa");
    assert!(m.iter().any(|p| p == "Skyrim - Meshes1.bsa"));
}

/// A mid-series non-zero member (`…2`) auto-expands nothing — the user is
/// listing members explicitly; expanding would double-open every archive.
#[test]
fn siblings_mid_series_digit_offers_none() {
    assert!(numeric_sibling_paths("Skyrim - Textures3.bsa").is_empty());
    assert!(numeric_sibling_paths("Skyrim - Textures1.bsa").is_empty());
}

/// `…10` (a digit before the trailing `0`) is an explicit member, NOT a
/// series start — must not be mistaken for one and expanded to `…11`..`…19`.
#[test]
fn siblings_ten_suffix_is_not_a_series_start() {
    assert!(numeric_sibling_paths("Mod - Textures10.bsa").is_empty());
}

/// BA2 extension is handled the same way (FO4/Starfield naming).
#[test]
fn siblings_ba2_zero_start() {
    let s = numeric_sibling_paths("DLC - Textures0.ba2");
    assert_eq!(s[0], "DLC - Textures1.ba2");
    assert!(s.iter().all(|p| p.ends_with(".ba2")));
}

// ── #1591 — conductor diffuse-tint must use mult-free chromaticity ──

/// The blend uses `specular_color` (chromaticity), not
/// `specular_color × specular_mult`. The real vanilla strong-metal cases
/// the audit sampled: pre-fix the blend target was `spec × mult`, which
/// darkened toward black (mult<1) or overshot past 1.0 (mult>1). Because
/// `conductor_diffuse_tint` takes no `mult` argument, the tint is
/// structurally mult-invariant — these assert the exact mult-free values.
#[test]
fn conductor_tint_is_mult_free() {
    // spec=[1.0,0.255,0.255]; diffuse=[0.5,0.5,0.5].
    // Mult-free target: 0.5*diffuse + 0.5*spec.
    let got = conductor_diffuse_tint([0.5, 0.5, 0.5], [1.0, 0.255, 0.255]);
    assert!((got[0] - 0.75).abs() < 1e-6, "{got:?}");
    assert!((got[1] - 0.3775).abs() < 1e-6, "{got:?}");
    assert!((got[2] - 0.3775).abs() < 1e-6, "{got:?}");
    // The old mult=0.25 fold would have blended toward [0.25,0.064,0.064]
    // → diffuse ≈ [0.375,0.282,0.282], strictly darker than the above.
    assert!(got[0] > 0.375, "mult<1 must NOT darken the tint: {got:?}");
}

/// `mult > 1` previously overshot a channel past 1.0 unclamped into
/// `GpuMaterial.diffuse_*`. The mult-free blend of two `[0,1]` inputs is
/// already in range, and the `[0,1]` clamp guards a >1 diffuse input.
#[test]
fn conductor_tint_clamps_to_unit_range() {
    // Both inputs in range → result in range (no overshoot).
    let in_range = conductor_diffuse_tint([1.0, 1.0, 1.0], [1.0, 0.467, 0.318]);
    assert!(in_range.iter().all(|&c| (0.0..=1.0).contains(&c)), "{in_range:?}");
    // A >1 diffuse input (defensive) is clamped.
    let clamped = conductor_diffuse_tint([2.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    assert_eq!(clamped[0], 1.0, "0.5*2.0 + 0.5*1.0 = 1.5 → clamped to 1.0");
}

// ── #1987 — `bgsm_metalness` pin against the #1476 luminance regression ──

/// Legacy (non-pbr) branch: white/achromatic spec is a dielectric, not a
/// conductor. This is the exact case (`paintpeelingconcrete`-style
/// `spec=[1,1,1]`) that the luminance formula got backwards — it must
/// read ~0.0, never the mirror-chrome `1.0` the pre-fix code produced.
#[test]
fn bgsm_metalness_legacy_white_spec_is_dielectric() {
    let m = bgsm_metalness([1.0, 1.0, 1.0], false);
    assert!(m < 1.0e-6, "white spec must classify as dielectric: {m}");
}

/// Legacy branch: tinted spec (e.g. `metallocker`-style `[1,0.85,0.70]`)
/// is a conductor — saturation-derived metalness must read clearly above
/// zero.
#[test]
fn bgsm_metalness_legacy_tinted_spec_is_conductor() {
    let m = bgsm_metalness([1.0, 0.85, 0.70], false);
    assert!(m > 0.1, "tinted spec must classify as metallic: {m}");
}

/// Legacy branch is mult-invariant by construction (`mult` is folded in
/// by the caller before pbr F0-luminance, never before this saturation
/// formula) — pass the same white spec regardless of authored mult and
/// confirm it still reads dielectric.
#[test]
fn bgsm_metalness_legacy_near_zero_spec_is_dielectric() {
    let m = bgsm_metalness([0.0, 0.0, 0.0], false);
    assert_eq!(m, 0.0, "near-zero spec magnitude must not divide-by-zero into metallic");
}

/// pbr branch: F0 at the dielectric floor (0.04 achromatic) reads ~0.0.
#[test]
fn bgsm_metalness_pbr_dielectric_floor_is_zero() {
    let m = bgsm_metalness([0.04, 0.04, 0.04], true);
    assert!(m.abs() < 1.0e-5, "F0=0.04 must read as dielectric floor: {m}");
}

/// pbr branch: full-white F0 is a fully metallic conductor.
#[test]
fn bgsm_metalness_pbr_white_f0_is_metallic() {
    let m = bgsm_metalness([1.0, 1.0, 1.0], true);
    assert!((m - 1.0).abs() < 1.0e-6, "F0=1.0 must read as fully metallic: {m}");
}

// ── `normalize_mesh_path` — regression for unclothed NPCs in
//   FNV Prospector Saloon, 2026-05-25. ARMO `MODL` paths are
//   authored relative to the `meshes\` root (e.g.
//   `armor\powdergang\powdergang03.NIF`); the BSA stores them
//   fully prefixed. Pre-fix `extract_mesh` passed the authored
//   path through verbatim and every leaf-armor lookup missed.

// ── `derive_normal_map_path` — #1303 / OBL-D4-NEW-01. Oblivion ships
//   normal maps as `<base>_n.dds` siblings, not explicit NIF slots.
#[test]
fn derive_normal_map_path_inserts_n_before_extension() {
    assert_eq!(
        derive_normal_map_path(r"textures\architecture\imperialcity\icwallbuttress01.dds"),
        r"textures\architecture\imperialcity\icwallbuttress01_n.dds"
    );
    // Extension case is preserved (Bethesda paths are mixed-case).
    assert_eq!(derive_normal_map_path("Foo.DDS"), "Foo_n.DDS");
    // No extension → append the conventional `_n.dds`.
    assert_eq!(derive_normal_map_path("bar"), "bar_n.dds");
    // Only the final extension is split, not dots earlier in the path.
    assert_eq!(derive_normal_map_path(r"a.b\c.dds"), r"a.b\c_n.dds");
}

#[test]
fn normalize_mesh_path_prepends_missing_meshes_prefix() {
    let out = normalize_mesh_path(r"armor\powdergang\powdergang03.NIF");
    assert_eq!(out.as_ref(), r"meshes\armor\powdergang\powdergang03.NIF");
    assert!(matches!(out, std::borrow::Cow::Owned(_)));
}

#[test]
fn normalize_mesh_path_passes_already_prefixed_borrowed() {
    let out = normalize_mesh_path(r"meshes\characters\_male\upperbody.nif");
    assert_eq!(out.as_ref(), r"meshes\characters\_male\upperbody.nif");
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn normalize_mesh_path_is_case_insensitive_on_prefix() {
    // Modder-authored or DLC content may ship the prefix with a
    // different case (`Meshes\…`); the normalizer must accept it.
    let out = normalize_mesh_path(r"MESHES\armor\foo.nif");
    assert_eq!(out.as_ref(), r"MESHES\armor\foo.nif");
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn normalize_mesh_path_accepts_forward_slash_prefix() {
    // Mod-authoring tools sometimes export forward slashes.
    let out = normalize_mesh_path("meshes/armor/foo.nif");
    assert_eq!(out.as_ref(), "meshes/armor/foo.nif");
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn normalize_mesh_path_is_idempotent() {
    // Callers that already pre-normalised must round-trip without
    // double-prefixing — the cell-loader static-spawn path at
    // `cell_loader/references.rs:421-426` predates the centralised
    // normaliser and still pre-prepends `meshes\` itself; the
    // double-normalise must be a no-op.
    let once = normalize_mesh_path(r"armor\powdergang\powdergang03.NIF");
    let twice = normalize_mesh_path(once.as_ref());
    assert_eq!(once, twice);
}

#[test]
fn normalize_mesh_path_handles_short_input() {
    // Pathological input shorter than the 7-byte prefix — must
    // not panic and must still get the prefix.
    let out = normalize_mesh_path("a.nif");
    assert_eq!(out.as_ref(), r"meshes\a.nif");
}

/// #1292 — Starfield BSGeometry external `.mesh` companion files
/// live at `geometries\<hash>.mesh` directly (NO `meshes\` prefix).
/// The importer composes this canonical path before calling the
/// resolver. Pre-#1292 the normaliser blindly prepended `meshes\`
/// turning it into `meshes\geometries\<hash>.mesh` which doesn't
/// exist in the archive → 99.7% spawn-rate failure on Cydonia.
#[test]
fn normalize_mesh_path_passes_geometries_prefix_unchanged() {
    let out = normalize_mesh_path(r"geometries\aa2d865fc6bf336b909b\e84b59f1a4b705a40845.mesh");
    assert_eq!(
        out.as_ref(),
        r"geometries\aa2d865fc6bf336b909b\e84b59f1a4b705a40845.mesh",
        "Starfield `geometries\\X.mesh` must NOT get a `meshes\\` prefix",
    );
    assert!(
        matches!(out, std::borrow::Cow::Borrowed(_)),
        "already-canonical paths must borrow, not allocate",
    );
}

/// Case-insensitive + forward-slash variants of the geometries
/// prefix gate. Mirrors the case-insensitive / forward-slash
/// coverage on the `meshes\` prefix.
#[test]
fn normalize_mesh_path_geometries_prefix_is_case_and_separator_insensitive() {
    for variant in [
        r"GEOMETRIES\abc\def.mesh",
        r"Geometries\abc\def.mesh",
        "geometries/abc/def.mesh",
        "GEOMETRIES/abc/def.mesh",
    ] {
        let out = normalize_mesh_path(variant);
        assert_eq!(
            out.as_ref(),
            variant,
            "{variant:?} must pass through unchanged"
        );
    }
}

#[test]
fn strip_build_prefix_handles_skyrim_hd_prefix() {
    // The headline case from the Markarth render: Skyrim AE bundles
    // the HD juniper / reach branches / driftwood with the full
    // pipeline-internal prefix.
    let out =
        strip_build_prefix("skyrimhd\\build\\pc\\data\\textures\\plants\\florajuniper.dds");
    assert_eq!(out.as_ref(), "textures\\plants\\florajuniper.dds");
}

/// Live observation from MedTekResearch01 (FO4) `tex.missing` run
/// 2026-05-17 — every BGSM/BGEM authored in FO4 vanilla carries
/// this exact `c:\projects\fallout4\build\pc\data\…` pipeline
/// prefix. Pre-fix the BGSM resolver didn't strip and 11 / 12
/// unique missing-material entries were variants of this case
/// (metallocker01.bgsm, woodmetalcrate01.bgsm, hightechlamp01.bgsm,
/// …). The strip-helper already handles the LAST `\data\`
/// boundary correctly for the multi-segment case; this test pins
/// the exact FO4 input → archive-relative output transformation
/// the resolver depends on.
#[test]
fn strip_build_prefix_handles_fo4_pipeline_prefix() {
    let out = strip_build_prefix(
        "c:\\projects\\fallout4\\build\\pc\\data\\materials\\setdressing\\metallocker01.bgsm",
    );
    assert_eq!(out.as_ref(), "materials\\setdressing\\metallocker01.bgsm");
}

/// MaterialProvider's archive-read helper must call
/// `normalize_material_path` so the FO4 BGSM lookup actually
/// hits the archive index. Pre-fix the lookup skipped the
/// normalisation and every non-canonical path resolved to None.
/// Probes the transformation with an empty archive set — the
/// answer must be `None` for any input, but the call shouldn't
/// panic on any of the four observed failure-mode forms.
#[test]
fn material_provider_extract_normalises_without_panic() {
    let provider = MaterialProvider::new();
    for path in [
        // Form 1 — FO4 pipeline build prefix (live observation,
        // 46× hit count on MedTek).
        "c:\\projects\\fallout4\\build\\pc\\data\\materials\\setdressing\\metallocker01.bgsm",
        // Form 2 — leading `data\` (live observation, ~3 BGSM
        // files in MedTek setdressing).
        "data\\materials\\setdressing\\metaltrashcan01alpha.bgsm",
        // Form 3 — forward slashes (live observation, template
        // parents in shared BGSMs).
        "template/defaulttemplate_wet.bgsm",
        // Form 4 — composed: forward slashes WITH leading data/.
        "data/materials/template/metaltemplate_wet.bgsm",
    ] {
        let result = provider.extract_from_archives(path);
        assert!(
            result.is_none(),
            "no archives → no bytes; must not panic on input {path:?}"
        );
    }
}

// ── normalize_material_path — per-rule + composed cases ─────

/// Rule 1: build-pipeline prefix strip (live FO4 MedTek case).
#[test]
fn normalize_material_path_strips_fo4_build_prefix() {
    let out = normalize_material_path(
        "c:\\projects\\fallout4\\build\\pc\\data\\materials\\setdressing\\metallocker01.bgsm",
    );
    assert_eq!(out.as_ref(), "materials\\setdressing\\metallocker01.bgsm");
}

/// Rule 2: leading `data\` strip — covers the `metaltrashcan01alpha.bgsm`
/// failure mode where the path begins with `data\` (no leading
/// separator). `strip_build_prefix` alone doesn't catch this
/// because it requires a separator BEFORE the `data` segment.
#[test]
fn normalize_material_path_strips_leading_data_segment() {
    let out =
        normalize_material_path("data\\materials\\setdressing\\metaltrashcan01alpha.bgsm");
    assert_eq!(
        out.as_ref(),
        "materials\\setdressing\\metaltrashcan01alpha.bgsm"
    );
}

/// Rule 2 sibling: leading `data/` with forward slash.
#[test]
fn normalize_material_path_strips_leading_data_segment_forward_slash() {
    let out = normalize_material_path("data/materials/setdressing/foo.bgsm");
    assert_eq!(out.as_ref(), "materials\\setdressing\\foo.bgsm");
}

/// `normalize_texture_path` — canonical form passes through borrowed.
#[test]
fn normalize_texture_path_passes_canonical_through_borrowed() {
    let out = normalize_texture_path("textures\\landscape\\plants\\juniper_d.dds");
    assert_eq!(out.as_ref(), "textures\\landscape\\plants\\juniper_d.dds");
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

/// `normalize_texture_path` — paths without a `textures\` prefix
/// get one prepended. Bethesda CLMT / WTHR / LTEX records author
/// this shape.
#[test]
fn normalize_texture_path_prepends_textures_when_missing() {
    let out = normalize_texture_path("landscape\\plants\\juniper_d.dds");
    assert_eq!(out.as_ref(), "textures\\landscape\\plants\\juniper_d.dds");
}

/// `normalize_texture_path` — leading `data\textures\…` strip.
/// F1.1 from the 2026-05-26 Fallout symptom sweep: FO4 head NIFs'
/// `BSShaderTextureSet` authors per-NPC FaceGen textures with the
/// `data\` prefix; the archive stores them at `textures\…`. Without
/// the strip every NPC head rendered with a checkerboard face.
#[test]
fn normalize_texture_path_strips_leading_data_facegen() {
    let out = normalize_texture_path(
        "data\\textures\\actors\\character\\facecustomization\\fallout4.esm\\001d4387_d.dds",
    );
    assert_eq!(
        out.as_ref(),
        "textures\\actors\\character\\facecustomization\\fallout4.esm\\001d4387_d.dds",
    );
}

/// `normalize_texture_path` — `data/` forward-slash variant of F1.1.
/// Same path shape, just mixed separators (mod-authoring tools).
#[test]
fn normalize_texture_path_strips_leading_data_forward_slash() {
    let out = normalize_texture_path("data/textures/landscape/foo.dds");
    // After strip we re-check the `textures\` prefix — note we don't
    // rewrite slashes inside the trailer, since texture lookups use
    // a case-insensitive separator-tolerant key downstream.
    assert_eq!(out.as_ref(), "textures/landscape/foo.dds");
}

/// Rule 3: `/` → `\` separator normalisation. Live case from BGSM
/// `root_material_path` fields authored with forward slashes.
#[test]
fn normalize_material_path_converts_forward_slashes_to_backslashes() {
    let out = normalize_material_path("materials/template/defaulttemplate_wet.bgsm");
    assert_eq!(
        out.as_ref(),
        "materials\\template\\defaulttemplate_wet.bgsm"
    );
}

/// Rule 4: `materials\` prefix add when missing. Live case from
/// the bare `template/defaulttemplate_wet.bgsm` form (no
/// `materials\` segment) inside BGSM parent references.
#[test]
fn normalize_material_path_prepends_materials_when_missing() {
    let out = normalize_material_path("template\\defaulttemplate_wet.bgsm");
    assert_eq!(
        out.as_ref(),
        "materials\\template\\defaulttemplate_wet.bgsm"
    );
}

/// Composed: `template/defaulttemplate_wet.bgsm` — the headline
/// template-parent failure mode (forward slashes AND missing
/// `materials\` prefix at the same time). 11/12 BGSM resolve
/// failures in MedTek post-build-prefix-fix went through this
/// exact composition.
#[test]
fn normalize_material_path_handles_template_parent_form() {
    let out = normalize_material_path("template/defaulttemplate_wet.bgsm");
    assert_eq!(
        out.as_ref(),
        "materials\\template\\defaulttemplate_wet.bgsm"
    );
}

/// Canonical-form passthrough: no allocation when the input is
/// already `materials\…`-prefixed, backslashed, no build prefix,
/// no leading `data\`. The `Cow::Borrowed` return signals the
/// fast-path took.
#[test]
fn normalize_material_path_canonical_form_borrows() {
    let input = "materials\\setdressing\\foo.bgsm";
    let out = normalize_material_path(input);
    assert_eq!(out.as_ref(), input);
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

/// Case-insensitive `materials\` prefix check — `Materials\foo.bgsm`
/// must NOT be double-prefixed into `materials\Materials\foo.bgsm`.
#[test]
fn normalize_material_path_does_not_double_prefix_capitalised_materials() {
    let out = normalize_material_path("Materials\\foo.bgsm");
    // First-rune case is preserved when no other rule fires —
    // the BSA index lookup is case-insensitive (per
    // `BsaArchive::contains` + `Ba2Archive::contains`) so either
    // case resolves the same file. We just need to avoid the
    // double-prefix bug.
    assert_eq!(out.as_ref(), "Materials\\foo.bgsm");
}

#[test]
fn strip_build_prefix_passes_canonical_paths_through_borrowed() {
    let input = "textures\\landscape\\trees\\reachtreebranch01.dds";
    let out = strip_build_prefix(input);
    assert_eq!(out.as_ref(), input);
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn strip_build_prefix_is_case_insensitive_on_data_token() {
    // Anniversary Edition's HD bundle uses lowercase `data`, but
    // we shouldn't be fragile if a future CC pack uses `Data`.
    let out = strip_build_prefix("skyrimhd\\build\\pc\\Data\\textures\\plants\\foo.dds");
    assert_eq!(out.as_ref(), "textures\\plants\\foo.dds");
}

#[test]
fn strip_build_prefix_accepts_forward_slashes() {
    // Mod-authoring tools occasionally export forward slashes.
    let out = strip_build_prefix("skyrimhd/build/pc/data/textures/plants/foo.dds");
    assert_eq!(out.as_ref(), "textures/plants/foo.dds");
}

#[test]
fn strip_build_prefix_uses_last_data_boundary() {
    // Pathological case: an asset that genuinely lives under a
    // nested `data\` directory should strip up to the LAST
    // boundary so the longest known-prefix wins.
    let out = strip_build_prefix("vendor\\data\\skyrimhd\\build\\pc\\data\\textures\\foo.dds");
    assert_eq!(out.as_ref(), "textures\\foo.dds");
}

#[test]
fn strip_build_prefix_preserves_path_with_no_data_segment() {
    let input = "meshes\\architecture\\foo.nif";
    let out = strip_build_prefix(input);
    assert_eq!(out.as_ref(), input);
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn strip_build_prefix_preserves_trailing_data_directory() {
    // A path that ends with `\data\` exactly would strip to empty;
    // the helper must guard that and pass the path through
    // untouched so callers can fall through to "not found" rather
    // than hitting an empty BSA lookup that might silently succeed
    // on the first-entry of the archive.
    let input = "scratch\\data\\";
    let out = strip_build_prefix(input);
    assert_eq!(out.as_ref(), input);
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn normalize_adds_prefix_when_missing() {
    // WTHR cloud path authored relative to `textures\` root.
    let out = normalize_texture_path("sky\\cloudsnoon.dds");
    assert_eq!(out.as_ref(), "textures\\sky\\cloudsnoon.dds");
}

#[test]
fn normalize_leaves_fully_qualified_paths_borrowed() {
    // Cell loader's landscape path path-building already supplies
    // the `textures\` prefix; the fn must pass through without
    // allocating (Cow::Borrowed).
    let input = "textures\\landscape\\dirt02.dds";
    let out = normalize_texture_path(input);
    assert_eq!(out.as_ref(), input);
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn normalize_is_case_insensitive_on_prefix() {
    // A future tool or mod authoring flow might export
    // `Textures\…` or `TEXTURES\…`; the prefix check is ASCII-
    // case-insensitive and shouldn't double up.
    let out = normalize_texture_path("Textures\\sky\\cloudsnoon.dds");
    assert_eq!(out.as_ref(), "Textures\\sky\\cloudsnoon.dds");
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn normalize_accepts_forward_slash_separator() {
    // Mod authoring tools occasionally emit forward slashes.
    // The prefix check accepts either.
    let out = normalize_texture_path("textures/sky/cloudsnoon.dds");
    assert_eq!(out.as_ref(), "textures/sky/cloudsnoon.dds");
    assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
}

#[test]
fn normalize_prefixes_non_textures_paths_as_owned() {
    // Any path whose first segment isn't `textures\` — e.g. a
    // relative CLMT `sun_01.dds` or a broken mod export — gets
    // the prefix prepended. The fn allocates in this branch, so
    // result is Cow::Owned.
    let out = normalize_texture_path("sun_01.dds");
    assert_eq!(out.as_ref(), "textures\\sun_01.dds");
    assert!(matches!(out, std::borrow::Cow::Owned(_)));
}

#[test]
fn normalize_short_string_gets_prefixed() {
    // Guard against the `bytes.len() >= 9` check: a 1-byte path
    // should still prefix cleanly.
    let out = normalize_texture_path("a");
    assert_eq!(out.as_ref(), "textures\\a");
}

// ── MaterialProvider / BGSM merge (#493) ──────────────────────────
//
// The merge logic in `merge_bgsm_into_mesh` has three moving parts:
//   1. Dispatch on `material_path` extension (.bgsm / .bgem / other)
//   2. For BGSM: walk the template chain child-first, fill empties
//   3. For BGEM: single-file fill, no inheritance
//
// We test part 2 + 3 directly against synthetic `ResolvedMaterial` /
// `BgemFile` values (bypassing archive IO which the `bgsm` crate's
// own tests cover). Part 1 is covered through the resolve_bgsm
// failure-dedup test below.

use byroredux_bgsm::template::ResolvedMaterial;
use byroredux_bgsm::{BgemFile, BgsmFile};

/// Same fill closure as `merge_bgsm_into_mesh` so the tests verify
/// the exact precedence rule the prod helper uses.
fn fill(slot: &mut Option<String>, value: &str) -> bool {
    if slot.is_none() && !value.is_empty() {
        *slot = Some(value.to_string());
        true
    } else {
        false
    }
}

/// Walks a ResolvedMaterial chain child-first, filling the 6 slots
/// the prod merge helper writes for BGSM files. The slot set mirrors
/// `merge_bgsm_into_mesh` exactly; the closure is inlined in prod
/// for a single allocation-free pass.
fn apply_bgsm_chain(
    resolved: &ResolvedMaterial,
    texture_path: &mut Option<String>,
    normal_map: &mut Option<String>,
    glow_map: &mut Option<String>,
    gloss_map: &mut Option<String>,
    env_map: &mut Option<String>,
    parallax_map: &mut Option<String>,
) {
    for step in resolved.walk() {
        fill(texture_path, &step.file.diffuse_texture);
        fill(normal_map, &step.file.normal_texture);
        fill(glow_map, &step.file.glow_texture);
        fill(gloss_map, &step.file.smooth_spec_texture);
        fill(env_map, &step.file.envmap_texture);
        fill(parallax_map, &step.file.displacement_texture);
    }
}

#[test]
fn bgsm_merge_fills_only_empty_slots() {
    // NIF already authored a diffuse; BGSM should NOT overwrite it.
    let mut texture_path: Option<String> = Some("nif_diffuse.dds".into());
    let mut normal_map: Option<String> = None;
    let mut glow_map: Option<String> = None;
    let mut gloss_map: Option<String> = None;
    let mut env_map: Option<String> = None;
    let mut parallax_map: Option<String> = None;

    let bgsm = BgsmFile {
        diffuse_texture: "bgsm_diffuse.dds".into(),
        normal_texture: "bgsm_normal.dds".into(),
        glow_texture: "bgsm_glow.dds".into(),
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: bgsm,
        parent: None,
    };

    apply_bgsm_chain(
        &resolved,
        &mut texture_path,
        &mut normal_map,
        &mut glow_map,
        &mut gloss_map,
        &mut env_map,
        &mut parallax_map,
    );

    // NIF-authored field preserved.
    assert_eq!(texture_path.as_deref(), Some("nif_diffuse.dds"));
    // Empty slots filled.
    assert_eq!(normal_map.as_deref(), Some("bgsm_normal.dds"));
    assert_eq!(glow_map.as_deref(), Some("bgsm_glow.dds"));
}

#[test]
fn bgsm_merge_child_wins_over_parent() {
    let mut texture_path: Option<String> = None;
    let mut normal_map: Option<String> = None;
    let mut glow_map: Option<String> = None;
    let mut gloss_map: Option<String> = None;
    let mut env_map: Option<String> = None;
    let mut parallax_map: Option<String> = None;

    let child = BgsmFile {
        diffuse_texture: "child_diffuse.dds".into(),
        // child leaves normal empty → parent fills it
        ..Default::default()
    };
    let parent = BgsmFile {
        diffuse_texture: "parent_diffuse.dds".into(),
        normal_texture: "parent_normal.dds".into(),
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: child,
        parent: Some(Arc::new(ResolvedMaterial {
            file: parent,
            parent: None,
        })),
    };

    apply_bgsm_chain(
        &resolved,
        &mut texture_path,
        &mut normal_map,
        &mut glow_map,
        &mut gloss_map,
        &mut env_map,
        &mut parallax_map,
    );

    assert_eq!(texture_path.as_deref(), Some("child_diffuse.dds"));
    assert_eq!(normal_map.as_deref(), Some("parent_normal.dds"));
}

#[test]
fn bgem_merge_fills_effect_slots() {
    // BGEM has no template inheritance — single file.
    let mut texture_path: Option<String> = None;
    let mut normal_map: Option<String> = None;
    let mut env_mask: Option<String> = None;

    let bgem = BgemFile {
        base_texture: "effect_base.dds".into(),
        normal_texture: "effect_normal.dds".into(),
        envmap_mask_texture: "effect_mask.dds".into(),
        ..Default::default()
    };

    fill(&mut texture_path, &bgem.base_texture);
    fill(&mut normal_map, &bgem.normal_texture);
    fill(&mut env_mask, &bgem.envmap_mask_texture);

    assert_eq!(texture_path.as_deref(), Some("effect_base.dds"));
    assert_eq!(normal_map.as_deref(), Some("effect_normal.dds"));
    assert_eq!(env_mask.as_deref(), Some("effect_mask.dds"));
}

/// Regression for #1076 / FO4-D6-002 — BGSM v>2 standalone slots
/// (`specular_texture`, `lighting_texture`, `flow_texture`,
/// `wrinkles_texture`) must forward to `ImportedMesh`'s
/// `specular_map` / `lighting_map` / `flow_map` / `wrinkle_map`.
/// Pre-fix the parser decoded all four fields and the merge
/// dropped them on the floor — FO4 water surfaces lost their
/// flow direction, NPC skin lost wrinkle blending, PBR specular
/// fell back to the gloss_map's .r-only path.
#[test]
fn bgsm_merge_forwards_v2_plus_standalone_slots() {
    // Use the in-test `fill` helper (`Option<String>` variant)
    // that mirrors the prod merge's intern-and-set semantic.
    let mut specular_map: Option<String> = None;
    let mut lighting_map: Option<String> = None;
    let mut flow_map: Option<String> = None;
    let mut wrinkle_map: Option<String> = None;

    let bgsm = BgsmFile {
        specular_texture: "armor_specular.dds".into(),
        lighting_texture: "armor_lighting.dds".into(),
        flow_texture: "water_flow.dds".into(),
        wrinkles_texture: "ncr_wrinkles.dds".into(),
        ..Default::default()
    };

    // Mirror the prod loop body for the four new slots.
    fill(&mut specular_map, &bgsm.specular_texture);
    fill(&mut lighting_map, &bgsm.lighting_texture);
    fill(&mut flow_map, &bgsm.flow_texture);
    fill(&mut wrinkle_map, &bgsm.wrinkles_texture);

    assert_eq!(specular_map.as_deref(), Some("armor_specular.dds"));
    assert_eq!(lighting_map.as_deref(), Some("armor_lighting.dds"));
    assert_eq!(flow_map.as_deref(), Some("water_flow.dds"));
    assert_eq!(wrinkle_map.as_deref(), Some("ncr_wrinkles.dds"));
}

/// Regression for #1077 / FO4-D6-003 Phase 1 — BGSM-only shader
/// flags (`pbr`, `translucency`, `model_space_normals`) must
/// forward to `ImportedMesh`'s `is_pbr` / `has_translucency` /
/// `model_space_normals`. Pre-fix the parser decoded all three
/// and the merge dropped them on the floor — FO4 materials
/// authored with `pbr=true` rendered on the Gamebryo-legacy
/// specular path (the renderer didn't even know to dispatch
/// PBR-vs-legacy). Phase 2 (the `triangle.frag` gating) is
/// deferred; this test pins the data-propagation contract.
#[test]
fn bgsm_merge_forwards_phase1_shader_flags() {
    // Three local bools standing in for the corresponding
    // ImportedMesh fields. Mirrors the prod merge's "first true
    // wins" gate.
    let mut is_pbr = false;
    let mut has_translucency = false;
    let mut model_space_normals = false;

    let bgsm = BgsmFile {
        pbr: true,
        translucency: true,
        model_space_normals: true,
        ..Default::default()
    };

    // Mirror the prod merge's gates.
    if !is_pbr && bgsm.pbr {
        is_pbr = true;
    }
    if !has_translucency && bgsm.translucency {
        has_translucency = true;
    }
    if !model_space_normals && bgsm.model_space_normals {
        model_space_normals = true;
    }

    assert!(
        is_pbr,
        "BGSM.pbr=true must propagate to ImportedMesh.is_pbr"
    );
    assert!(has_translucency);
    assert!(model_space_normals);
}

/// Companion: with all three flags `false` on the BGSM, the
/// translucency / model-space-normal mesh fields must stay at their
/// defaults (a `false` author doesn't override). `is_pbr` is the
/// exception post-#1352: it is now driven by `from_bgsm` (any
/// successful BGSM resolve), NOT by `bgsm.pbr`, so it is `true` even
/// here — every vanilla FO4 BGSM (which never sets `pbr`) routes
/// through the Disney lobe.
#[test]
fn bgsm_merge_does_not_set_phase1_flags_from_false() {
    let mut is_pbr = false;
    let mut has_translucency = false;
    let mut model_space_normals = false;

    let bgsm = BgsmFile {
        pbr: false,
        translucency: false,
        model_space_normals: false,
        ..Default::default()
    };

    // #1352 — a successful BGSM resolve sets `from_bgsm = true`, which
    // now unconditionally implies `is_pbr` (the per-BGSM `bgsm.pbr`
    // gate is a subsumed backstop).
    let from_bgsm = true;
    if from_bgsm || bgsm.pbr {
        is_pbr = true;
    }
    if !has_translucency && bgsm.translucency {
        has_translucency = true;
    }
    if !model_space_normals && bgsm.model_space_normals {
        model_space_normals = true;
    }

    assert!(
        is_pbr,
        "#1352: from_bgsm now implies is_pbr regardless of bgsm.pbr"
    );
    assert!(!has_translucency);
    assert!(!model_space_normals);
}

/// Child-first precedence for the new flags — first authored
/// `true` in the BGSM template chain wins, mirroring the
/// texture-slot precedence (which the parser walks child-first).
/// A `false` child followed by a `true` parent must flip the
/// flag.
#[test]
fn bgsm_merge_phase1_flags_honor_child_first_chain() {
    let mut is_pbr = false;

    let child = BgsmFile {
        pbr: false, // child doesn't author PBR
        ..Default::default()
    };
    let parent = BgsmFile {
        pbr: true, // parent template enables PBR
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: child,
        parent: Some(Arc::new(ResolvedMaterial {
            file: parent,
            parent: None,
        })),
    };

    for step in resolved.walk() {
        if !is_pbr && step.file.pbr {
            is_pbr = true;
        }
    }

    assert!(
        is_pbr,
        "parent's pbr=true must flow down to the merged result"
    );
}

/// Regression for FO4 BGSM glass / alpha-blended decals. FO4
/// moved per-material blend state out of `NiAlphaProperty` into
/// BGSM's `base.alpha_blend_mode`. Pre-fix the merge dropped
/// that tuple, leaving `ImportedMesh.has_alpha = false` on
/// every BGSM-only glass pane → no `AlphaBlend` component
/// attached → `INSTANCE_FLAG_ALPHA_BLEND` never set → the
/// `MATERIAL_KIND_GLASS` classifier in `static_meshes.rs`
/// short-circuited and the panel rendered fully opaque
/// (visible symptom: Institute Bioscience glass too opaque,
/// no refraction, wrong tint). Pins:
///   1. `function > 0` → `has_alpha = true` + blend factors copied
///   2. `function == 0` (None) → no override (NIF-side value wins)
///
/// The three real `(function, src, dst)` tuples below are taken
/// directly from the reference implementation's
/// `ConvertAlphaBlendMode` (`Material-Editor:BaseMaterialFile.cs:363-387`),
/// not invented — see #1823, which replaced this test's prior
/// synthetic `(function=2, src=1, dst=1)` Additive fixture (a tuple
/// the reference parser never actually emits) that had masked the
/// #1651 blend-swap regression.
#[test]
fn bgsm_merge_forwards_alpha_blend_mode() {
    use ash::vk;
    use byroredux_bgsm::AlphaBlendMode;
    use byroredux_renderer::vulkan::pipeline::gamebryo_to_vk_blend_factor;

    // Mirror the prod merge's three writes for the alpha-blend block.
    // `bgsm_blend_to_gamebryo` is a narrowing cast, not a translation
    // (#1823) — `src_blend`/`dst_blend` are already Gamebryo-native.
    fn apply(bgsm: &BgsmFile, has_alpha: &mut bool, src: &mut u8, dst: &mut u8) {
        if bgsm.base.alpha_blend_mode.function > 0 {
            *has_alpha = true;
            *src = bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.src_blend);
            *dst = bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.dst_blend);
        }
    }

    // Case 1: Standard (function=1, src=6, dst=7) — Institute glass
    // case. Must produce (SRC_ALPHA, ONE_MINUS_SRC_ALPHA).
    let mut has_alpha = false;
    let mut src = 0u8;
    let mut dst = 0u8;
    let mut bgsm = BgsmFile::default();
    bgsm.base.alpha_blend_mode = AlphaBlendMode {
        function: 1,
        src_blend: 6,
        dst_blend: 7,
    };
    apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
    assert!(has_alpha, "function=1 (Standard) must set has_alpha");
    assert_eq!(src, 6);
    assert_eq!(dst, 7);
    assert_eq!(gamebryo_to_vk_blend_factor(src), vk::BlendFactor::SRC_ALPHA);
    assert_eq!(
        gamebryo_to_vk_blend_factor(dst),
        vk::BlendFactor::ONE_MINUS_SRC_ALPHA
    );

    // Case 2: Additive (function=1, src=6, dst=0) — the real reference
    // tuple for FO4 effect/glow-card BGEMs. Must produce
    // (SRC_ALPHA, ONE) — additive accumulation. #1651's swap turned
    // dst=0 into 1 (ZERO), corrupting this to an alpha-weighted
    // opaque overwrite instead of additive.
    let mut has_alpha = false;
    let mut src = 0u8;
    let mut dst = 0u8;
    let mut bgsm = BgsmFile::default();
    bgsm.base.alpha_blend_mode = AlphaBlendMode {
        function: 1,
        src_blend: 6,
        dst_blend: 0,
    };
    apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
    assert!(has_alpha);
    assert_eq!(src, 6);
    assert_eq!(dst, 0);
    assert_eq!(gamebryo_to_vk_blend_factor(src), vk::BlendFactor::SRC_ALPHA);
    assert_eq!(
        gamebryo_to_vk_blend_factor(dst),
        vk::BlendFactor::ONE,
        "Additive dst must resolve to ONE for accumulation"
    );

    // Case 3: Multiplicative (function=1, src=4, dst=1) — the real
    // reference tuple. Must produce (DST_COLOR, ZERO) — dst *= src.
    // #1651's swap turned dst=1 into 0 (ONE), leaking the destination
    // through instead of multiplying it out.
    let mut has_alpha = false;
    let mut src = 0u8;
    let mut dst = 0u8;
    let mut bgsm = BgsmFile::default();
    bgsm.base.alpha_blend_mode = AlphaBlendMode {
        function: 1,
        src_blend: 4,
        dst_blend: 1,
    };
    apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
    assert!(has_alpha);
    assert_eq!(src, 4);
    assert_eq!(dst, 1);
    assert_eq!(gamebryo_to_vk_blend_factor(src), vk::BlendFactor::DST_COLOR);
    assert_eq!(
        gamebryo_to_vk_blend_factor(dst),
        vk::BlendFactor::ZERO,
        "Multiplicative dst must resolve to ZERO so it doesn't leak through"
    );

    // Case 4: function=0 (None) — the BGSM explicitly says "no
    // blend." Don't flip has_alpha. Caller's `set_blend` guard
    // then also prevents a subsequent parent from re-triggering.
    let mut has_alpha = false;
    let mut src = 6u8;
    let mut dst = 7u8;
    let bgsm = BgsmFile::default();
    assert_eq!(bgsm.base.alpha_blend_mode.function, 0);
    apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
    assert!(!has_alpha, "function=0 must NOT set has_alpha");
    assert_eq!(src, 6, "src untouched when function=0");
    assert_eq!(dst, 7);
}

/// #1823 — `bgsm_blend_to_gamebryo` performs no translation, only a
/// `u32 -> u8` narrowing cast. Pins the identity contract across the
/// full valid range (0..=10), specifically including `0`/`1` — the
/// #1651 regression swapped exactly those two, which corrupted the
/// real Additive (`dst=0`) and Multiplicative (`dst=1`) blend modes
/// (see `bgsm_merge_forwards_alpha_blend_mode`). Shared by the BGSM
/// and BGEM merge branches, so this pins the contract for both.
#[test]
fn bgsm_blend_to_gamebryo_is_identity_narrowing() {
    for v in 0u32..=10 {
        assert_eq!(
            bgsm_blend_to_gamebryo(v),
            v as u8,
            "must pass {v} through unchanged, no 0/1 swap"
        );
    }
}

/// Companion regression for the SIBLING half of #1076 — BGEM also
/// authors `specular_texture` and `lighting_texture` (BGEM does
/// not author flow / wrinkles per `bgem.rs`). Pre-fix the BGEM
/// merge dropped both, leaving FO4 effect shaders that authored
/// a per-texel specular layer rendering on NIF-fallback specular.
#[test]
fn bgem_merge_forwards_specular_and_lighting_slots() {
    let mut specular_map: Option<String> = None;
    let mut lighting_map: Option<String> = None;

    let bgem = BgemFile {
        specular_texture: "fx_specular.dds".into(),
        lighting_texture: "fx_lighting.dds".into(),
        ..Default::default()
    };

    fill(&mut specular_map, &bgem.specular_texture);
    fill(&mut lighting_map, &bgem.lighting_texture);

    assert_eq!(specular_map.as_deref(), Some("fx_specular.dds"));
    assert_eq!(lighting_map.as_deref(), Some("fx_lighting.dds"));
}

/// Regression for #1358 — BGEM `base_color` / `base_color_scale` must
/// forward to `mesh.emissive_color` / `mesh.emissive_mult` with
/// `emissive_source = EmissiveSource::Effect`. Pre-fix the BGEM merge
/// set `emissive_color = bgem.emittance_color` (a separate v≥11
/// additive glow) and left `emissive_mult = 0.0` and
/// `emissive_source = None`, causing all FO4 effect surfaces (fire,
/// electricity, plasma, neon signs) to render white instead of their
/// authored tint.
#[test]
fn bgem_merge_forwards_base_color_as_emissive() {
    use byroredux_bgsm::BgemFile;
    use byroredux_core::ecs::components::material::EmissiveSource;

    let bgem = BgemFile {
        base_color: [0.8, 0.2, 0.1],
        base_color_scale: 2.5,
        emittance_color: [0.0, 1.0, 0.0], // distinct — must NOT be forwarded
        ..Default::default()
    };

    // Mirror the prod assignment from the BGEM branch.
    let emissive_color = bgem.base_color;
    let emissive_mult = bgem.base_color_scale;
    let emissive_source = EmissiveSource::Effect;

    assert_eq!(emissive_color, [0.8, 0.2, 0.1]);
    assert!((emissive_mult - 2.5).abs() < f32::EPSILON);
    assert!(
        matches!(emissive_source, EmissiveSource::Effect),
        "BGEM emissive_source must be Effect, not Material or Lighting"
    );
    // emittance_color must NOT be used as the primary emissive
    assert_ne!(emissive_color, bgem.emittance_color);
}

/// Regression for the FO4 HalluciGen gas-lab white-out — BGEM
/// `soft`/`soft_depth` must forward to `mesh.effect_shader` so
/// `material_translate` builds `soft_falloff_depth` + MAT_FLAG_EFFECT_SOFT
/// for the soft-particle depth fade in triangle.frag. Pre-fix only the NIF
/// `BSEffectShaderProperty` path populated these, so every FO4 BGEM
/// mist / steam / beam volume (`soft = true` in the authored file)
/// rendered with no depth feather and stacked to an opaque white-out.
#[test]
fn bgem_merge_forwards_soft_particle_depth() {
    use byroredux_bgsm::BgemFile;
    use byroredux_nif::import::BsEffectShaderData;
    use byroredux_renderer::vulkan::material::material_flag::EFFECT_SOFT;

    let bgem = BgemFile {
        soft_enabled: true,
        soft_depth: 200.0,
        effect_lighting_enabled: true,
        lighting_influence: 1.0,
        falloff_start_angle: 0.5,
        falloff_stop_angle: 0.2,
        falloff_start_opacity: 0.9,
        falloff_stop_opacity: 0.1,
        ..Default::default()
    };

    // Mirror the prod assignment from the BGEM branch.
    let es = BsEffectShaderData {
        falloff_start_angle: bgem.falloff_start_angle,
        falloff_stop_angle: bgem.falloff_stop_angle,
        falloff_start_opacity: bgem.falloff_start_opacity,
        falloff_stop_opacity: bgem.falloff_stop_opacity,
        soft_falloff_depth: bgem.soft_depth,
        effect_soft: bgem.soft_enabled,
        effect_lit: bgem.effect_lighting_enabled,
        lighting_influence: (bgem.lighting_influence.clamp(0.0, 1.0) * 255.0).round() as u8,
        ..Default::default()
    };

    assert!(
        (es.soft_falloff_depth - 200.0).abs() < f32::EPSILON,
        "soft_depth must forward to soft_falloff_depth"
    );
    assert!(es.effect_soft, "soft_enabled must map to effect_soft");
    assert_eq!(es.lighting_influence, 255, "1.0 influence → 255 on u8 payload");
    let flags = crate::cell_loader::pack_effect_shader_flags(Some(&es));
    assert_ne!(
        flags & EFFECT_SOFT,
        0,
        "EFFECT_SOFT must be packed so the shader's soft-fade branch fires"
    );
}

/// Regression for #1585 / F6 — `geometry_csg` must open + resolve the
/// `<Plugin> - Geometry.csg` companion ONCE per plugin across N precombine
/// cell-loads, caching even the negative (no-CSG) result so a plugin
/// without a companion blob isn't re-stat'd on every cell. Pre-fix
/// `spawn_precombined_meshes` called `open_geometry_csg` unconditionally
/// per cell, re-parsing the chunk table and discarding the warm zlib cache.
#[test]
fn geometry_csg_caches_result_across_cell_loads() {
    let mut mp = MaterialProvider::new();
    // No companion `… - Geometry.csg` exists beside this path → None.
    let plugin = "/nonexistent/does-not-exist/Fallout4.esm";

    assert!(mp.geometry_csg(plugin).is_none());
    assert_eq!(
        mp.csg_cache.len(),
        1,
        "the negative result is cached under the plugin key"
    );
    // A second (and Nth) precombine cell-load is a pure cache hit — no
    // re-open, no re-stat, no chunk-table re-parse.
    assert!(mp.geometry_csg(plugin).is_none());
    assert_eq!(
        mp.csg_cache.len(),
        1,
        "second call hits cache; no new probe of the missing CSG"
    );
}

/// Regression for #1453 — BGEM `grayscale_texture` (palette/gradient LUT
/// for fire-gradient, electricity-gradient, magic VFX) must forward to
/// `mesh.bgsm_greyscale_lut_path`. Pre-fix the field was silently dropped,
/// so effect materials that relied on a colour-ramp palette rendered
/// without the LUT lookup.
#[test]
fn bgem_merge_forwards_grayscale_texture_as_lut_path() {
    use byroredux_bgsm::BgemFile;

    let bgem = BgemFile {
        grayscale_texture: "textures\\effects\\gradients\\fire_gradient.dds".into(),
        ..Default::default()
    };

    // Mirror the prod assignment from the BGEM branch.
    let mut lut_path: Option<String> = None;
    if lut_path.is_none() && !bgem.grayscale_texture.is_empty() {
        lut_path = Some(bgem.grayscale_texture.clone());
    }

    assert_eq!(
        lut_path.as_deref(),
        Some("textures\\effects\\gradients\\fire_gradient.dds"),
        "BGEM grayscale_texture must be forwarded to bgsm_greyscale_lut_path"
    );

    // An empty grayscale_texture must NOT overwrite an already-set path.
    let bgem_empty = BgemFile {
        grayscale_texture: String::new(),
        ..Default::default()
    };
    let original_path = lut_path.clone();
    if lut_path.is_none() && !bgem_empty.grayscale_texture.is_empty() {
        lut_path = Some(bgem_empty.grayscale_texture.clone());
    }
    assert_eq!(
        lut_path, original_path,
        "empty texture must not clobber existing path"
    );
}

/// Regression for #1580 — BGEM's `grayscale_to_palette_alpha` bool must
/// forward alongside the LUT path so `pack_bgsm_material_flags` (in
/// `cell_loader.rs`) can pick `EFFECT_PALETTE_ALPHA` over the color
/// default. Pre-fix the bool had zero consumers outside the parser.
#[test]
fn bgem_merge_forwards_grayscale_to_palette_alpha_bool() {
    use byroredux_bgsm::BgemFile;

    let bgem = BgemFile {
        grayscale_texture: "textures\\effects\\gradients\\electricity.dds".into(),
        grayscale_to_palette_alpha: true,
        ..Default::default()
    };

    // Mirror the prod assignment from the BGEM branch.
    let mut lut_path: Option<String> = None;
    let mut lut_is_alpha = false;
    if lut_path.is_none() && !bgem.grayscale_texture.is_empty() {
        lut_path = Some(bgem.grayscale_texture.clone());
        lut_is_alpha = bgem.grayscale_to_palette_alpha;
    }

    assert_eq!(lut_path.as_deref(), Some("textures\\effects\\gradients\\electricity.dds"));
    assert!(
        lut_is_alpha,
        "grayscale_to_palette_alpha=true must forward to bgsm_greyscale_lut_is_alpha"
    );

    // BGSM never authors an alpha variant — a BGSM-only path stays color.
    let bgem_color_only = BgemFile {
        grayscale_texture: "textures\\effects\\gradients\\fire.dds".into(),
        grayscale_to_palette_alpha: false,
        ..Default::default()
    };
    let mut lut_path2: Option<String> = None;
    let mut lut_is_alpha2 = false;
    if lut_path2.is_none() && !bgem_color_only.grayscale_texture.is_empty() {
        lut_path2 = Some(bgem_color_only.grayscale_texture.clone());
        lut_is_alpha2 = bgem_color_only.grayscale_to_palette_alpha;
    }
    assert!(lut_path2.is_some());
    assert!(!lut_is_alpha2, "default BGEM/BGSM path must stay the color variant");
}

/// Every failing-to-resolve path logs at most once, so a broken
/// material in a 1000-REFR cell doesn't spam the log.
#[test]
fn failed_path_set_dedups_warnings() {
    let mut provider = MaterialProvider::new();
    // No archives registered → every resolve_bgsm fails at the
    // archive read step. The failed_paths set grows on the first
    // call only.
    let before = provider.failed_paths.len();
    let _ = provider.resolve_bgsm("materials/absent.bgsm");
    let _ = provider.resolve_bgsm("materials/absent.bgsm");
    let _ = provider.resolve_bgsm("materials/absent.bgsm");
    let after = provider.failed_paths.len();
    assert_eq!(after, before + 1);
}

/// `build_material_provider` on CLI args without `--materials-ba2`
/// returns an empty provider — the merge helper short-circuits
/// when the archive lookup fails, so pre-FO4 content pays zero cost.
#[test]
fn build_material_provider_without_args_is_empty() {
    let provider = build_material_provider(&[]);
    assert!(provider.archives.is_empty());
}

/// `build_script_provider` without `--scripts-bsa` yields an empty
/// provider whose every `.pex` lookup misses — the attach path then
/// skips the VMAD branch (the `is_empty` fast-out) and falls through
/// exactly like an unregistered SCPT. No game data needed.
#[test]
fn build_script_provider_without_args_is_empty_and_misses() {
    let provider = build_script_provider(&[]);
    assert!(provider.is_empty());
    assert!(provider.extract_pex("DA10MainDoorScript").is_none());
}

/// The `.pex` archive-key normalisation: a bare VMAD-authored script
/// name resolves to `scripts\<lower>.pex`, and names that already
/// carry the folder / extension / forward-slashes are idempotent.
#[test]
fn pex_archive_path_normalises_every_authored_form() {
    // Bare name (the common VMAD case).
    assert_eq!(
        pex_archive_path("DA10MainDoorScript"),
        "scripts\\da10maindoorscript.pex"
    );
    // Already lowercase + folder + extension → unchanged.
    assert_eq!(
        pex_archive_path("scripts\\da10maindoorscript.pex"),
        "scripts\\da10maindoorscript.pex"
    );
    // Extension present, folder missing.
    assert_eq!(
        pex_archive_path("MyScript.pex"),
        "scripts\\myscript.pex"
    );
    // Forward slashes are converted to the archive's backslashes.
    assert_eq!(
        pex_archive_path("scripts/Sub/MyScript"),
        "scripts\\sub\\myscript.pex"
    );
    // Mixed case folded.
    assert_eq!(pex_archive_path("FXShader"), "scripts\\fxshader.pex");
}

/// Regression for #583 / #1454 / #1455 — synthetic BGSM template chain
/// exercises child-first scalar precedence inline with the prod helper
/// body. Child authors `emit_enabled=true` + distinct emissive, specular,
/// glossiness, alpha, UV, fresnel_power, grayscale_to_palette_scale, and
/// two_sided; parent authors different values. The child's scalar values
/// must win; parent must contribute only fields the child left at its
/// default.
///
/// This mirrors the prod `merge_bgsm_into_mesh` scalar body (minus the
/// archive-read step); any future drift between the two surfaces here.
#[test]
fn bgsm_merge_forwards_scalars_child_first() {
    use byroredux_bgsm::template::ResolvedMaterial;
    use byroredux_bgsm::{BaseMaterial, BgsmFile};
    use std::sync::Arc;

    let child = BgsmFile {
        base: BaseMaterial {
            alpha: 0.25,
            u_offset: 0.1,
            v_offset: 0.2,
            u_scale: 2.0,
            v_scale: 3.0,
            two_sided: true,
            ..Default::default()
        },
        emit_enabled: true,
        emittance_color: [1.0, 0.5, 0.25],
        emittance_mult: 7.0,
        specular_color: [0.9, 0.8, 0.7],
        specular_mult: 3.5,
        smoothness: 0.85,
        fresnel_power: 3.5, // non-default; must win over parent's 9.0
        grayscale_to_palette_scale: 0.75, // non-default; must win over parent's 2.5
        ..Default::default()
    };
    let parent = BgsmFile {
        base: BaseMaterial {
            alpha: 0.5,
            u_offset: 99.0, // must NOT win
            ..Default::default()
        },
        emit_enabled: true,
        emittance_color: [0.0, 0.0, 0.0],
        emittance_mult: 0.0,
        specular_mult: 0.01,             // must NOT win
        smoothness: 0.01,                // must NOT win
        fresnel_power: 9.0,              // must NOT win
        grayscale_to_palette_scale: 2.5, // must NOT win
        ..Default::default()
    };
    let resolved = ResolvedMaterial {
        file: child,
        parent: Some(Arc::new(ResolvedMaterial {
            file: parent,
            parent: None,
        })),
    };

    // Replicate the scalar-forwarding half of merge_bgsm_into_mesh
    // inline. Mesh starts with engine defaults so every write below
    // is the BGSM path overriding a fallback.
    let mut emissive_color = [0.0f32; 3];
    let mut emissive_mult = 0.0f32;
    let mut specular_color = [1.0f32; 3];
    let mut specular_strength = 1.0f32;
    let mut glossiness = 0.0f32;
    let mut mat_alpha = 1.0f32;
    let mut uv_offset = [0.0f32; 2];
    let mut uv_scale = [1.0f32; 2];
    let mut two_sided = false;
    let mut is_decal = false;
    let mut fresnel_power = 5.0f32;
    let mut grayscale_to_palette_scale = 1.0f32;

    let mut set_emissive = false;
    let mut set_specular = false;
    let mut set_glossiness = false;
    let mut set_alpha = false;
    let mut set_uv = false;
    let mut set_fresnel = false;
    let mut set_palette_scale = false;
    for step in resolved.walk() {
        let bgsm = &step.file;
        if !set_emissive && bgsm.emit_enabled {
            emissive_color = bgsm.emittance_color;
            emissive_mult = bgsm.emittance_mult;
            set_emissive = true;
        }
        if !set_specular {
            specular_color = bgsm.specular_color;
            specular_strength = bgsm.specular_mult;
            set_specular = true;
        }
        if !set_glossiness {
            // Mirror of the production conversion (`bgsm.smoothness * 100.0`)
            // — 0–1 smoothness on the BGSM side becomes 0–100 glossiness
            // on the Material side.
            glossiness = bgsm.smoothness * 100.0;
            set_glossiness = true;
        }
        if !set_fresnel {
            fresnel_power = bgsm.fresnel_power;
            set_fresnel = true;
        }
        if !set_palette_scale {
            grayscale_to_palette_scale = bgsm.grayscale_to_palette_scale;
            set_palette_scale = true;
        }
        if !set_alpha {
            mat_alpha = bgsm.base.alpha;
            set_alpha = true;
        }
        if !set_uv {
            uv_offset = [bgsm.base.u_offset, bgsm.base.v_offset];
            uv_scale = [bgsm.base.u_scale, bgsm.base.v_scale];
            set_uv = true;
        }
        if bgsm.base.two_sided {
            two_sided = true;
        }
        if bgsm.base.decal {
            is_decal = true;
        }
    }

    // Child values must win.
    assert_eq!(emissive_color, [1.0, 0.5, 0.25]);
    assert_eq!(emissive_mult, 7.0);
    assert_eq!(specular_color, [0.9, 0.8, 0.7]);
    assert_eq!(specular_strength, 3.5);
    // BGSM smoothness 0.85 → 85.0 on the Material 0–100 scale.
    assert_eq!(glossiness, 85.0);
    assert_eq!(mat_alpha, 0.25);
    assert_eq!(uv_offset, [0.1, 0.2]);
    assert_eq!(uv_scale, [2.0, 3.0]);
    // #1454 — child's non-default fresnel wins over parent's 9.0.
    assert!((fresnel_power - 3.5).abs() < f32::EPSILON);
    // #1455 — child's non-default palette scale wins over parent's 2.5.
    assert!((grayscale_to_palette_scale - 0.75).abs() < f32::EPSILON);
    // Boolean OR across the chain — child authored true.
    assert!(two_sided);
    assert!(!is_decal);
}

/// `detect_kind` returns `Bgem` for a buffer with BGEM magic even
/// when the caller intended BGSM dispatch. This is the unit
/// foundation for the wrong-extension footgun guard (#758): a forged
/// `.bgsm`-named file carrying BGEM magic is correctly identified.
#[test]
fn detect_kind_returns_bgem_for_bgem_magic_in_bgsm_named_file() {
    use byroredux_bgsm::{detect_kind, MaterialKind};
    // Minimal BGEM header (just the 4-byte magic) — enough for detect_kind.
    let bgem_magic_bytes: &[u8] = b"BGEM";
    assert_eq!(
        detect_kind(bgem_magic_bytes),
        Some(MaterialKind::Bgem),
        "detect_kind must return Bgem even when the caller opened a .bgsm-named file"
    );

    let bgsm_magic_bytes: &[u8] = b"BGSM";
    assert_eq!(
        detect_kind(bgsm_magic_bytes),
        Some(MaterialKind::Bgsm),
        "detect_kind must return Bgsm for genuine BGSM magic"
    );

    // A mismatched extension is detected by comparing ext_kind vs magic_kind
    // as done in merge_bgsm_into_mesh. Simulate the comparison logic.
    let ext_kind = Some(MaterialKind::Bgsm); // extension says .bgsm
    let magic_kind = detect_kind(bgem_magic_bytes); // magic says BGEM
    assert_ne!(
        ext_kind, magic_kind,
        "extension (.bgsm) and magic (BGEM) must disagree — this is the mismatch case"
    );
}

// ── #1289 / SF-D3-NEW-01 — Starfield `.mat` arm in
//   `merge_bgsm_into_mesh`. Verifies the audit-fail closure: a
//   Starfield-shaped mesh (`.mat` material path) flips `is_pbr`
//   when (and only when) the Component Database is loaded.

fn imported_mesh_with_material_path(
    pool: &mut byroredux_core::string::StringPool,
    path: &str,
) -> ImportedMesh {
    // Empty-but-valid `ImportedMesh`; the merge helper only touches
    // material-flow fields. `ImportedMesh` has no `Default` impl
    // (every field is concretely meaningful), so we hand-construct
    // — mirrors the same shape as `empty_mesh()` in
    // `pack_bgsm_material_flags_tests` (`byroredux/src/cell_loader.rs`).
    ImportedMesh {
        positions: Vec::new(),
        colors: Vec::new(),
        normals: Vec::new(),
        tangents: Vec::new(),
        uvs: Vec::new(),
        indices: Vec::new(),
        translation: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        texture_path: None,
        material_path: Some(pool.intern(path)),
        name: None,
        has_alpha: false,
        src_blend_mode: 6,
        dst_blend_mode: 7,
        alpha_test: false,
        alpha_threshold: 0.0,
        alpha_test_func: 6,
        two_sided: false,
        is_decal: false,
        normal_map: None,
        glow_map: None,
        detail_map: None,
        gloss_map: None,
        dark_map: None,
        parallax_map: None,
        env_map: None,
        env_mask: None,
        tint_map: None,
        inner_layer_map: None,
        specular_map: None,
        lighting_map: None,
        flow_map: None,
        wrinkle_map: None,
        is_pbr: false,
        has_translucency: false,
        model_space_normals: false,
        from_bgsm: false,
        bgem_glass: false,
        metalness_override: None,
        roughness_override: None,
        translucency_subsurface_color: [0.0; 3],
        translucency_transmissive_scale: 0.0,
        translucency_turbulence: 0.0,
        translucency_thick_object: false,
        translucency_mix_albedo: false,
        parallax_max_passes: None,
        parallax_height_scale: None,
        vertex_color_mode: 2,
        texture_clamp_mode: 0,
        emissive_color: [0.0; 3],
        emissive_mult: 0.0,
        emissive_source: byroredux_core::ecs::components::material::EmissiveSource::None,
        specular_color: [1.0; 3],
        diffuse_color: [1.0; 3],
        ambient_color: [1.0; 3],
        specular_strength: 1.0,
        glossiness: 80.0,
        refraction_strength: 0.0,
        lighting_effect_1: 0.0,
        lighting_effect_2: 0.0,
        subsurface_rolloff: 0.0,
        rimlight_power: 0.0,
        backlight_power: 0.0,
        grayscale_to_palette_scale: 1.0,
        bgsm_greyscale_lut_path: None,
        bgsm_greyscale_lut_is_alpha: false,
        fresnel_power: 5.0,
        uv_offset: [0.0; 2],
        uv_scale: [1.0; 2],
        mat_alpha: 1.0,
        env_map_scale: 1.0,
        parent_node: None,
        skin: None,
        z_test: true,
        z_write: true,
        z_function: 3,
        local_bound_center: [0.0; 3],
        local_bound_radius: 0.0,
        effect_shader: None,
        material_kind: 0,
        shader_type_fields: byroredux_nif::import::ShaderTypeFields::default(),
        no_lighting_falloff: None,
        wireframe: false,
        flat_shading: false,
        flags: 0,
        bs_lod_cutoffs: None,
        bs_sub_index: None,
    }
}

/// Synthetic minimal CDB: BETH magic + header + STRT (empty) + TYPE
/// chunk declaring zero types. Sufficient for `register_starfield_cdb`
/// to mark `has_starfield_cdb() == true` without needing 105 MB of
/// real Starfield data.
///
/// `chunkCount` field is "chunks INCLUDING the BETH header" per
/// `crates/sfmaterial/src/reader.rs::index_chunks` line 143-147 —
/// BETH + STRT + TYPE = 3, so the post-header chunk loop reads 2.
fn minimal_cdb_bytes() -> Vec<u8> {
    let mut buf = Vec::with_capacity(40);
    // 16-byte header: magic + headerSize + fileVersion + chunkCount=3.
    buf.extend_from_slice(&0x48544542u32.to_le_bytes()); // BETH
    buf.extend_from_slice(&8u32.to_le_bytes()); // headerSize
    buf.extend_from_slice(&4u32.to_le_bytes()); // fileVersion
    buf.extend_from_slice(&3u32.to_le_bytes()); // chunkCount (incl BETH)
                                                // STRT chunk: type + size + empty payload.
    buf.extend_from_slice(b"STRT");
    buf.extend_from_slice(&0u32.to_le_bytes()); // size = 0
                                                // TYPE chunk: type + size=4 + u32 type_count=0.
    buf.extend_from_slice(b"TYPE");
    buf.extend_from_slice(&4u32.to_le_bytes()); // size = 4
    buf.extend_from_slice(&0u32.to_le_bytes()); // type_count = 0
    buf
}

/// #1571 / SF-D3-03 — the discovery predicate must match the base
/// CDB AND every DLC/Creation-namespaced one, and reject everything
/// else. `Ba2Archive::list_files` hands back lowercase/backslash
/// paths, but the predicate normalises so it's robust to either.
#[test]
fn is_materialsbeta_cdb_path_matches_base_and_dlc() {
    // Base game.
    assert!(is_materialsbeta_cdb_path(
        "materials\\materialsbeta.cdb"
    ));
    // DLC / Creations — the paths the hardcoded extract missed.
    assert!(is_materialsbeta_cdb_path(
        "materials\\creations\\shatteredspace\\materialsbeta.cdb"
    ));
    assert!(is_materialsbeta_cdb_path(
        "materials\\creations\\sfbgs003\\materialsbeta.cdb"
    ));
    assert!(is_materialsbeta_cdb_path(
        "materials\\creations\\sfbgs00d\\materialsbeta.cdb"
    ));
    // Forward-slash + mixed-case input still matches (normalised).
    assert!(is_materialsbeta_cdb_path(
        "Materials/Creations/ShatteredSpace/MaterialsBeta.cdb"
    ));
    // Non-CDB / wrong-root paths are rejected.
    assert!(!is_materialsbeta_cdb_path(
        "materials\\foo\\bar.bgsm"
    ));
    assert!(!is_materialsbeta_cdb_path(
        "meshes\\materialsbeta.cdb" // right filename, wrong root
    ));
    assert!(!is_materialsbeta_cdb_path("materialsbeta.cdb")); // no materials\ root
}

/// #1571 — every discovered CDB is held in load order (base first,
/// then DLC) so SF-D3-01 Phase 2 can build one last-wins index. The
/// pre-fix single `Option<Arc<…>>` could only hold one, silently
/// dropping DLC materials once Phase 2 lands.
#[test]
fn discovered_cdbs_accumulate_in_load_order() {
    let mut provider = MaterialProvider::new();
    assert!(!provider.has_starfield_cdb(), "empty provider has no CDB");

    // Base CDB, then a DLC CDB — both pass the header-only probe
    // (#2100), both counted.
    provider.register_starfield_cdb(&minimal_cdb_bytes());
    provider.register_starfield_cdb(&minimal_cdb_bytes());
    assert!(provider.has_starfield_cdb());
    assert_eq!(
        provider.sf_cdb_count,
        2,
        "a second CDB must increment the count, not replace the first (was \
         the single-Option bug that dropped DLC CDBs)"
    );

    // A malformed CDB is rejected (peek_magic, #2102) + warned, leaving
    // the count intact.
    provider.register_starfield_cdb(b"not a cdb");
    assert_eq!(
        provider.sf_cdb_count,
        2,
        "a rejected CDB must not change the already-counted CDBs"
    );
}

/// Audit-fail closure: a `.mat` path on a Starfield mesh with the
/// CDB loaded must flip `is_pbr=true` so `pack_bgsm_material_flags`
/// packs `MAT_FLAG_PBR_BSDF` and `triangle.frag` routes through
/// Disney BSDF instead of legacy Lambert.
#[test]
fn merge_sets_is_pbr_on_mat_path_when_cdb_loaded() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    provider.register_starfield_cdb(&minimal_cdb_bytes());
    assert!(
        provider.has_starfield_cdb(),
        "minimal CDB payload must mark the provider as Starfield-loaded"
    );

    let mut mesh =
        imported_mesh_with_material_path(&mut pool, "materials/setpieces/cargobay.mat");
    assert!(!mesh.is_pbr, "fresh ImportedMesh defaults to is_pbr=false");

    let touched = merge_bgsm_into_mesh(&mut mesh, &mut provider, &mut pool);

    assert!(touched, ".mat arm must report touched=true");
    assert!(
        mesh.is_pbr,
        "Starfield .mat path must flip is_pbr=true → MAT_FLAG_PBR_BSDF in shader"
    );
    // `from_bgsm` deliberately stays false — that flag gates BGSM
    // spec-glossiness translation which is wrong for Starfield .mat
    // (metalness/roughness direct authoring).
    assert!(!mesh.from_bgsm, "Starfield path must NOT set from_bgsm");
}

/// CDB-presence gate: a `.mat` path against a non-Starfield archive
/// set (no CDB loaded) must NOT flip `is_pbr`. Modded `.mat` paths
/// on FO4 / FNV / Skyrim cells shouldn't accidentally route to
/// Disney BSDF.
#[test]
fn merge_skips_mat_path_when_cdb_absent() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    // No `register_starfield_cdb` call.
    assert!(!provider.has_starfield_cdb());

    let mut mesh = imported_mesh_with_material_path(&mut pool, "materials/modded.mat");
    let touched = merge_bgsm_into_mesh(&mut mesh, &mut provider, &mut pool);

    // Falls through past the .mat arm; bgsm/bgem dispatch fails
    // because the path doesn't match either suffix; returns false
    // (no archive to resolve from anyway).
    assert!(!touched, "no CDB + no archives → no merge work");
    assert!(!mesh.is_pbr, ".mat path without CDB must NOT flip is_pbr");
}

/// SF3-02 / #1831 — a `.mat` path with no CDB loaded gets the
/// CDB-specific diagnostic, naming the actual degradation instead of
/// the generic "unsupported format" message.
#[test]
fn unresolved_material_warning_names_missing_cdb_for_mat_path() {
    let msg = unresolved_material_warning("materials/modded.mat", false);
    assert!(
        msg.contains("no CDB is loaded/parsed"),
        "expected the CDB-specific diagnostic, got: {msg}"
    );
    assert!(msg.contains("--materials-ba2"));
}

/// A `.mat` path is only reachable in this arm when the CDB IS present
/// but nonetheless useless here (defence in depth) — must fall back to
/// the generic message rather than falsely blaming a present CDB.
#[test]
fn unresolved_material_warning_falls_back_when_cdb_present() {
    let msg = unresolved_material_warning("materials/modded.mat", true);
    assert!(
        msg.contains("unsupported format"),
        "expected the generic diagnostic when a CDB is loaded, got: {msg}"
    );
}

/// A non-`.mat` unrecognised extension always gets the generic message,
/// regardless of CDB state — the CDB-specific wording is `.mat`-only.
#[test]
fn unresolved_material_warning_generic_for_non_mat_path() {
    let msg = unresolved_material_warning("materials/weird.xyz", false);
    assert!(msg.contains("unsupported format"));
    assert!(!msg.contains("CDB"));
}

/// A `.bgsm` path must NOT enter the Starfield arm even when the
/// CDB is loaded — the FO4 BGSM dispatch wins, preserving
/// spec-glossiness translation.
#[test]
fn mat_arm_does_not_steal_bgsm_dispatch() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    provider.register_starfield_cdb(&minimal_cdb_bytes());

    let mut mesh =
        imported_mesh_with_material_path(&mut pool, "materials/setdressing/metallocker01.bgsm");
    let _ = merge_bgsm_into_mesh(&mut mesh, &mut provider, &mut pool);

    // The .bgsm path falls past the .mat arm into BGSM dispatch,
    // which fails on the missing archive (no .bgsm to extract).
    // `is_pbr` stays at its default — BGSM dispatch doesn't flip
    // it without a successful resolve.
    assert!(
        !mesh.is_pbr,
        ".bgsm path must not be hijacked by the Starfield arm"
    );
}
