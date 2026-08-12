//! Mesh / normal-map / material path normalization.
//!
//! Extracted from the 2051-LOC `asset_provider/tests.rs` (#2411 / TD1-010)
//! at the topic-divider comments that file already carried. Contents
//! unchanged.

use super::super::*;

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
    let out = strip_build_prefix("skyrimhd\\build\\pc\\data\\textures\\plants\\florajuniper.dds");
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
    let out = normalize_material_path("data\\materials\\setdressing\\metaltrashcan01alpha.bgsm");
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
