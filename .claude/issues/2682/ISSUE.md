# PERF-D2-05: sort_draw_commands in-place partition self-swaps ~480-byte DrawCommands

**Issue**: #2682
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: LOW — **DOWNGRADED from MEDIUM** after the auditor disproved its own claim (see Impact)
- **Dimension**: 2 — Draw & Instancing
- **Location**: [mod.rs](byroredux/src/render/mod.rs) — `sort_draw_commands` (the raster/RT-only partition loop)
- **Status**: NEW
- **Description**: The partition calls `draw_commands.swap(raster_len, index)`
  without guarding `raster_len == index`. `<[T]>::swap` lowers to `ptr::swap`, which
  performs the full three-way copy regardless of index equality — so every raster
  draw encountered before the first RT-only draw pays a round-trip memcpy of a
  ~480-byte struct against itself.
- **Evidence**:
  ```rust
  let mut raster_len = 0;
  for index in 0..draw_commands.len() {
      if draw_commands[index].in_raster {
          draw_commands.swap(raster_len, index);
          raster_len += 1;
      }
  }
  ```
- **Impact**: I tried to disprove this and it is **much smaller than it first
  looks**, which is why it is LOW rather than MEDIUM. A self-swap occurs only while
  `raster_len == index`, i.e. only across the *initial run* of consecutive
  `in_raster` commands; once one RT-only draw has been seen, every subsequent swap is
  a real one. For a mixed set the expected wasted-swap count is small. The waste
  becomes O(N) only in the fully-visible case — a cell where frustum culling flags
  nothing, or any run under `BYRO_NO_CULL=1` — where it reaches roughly
  `N × 2 × 480 B` of pointless traffic (~2.4 MB/frame at the
  `fo4-InstituteBioScience` baseline's 3440 commands). **No quantitative guard
  exists for this site.**
- **Related**: #516 (the `in_raster` / TLAS predicate split that introduced the
  partition), #2173.
- **Suggested Fix**: One line — `if raster_len != index { draw_commands.swap(raster_len, index); }`.
  Cheap enough that the bounded worst case justifies it even though the expected
  case is small.

---


---
*Filed from [`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `PERF-D2-05`.*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or a bench delta vs the checked-in baseline) pins this fix
