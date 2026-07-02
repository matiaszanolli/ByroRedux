# SPT-NEW-01: detect_variant / SpeedTreeVariant are dead code — no production or test consumer

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1820
**Source report**: docs/audits/AUDIT_SPEEDTREE_2026-07-02.md
**Labels**: low, import-pipeline, tech-debt, bug

- **Severity**: LOW
- **Dimension**: Per-Game Variants
- **Location**: `crates/spt/src/version.rs:90-100` (`detect_variant`), `:24-49` (`SpeedTreeVariant` + impl), `crates/spt/src/lib.rs:61`
- **Status**: NEW (raised in the 2026-06-23 and 2026-07-01 reports, never filed as an issue unlike its siblings #1707/#1711/#1715; re-verified still accurate in `AUDIT_SPEEDTREE_2026-07-02.md`)

**Description**: `detect_variant` and `SpeedTreeVariant` are re-exported from `lib.rs` but have zero call sites outside `version.rs`'s own unit tests and one `#[cfg(feature = "recon")]` dev tool (`crates/spt/examples/spt_dissect.rs:63`). The production `parse_spt` independently re-validates `MAGIC_HEAD` via `bytes.starts_with(...)` (`parser.rs:48`) and never consults `detect_variant`; the placeholder importer is variant-agnostic. This confirms the Dimension-4 checklist expectation — nothing downstream depends on the variant being correct — so the documented `V5Fnv` default for every `__IdvSpt_02_` file (including Oblivion 4.x) is benign.

**Evidence**: `grep -rn "detect_variant\|SpeedTreeVariant" --include='*.rs' byroredux crates | grep -v version.rs` hits only `lib.rs` re-export/doc and the recon-gated `crates/spt/examples/spt_dissect.rs` — no production or test consumer. Confirmed unchanged against current source.

**Impact**: None at runtime. Maintenance only: the API reads as a live per-game dispatch hook but is inert, which can mislead a contributor into "fixing" the `V5Fnv` default or wiring it where the per-REFR route already works.

**Related**: Distinct from SPT-NEW-03 / #1711 (that is `bs_bound`, a different field).

**Suggested Fix**: Either wire `detect_variant` into the cell-loader `.spt` route as a logged sanity check (useful once the geometry-tail decoder needs Oblivion-vs-FO3/FNV disambiguation), or mark it `#[allow(dead_code)]` with a "reserved for Phase 2 variant dispatch" note.

## Completeness Checks
- [ ] **SIBLING**: Check for other per-game dispatch hooks in `crates/spt` that were wired at design time but never connected to a caller
- [ ] **TESTS**: If wired into the cell-loader route, add a test asserting the logged sanity check fires; if marked `#[allow(dead_code)]` instead, no test needed
