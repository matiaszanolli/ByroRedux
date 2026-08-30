# #3599 — REN-2026-08-30-D10-03: `GpuCamera`'s own rustdoc header still says 352 bytes and contradicts the test it names two lines later

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3599 --json state`.

---

- **Severity**: LOW
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera`, L335-343)
- **Status**: New (distinct site from #3450 / #3447 — this is the struct's own doc, not a SKILL file or `shader-pipeline.md`)
- **Description**: The header line reads
  `/// GPU-side camera data (**352 bytes**, std140-compatible).` while the very
  next paragraph says `Layout pinned by \`gpu_camera_is_368_bytes\` test — three
  \`mat4\` … + eleven trailing \`vec4\` … → 368 B`. The size-history sentence
  also terminates at `336 → 352 B with the structured renderer-debug control`
  and never records the `352 → 368 B` step that `exterior_sky_tint` (#3323)
  added. `GpuCamera.render_origin` — this dimension's primary entry point — is
  documented inside that same block, so anyone arriving here to reason about
  the render-origin contract meets a self-contradicting header first.
- **Evidence**:
  - `gpu_types.rs:335` — `/// GPU-side camera data (**352 bytes**, std140-compatible).`
  - `gpu_types.rs:337` — `/// Layout pinned by \`gpu_camera_is_368_bytes\` test`
  - `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:66` —
    `fn gpu_camera_is_368_bytes()` asserting `size_of::<GpuCamera>()`
  - The field list in the doc already enumerates eleven `vec4`s including
    `exterior_sky_tint`, so only the headline number and the history sentence
    are stale.
- **Impact**: Documentation only — the layout itself is pinned by a passing
  test and by `reflect.rs:608`'s SPIR-V size cross-check. But it is the third
  independent site now carrying the stale 352, and #3450 / #3447 were filed
  against the other two; leaving the authoritative one wrong is what keeps
  re-seeding the copies.
- **Suggested Fix**: One-line edit: `(**368 bytes**, std140-compatible)`, and
  extend the history sentence with `then 352 → 368 B with \`exterior_sky_tint\`
  (#3323)`. Consider closing it out together with #3450 / #3447.

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D10-03

## Dedup cross-reference

Third independent site carrying the stale `GpuCamera` = 352 B figure. **#3447** covers
`shader-pipeline.md`; **#3450** covers the two audit SKILL files. This is the struct's own
rustdoc in `gpu_types.rs` — the authoritative one that keeps re-seeding the copies. Close
all three together.


## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
