# #3737 — TD1-2026-08-30-02: `texture_registry.rs` is 2013 production LOC in a 2021-line file — #3081's 838 figure is not reproducible

**Labels**: bug, renderer, low, tech-debt

---

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/renderer/src/texture_registry.rs`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD1-2026-08-30-02`), HEAD `64f64480`

## Description

Unlike every other primary-bucket member, this file is *not* inflated by inline tests —
its own tests live in the sibling `texture_registry_tests.rs` via
`#[cfg(test)] #[path] mod`. **2013 of 2021 lines are production** texture-registry logic
(re-verified at HEAD: 2021 total lines, 4 `cfg(test)` markers, all in the last ~100 lines,
two of which are `#[path = "..."] mod` declarations pointing at *separate* files).

**This is filed deliberately against #3081's evidence table**, which recorded this file's
production LOC as 838 (majority-test). The 838 figure is not reproducible; 2013 is.

## Suggested Fix — split by lifecycle phase (the existing method ordering already implies the seam)

- **Acquire / lookup** (`get_by_path*`, `acquire_by_path*`, `acquire_by_path_for_view`,
  `fallback`, `neutral_fallback`) → `texture_registry/lookup.rs`.
- **Upload queue** (`enqueue_dds*`, `enqueue_dds_for_view`, `queue_or_hit*`,
  `pending_dds_upload_count`, `flush_pending_uploads` — the last alone is ~200 LOC) →
  `texture_registry/upload.rs`.
- **Release / deferred destroy** (`drop_texture`, `drop_textures`, `drop_released_texture`,
  `release_ref`, `release_refs_batch`, `decrement_ref`, `tick_deferred_destroy`,
  `drain_pending_destroys`) → `texture_registry/release.rs`.
- The struct, `new()` and the bindless descriptor plumbing stay in `mod.rs`.

Effort: medium. Same `sed`-extract method and `cargo fmt`-reformats-the-whole-crate caveat
as `TD1-2026-08-30-01`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
