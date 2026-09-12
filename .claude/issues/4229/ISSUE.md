# FNV-D2-02: translate_material keys glass classification and pre-computed PBR scalars off different (pre/post overlay) texture paths

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4229
**Labels**: bug, low, legacy-compat, game:fnv, game:fo4, game:skyrim, nifal
**Source**: `/audit-fnv` — `docs/audits/AUDIT_FNV_2026-09-11.md`, finding FNV-D2-02

**Severity**: LOW
**Dimension**: NIFAL Canonical Translation (FNV Slice) — `/audit-fnv` Dimension 2
**Location**: `byroredux/src/material_translate.rs:475-666` (`translate_material`, `texture_path` at `:485`, glass classification call at `:648-666`) vs `crates/nif/src/import/material/mod.rs:1387-1411,1518-1519` (PBR scalar pre-computation)

**Description**: `translate_material` classifies glass from the caller's **overlay-resolved** (post-REFR `XATO`/`XTNM`/`XTXR`) texture path, but PBR scalars (`metalness_override`/`roughness_override`) arrive pre-computed at NIF-import time from the mesh's **own, un-overlaid** path (`classify_legacy_pbr` in `crates/nif/src/import/material/mod.rs`). The two classifications key off different strings within one `translate_material` call, so an overlay swap can carry the original texture's PBR classification onto a materially different surface with no per-draw fallback to catch the mismatch.

**Evidence**: `texture_path` at `material_translate.rs:485` is `textures.base_color.clone()` (overlay-resolved by the caller before this function runs), consumed by `classify_glass_into_material` at `:648-666`. `crates/nif/src/import/material/mod.rs:1387-1411` computes `metalness_override`/`roughness_override` from `classify_legacy_pbr`, which reads the mesh's own un-overlaid texture path via `self.texture_path` — a separate, earlier resolution than the overlay path `translate_material` sees.

**Impact**: Currently unreachable on FNV because FNV's own `XATO` mis-decode (#1887/#3511, tracked separately) makes the overlay path inert. Live and reachable on FO4+/Skyrim TXST-overlay placements, where an overlay swap can carry the original texture's metalness/roughness onto a materially different surface with no per-draw fallback to mask it.

**Related**: #1887, #3511 (the FNV `XATO` misread that currently masks this on FNV), `docs/engine/nifal.md`.

**Suggested Fix**: Key both the glass classification and the PBR-scalar pre-computation off the same resolved texture path — either defer PBR-scalar classification to `translate_material`'s call site (after overlay resolution), or pass the overlay-resolved path back into `classify_legacy_pbr` before the NIF-import-time PBR computation runs.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per NIFAL's single-boundary invariant, per-game/overlay logic must land at the parser→`Material` boundary (`translate_material`), never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Check whether other overlay-dependent `MaterialInfo` fields (besides PBR scalars) have the same pre-vs-post-overlay key mismatch.
- [ ] **TESTS**: A regression test on an FO4/Skyrim TXST-overlay fixture pins the fix (overlay swap must not carry stale PBR classification).
