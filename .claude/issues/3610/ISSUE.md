# #3610 — REN-2026-08-30-D16-04: in-code comments quote the pre-retune froxel grid and a 4× ray-query count

**Labels**: `low,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3610 --json state`.

---

- **Severity**: Low
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs:543` (doc comment on `VOLUMETRIC_OUTPUT_CONSUMED`), `crates/renderer/shaders/volumetrics_inject.comp:25`–`26`, `crates/renderer/shaders/volumetrics_integrate.comp:27`–`28`
- **Status**: OPEN — new
- **Description**: Three comments state a froxel grid derived from the old
  `froxel_xy_divisor = 4`:
  - `volumetrics.rs:543` — *"~36.9M ray queries/frame at the default
    320x180x64 grid for a 1280x720 render extent"*
  - `volumetrics_inject.comp:25`–`26` — *"At the default 320x180x64 grid that
    is a worst-case ~36.9M ray queries/frame from this pass alone"*
  - `volumetrics_integrate.comp:27`–`28` — *"we run it 129 600 times per frame
    at 1080p with the default /4 grid"*

  At the live divisor of 8, a 1280×720 render extent gives **160×90×64**
  (921 600 froxels), so the same worst case is **~9.2M** ray queries, and a
  1920×1080 native extent gives 240×135 = **32 400** integrate columns, not
  129 600.
- **Evidence**:
  - `crates/renderer/src/vulkan/upscaling.rs:135` — `froxel_xy_divisor: 8`
  - `crates/renderer/src/vulkan/volumetrics.rs:562`–`576` — `froxel_extent` = `render.div_ceil(divisor)` × `froxel_z_slices`
  - `1280.div_ceil(8) = 160`, `720.div_ceil(8) = 90`; `160 × 90 × 64 = 921 600`; × the comment's own worst-case 10 traversals/froxel = 9 216 000
  - Test `froxel_extent_uses_render_resolution_and_configured_divisor`
    (`volumetrics.rs:3416`) was deliberately written to derive rather than
    snapshot `[320, 180, 64]` for exactly this reason — the comments were not
    given the same treatment.
- **Impact**: These are the numbers a performance investigation reads first
  when deciding whether the inject pass is worth optimising; being 4× high
  misdirects that decision. `volumetrics.rs:543` in particular is the
  justification comment on the live `VOLUMETRIC_OUTPUT_CONSUMED` gate.
- **Suggested Fix**: Restate all three relative to the config
  (e.g. "at the default divisor of 8, a 1280×720 render extent gives
  160×90×64") or drop the absolute counts and keep the per-froxel worst case,
  which is divisor-independent. Same fix shape the test at `:3416` already
  adopted.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D16-04

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
