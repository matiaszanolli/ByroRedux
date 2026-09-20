# EXT-D2-2026-09-19-02: VTXT opacity is an unvalidated wire f32 — falsifies the splat sorter's NaN-cannot-appear premise

- **ID**: EXT-D2-2026-09-19-02
- **Labels**: medium,esm-plugin,terrain-exterior,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4484

**Severity**: MEDIUM · **Dimension**: Terrain/splatting · **Tier Violated**: no-fabrication (a documented data guarantee the parser does not provide) · **Game Affected**: all LAND games (Oblivion/FO3/FNV/Skyrim/FO4; FO76/Starfield ship no LAND)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D2-2026-09-19-02)

**Location**: `crates/plugin/src/esm/cell/walkers.rs:1315-1318` (raw decode); `byroredux/src/cell_loader/terrain.rs:420-427` (false-premise comment), `:439-446` (`total_coverage`)

**Description**
VTXT opacity is `f32::from_le_bytes` off the wire — no finite gate, no [0,1] clamp — stored verbatim, while both the parser doc ("values 0.0–1.0", `crates/plugin/src/esm/cell/mod.rs:150`) and `select_top_by_coverage`'s comment ("NaN cannot appear") assert the guarantee. A NaN alpha makes `total_coverage` NaN and the `partial_cmp().unwrap_or(Equal)` comparator non-total (std documents `sort_by` as may-panic on such input). Probe at HEAD: no panic, but the NaN layer was **silently and nondeterministically dropped** and coverage ranking corrupted; out-of-range (±Inf) opacities are ordered but skew the 8-lane importance cap.

**Impact**
A corrupt or merged-plugin VTXT silently changes which splat lanes survive the cap — per-run, per-platform nondeterministic terrain texturing — with an in-code comment telling the next maintainer this input is impossible. No vanilla content trips it (corpus VTXT measured finite [0,1]).

**Suggested Fix**
Gate at the decode choke point: `if opacity.is_finite() { alpha[pos] = opacity.clamp(0.0, 1.0) }` in `parse_land_record`, matching the VHGT `sanitize_land_height` pattern one layer up. A `total_cmp` comparator in `select_top_by_coverage` is the secondary defense.

## Completeness Checks
- [ ] **SIBLING**: Check other VTXT/ATXT wire floats in the LAND walker for the same raw-decode shape
- [ ] **TESTS**: A parse test with a NaN/huge opacity pins the clamp; the comparator's determinism under NaN re-probed
