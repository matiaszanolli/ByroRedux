# #762: SF-D6-03: Starfield `.mat` (JSON) material file parser + provider integration

URL: https://github.com/matiaszanolli/ByroRedux/issues/762
Labels: enhancement, low, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 6, SF-D6-03)
**Severity**: LOW (forward-blocker / enhancement)
**Status**: NEW (planning tracker)

## Description

Starfield uses `.mat` files (JSON-formatted) catalogued in a `materialsbeta.cdb` component database. The `.mat` files are loose JSON with texture paths and shader-component references; the `.cdb` is a hashed lookup of compiled materials.

Reverse-engineering reference: gibbed (`github.com/gibbed/Gibbed.Starfield/tree/main/projects/Gibbed.Starfield.FileFormats`). Bethesda ships `Tools/ContentResources.zip` with loose pre-extracted `.mat` files — the loose path covers immediate development needs; `.cdb` lookup is needed for runtime resolution from the materials BA2.

## Suggested implementation outline

| Component | Effort | Notes |
|---|---|---|
| `serde_json` deserializer for `.mat` schema | small | One file format, JSON, well-defined |
| Field mapping into a new `StarfieldMaterial` (cannot share `BaseMaterial` with BGSM) | small | Different field set than BGSM |
| `.cdb` reader (reverse-engineered binary) | medium | Gibbed's reference impl is C# but documented; needed for the 99% case in vanilla `Starfield - Materials.ba2` |
| `MaterialProvider` plumbing | small | Mirror existing BGSM path |
| Wire to renderer's albedo/normal/AO slots | small | Already done for BGSM; pattern reusable |

**Total estimate: medium.** Self-contained crate (`crates/sfmaterial/`?) mirroring the `bgsm` crate.

## Suggested first deliverable

Crate `crates/sfmaterial/` with:
- `parse_mat(json: &str) -> Result<StarfieldMaterial>` for loose `.mat` (covers `Tools/ContentResources.zip` immediately)
- `MaterialProvider` integration so the warn-fallback in SF-D3-03 resolves to a real material when a loose `.mat` is found

Defer `.cdb` lookup to a Stage B follow-up.

## Completeness Checks

- [ ] **TESTS**: Round-trip a sample loose `.mat` through the parser; assert texture paths extracted.
- [ ] **SIBLING**: Update SF-D3-02 (#750) and SF-D3-06 (#761) once Starfield material support lands.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- SF-D3-02 (#750) — bgsm crate doc misadvertisement.
- SF-D3-03 (#751) — silent fallback in `merge_bgsm_into_mesh`.
- SF-D3-06 (this batch) — texture_clamp_mode default depends on this parser.
