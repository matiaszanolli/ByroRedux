# ESM-D7-02: `plugin-loading.md` still says the ESH (`0xFD`) layout is "not modelled yet"

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6802
Labels: low, esm-plugin, documentation, doc-rot
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Dimension**: 7 (Redux-native tier rot)
- **Location**: `docs/engine/plugin-loading.md:121` and `:310-313` (also the `FormIdPool` snippet at `:134-140`)
- **Status**: NEW
- **Description**: `GlobalSlot::Medium` (`crates/plugin/src/esm/reader.rs:513-516`, `compose` at `:535-537`) landed in `9d41ecb66` (2026-09-22, #4639), and `FormIdScheme::Starfield` (per-kind ordinals, #6118, `0a2955d33` 2026-10-20) after it. The doc text was last touched in `e3131f5ef` (2026-09-15) and still says "The ESH (`0xFD`) layout is not modelled yet" and "When a Starfield multi-master load needs it, add that layout to `GlobalSlot`". The `FormIdPool` struct sample also omits `entity_index` (#6338, `crates/core/src/form_id.rs:175-180`).
- **Impact**: documentation only. A reader would re-add a slot model that exists.
- **Suggested fix**: say `GlobalSlot::{Regular, Light, Medium}` + `FormIdScheme::{Positional, Starfield}` resolve all three layouts; drop the "add it later" paragraph; add the `entity_index` field to the sample.
- **Banner state**: the #5609 status banners are intact (see Checked and clean).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
