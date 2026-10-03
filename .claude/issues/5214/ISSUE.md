# #5214 — REN-D8-2026-10-03-01: `shader-pipeline.md`'s inject binding table stops at 23; the live shader declares 26 bindings (0–25)

**Labels**: low,renderer,shaders,documentation,doc-rot
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location**: `docs/engine/shader-pipeline.md` (the `volumetrics_inject.comp` table: "24 bindings — widened twice…"), against `crates/renderer/shaders/volumetrics_inject.comp` (`CombustionOccupancyOut` / `CombustionOccupancyIn`) and `crates/renderer/src/vulkan/volumetrics/init.rs` (layout bindings 24/25)
- **Status**: NEW (recurrence of the class closed by #3830)
- **Description**: c705c310d (#4784) added `layout(std430, set = 0, binding = 24) buffer CombustionOccupancyOut` and `binding = 25 readonly buffer CombustionOccupancyIn`. Both are in the descriptor-set layout and are rotated per FIF (out = slot `f`, in = slot `previous`). The same commit edited `shader-pipeline.md`, but only the `GpuLight` rows. The inject table still says "24 bindings", lists 0–23, and has no row for either occupancy buffer or its per-FIF rotation. Its own header warns: "verify against the source before relying on this table for a new binding".
- **Evidence**: `grep -n "binding 24\|24/25\|occupancy" docs/engine/shader-pipeline.md` returns nothing. `grep -n "binding = 2[45]" crates/renderer/shaders/volumetrics_inject.comp` returns both declarations.
- **Impact**: Documentation only. The next binding added to the inject set will be numbered against a stale table, which is how #3830 started.
- **Related**: #3830 (same table, 12 vs 24); REN-D5-2026-10-03-04 (the same buffers have no memory-budget ledger row).
- **Suggested Fix**: Add rows 24/25 (STORAGE_BUFFER, 16³ `u32` mask on the fog-cluster grid; 24 = this slot's atomicOr marks, 25 = previous slot's mask as the dilated skip gate). Update the count to 26.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix (or a doc-pin / `_audit-validate.sh` check where one fits)
