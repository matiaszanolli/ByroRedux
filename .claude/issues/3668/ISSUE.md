# #3668 — PERF-D5-2026-08-30-03: `memory-budget.md`'s G-buffer VRAM row understates the live attachment set by 4–10× and contradicts its own two columns

- **Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md`
- **Finding ID**: `PERF-D5-2026-08-30-03`
- **Filed**: 2026-08-30 (HEAD `64f64480`)
- **Labels**: medium,performance,memory,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3668

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is authoritative for current state.

---

- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `docs/engine/memory-budget.md:538`; ground truth
  `crates/renderer/src/vulkan/gbuffer.rs` (format constants + the seven
  `Attachment` fields) and `crates/renderer/src/vulkan/context/helpers.rs:190-223`
- **Status**: NEW
- **Description**: The row reads
  `| G-buffer (8 attachments × 2 FIF, incl. FSR reactive/transparency masks) | ~23 MB | ~47 MB (4K) |`.
  Every neighbouring row labels its columns 1080p / 4K. Computed from the shipped
  formats, the seven `GBuffer` attachments are 22 B/px → **91.2 MB** at 1080p ×2 FIF
  and **365.0 MB** at native 4K ×2 FIF. Read as the row's own "8 attachments"
  (i.e. including the `R16G16B16A16_SFLOAT` HDR colour) plus depth and
  depth_history, it is **141.0 MB** and **564.0 MB**. Either way the figures are
  4× to 10× low. The row is also self-inconsistent: a 4K "peak" only 2× the 1080p
  "typical" is impossible for a resolution-scaled allocation with 4× the pixels.
- **Evidence**: `git log -L538,538` shows the numbers were **incremented, never
  recomputed**, across the attachment-count churn: `78540d8e` seeded
  `7 attachments → ~35 MB / ~70 MB`; `ca874e41` (the `#1583`/`#1590` reservoir
  removal) wrote `6 attachments → ~22 MB / ~45 MB`; `2cb86be5` (the FSR mask
  addition) wrote `8 attachments → ~23 MB / ~47 MB` — i.e. two whole attachments,
  one of them 8 B/px, were added for +1 MB.
- **Impact**: `memory-budget.md` is named in `_audit-common.md` as an authoritative
  doc auditors are told to prefer over re-deriving, and this row feeds the
  `~1.74 GB` / `~3.4 GB at native 4K` totals two rows below. Understating one
  resolution-scaled subsystem by ~200 MB at 1080p and ~500 MB at 4K makes the
  "inside the < 4 GB target" conclusion unsound at native 4K — the same class of
  ledger error as `PERF-2026-08-27b-01` (vertex/index pool) and `#3447` (Instance
  SSBO), on a different row of the same table. `docs/engine/shader-pipeline.md`'s
  G-Buffer Layout table is, by contrast, correct and current.
- **Related**: `PERF-2026-08-27b-01`, open `#3447`. Both are the same defect class;
  neither covers this row.
- **Suggested Fix**: Replace the row with the computed figures and split HDR
  colour / depth / depth_history out explicitly, then re-total the table. Better:
  derive the number in code the way the volumetrics section already does
  (`FROXEL_BYTES_PER_SLOT` is read by a regression test *and* the boot log) — a
  *GBUFFER_BYTES_PER_PIXEL* constant next to the format constants, asserted by a
  test, would make this row impossible to drift again.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-30.md` (HEAD `64f64480`). Report status: NEW; re-verified CONFIRMED against HEAD at publish time.*
