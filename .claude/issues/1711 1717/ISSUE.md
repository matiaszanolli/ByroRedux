# #1711: SPT-NEW-03: OBND-derived bs_bound is computed then discarded on the cell-loader route (loose --tree route keeps it)

**Severity**: LOW (tech-debt) · **Status**: already resolved in code, issue not yet closed

`import_spt_scene` produces a Y-up `bs_bound` AABB from TREE.OBND. The loose
`--tree` route (`scene/nif_loader.rs:1029`) attaches it as a `BSBound`
component. The cell-loader route's `CachedNifImport` adapter carries no
`bs_bound` field, so the AABB never reaches cell-spawned entities — they only
get the per-mesh `LocalBound` sphere (`local_bound_center`/`local_bound_radius`),
which is the canonical cull/pick bound on both routes.

## Resolution
Already landed in commit `af359ed8` (2026-06-26, "Fix #1655 #1734 #1732 +
document #1711"): chose suggested-fix option (b) — do not thread `bs_bound`
through `CachedNifImport`. Added an explicit doc comment on the struct
(`nif_import_registry.rs:110-126`) explaining the decision, why it's not a
correctness gap (`BSBound` has no functional consumer today, only debug-server
inspection), and the re-enable path for a future AABB-keyed consumer (add
`bs_bound: Option<([f32;3],[f32;3])>` mirroring `bsx_flags`, attach in
`spawn.rs` for route parity).

Verified today that the current code still matches this decision:
- `CachedNifImport` (`nif_import_registry.rs:34-127`) has no `bs_bound` field.
- `spawn.rs` (cell route) inserts only `LocalBound` (line ~823), never `BSBound`.
- `scene/nif_loader.rs:1061-1069` (loose route) is the sole `BSBound` consumer.

## Completeness Checks
- [x] **SIBLING**: N/A — chose not to thread through the adapter, so no
      `bsx_flags`/`root_flags` round-trip pattern to mirror.
- [ ] **TESTS**: Not added. A full functional regression test would require
      extracting the `LocalBound`-seeding snippet out of the ~800-line
      per-REFR spawn loop (`spawn.rs`) and/or the `BSBound`-attachment snippet
      out of `nif_loader.rs`'s two top-level functions (both need a real NIF
      byte stream / Vulkan context to exercise end-to-end) — disproportionate
      refactor for a LOW-severity, explicitly-non-correctness tech-debt item.
      The intentional omission is compiler-enforced today: `CachedNifImport`
      has no `bs_bound` field, so no code path can accidentally attach
      `BSBound` on the cell route without a deliberate struct change touching
      all ~7 construction sites.

---

# #1717: SF-D7-01: ROADMAP / compat-matrix Starfield parse rates understate current state

**Severity**: LOW (documentation; code is *better* than the doc claims)
**Location**: `ROADMAP.md` compat-matrix row (line ~207) + per-game NIF
clean-parse-rate row (line ~737)

ROADMAP recorded "Starfield 98.6% aggregate, Meshes01 97.21%, MeshesPatch
98.11%, sweep date 2026-04-27". Intervening parser work (#1510 BSShaderType155
tail, #1606 starfield_tail, #754 BSWeakReferenceNode, #722 cloth) lifted the
rate but the matrix was never refreshed.

## Verification
Reproduced the live sweep before editing (per the issue's own completeness
check), `cargo test -p byroredux-nif --test parse_real_nifs -- --ignored
parse_rate_starfield_all_meshes --nocapture`, 2026-07-03:
```
[Starfield/Meshes01.ba2]    100.00% (31058/31058)
[Starfield/Meshes02.ba2]    100.00% (7552/7552)
[Starfield/MeshesPatch.ba2]  98.91% (29524/29849, 325 truncated)
[Starfield/LODMeshes.ba2]   100.00% (19535/19535)
[Starfield/FaceMeshes.ba2]  100.00% (1282/1282)
```
Aggregate: 88951/89276 clean = 99.64%. Exactly matches the issue's cited figures.

## Fix
Updated both ROADMAP.md rows with the verified 2026-07-03 figures. Left
`docs/audits/*.md` untouched (dated historical snapshots — the discrepancy
they recorded at the time was itself correct for that date) and
`docs/engine/*.md` reference pages untouched (out of this issue's explicit
scope, which named only ROADMAP.md). `HISTORY.md` was grepped and does not
cite the stale figures — nothing to update there.

## Completeness Checks
- [x] **SIBLING**: ROADMAP.md compat-matrix row + per-game clean-parse-rate
      row updated together; `HISTORY.md` checked, no stale citation found.
- [x] **TESTS**: Figures reproduced from a live opt-in sweep before committing
      the doc edit (exact match to the issue's cited numbers).
