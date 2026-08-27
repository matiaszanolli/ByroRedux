# PERF-D7-2026-08-27-05: `block_hole_mask`'s `resident_full_cells` linear scan cannot fire under the invariant its own caller establishes

- **Issue**: [#3389](https://github.com/matiaszanolli/ByroRedux/issues/3389)
- **Finding ID**: `PERF-D7-2026-08-27-05`
- **Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `low,performance,tech-debt,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3389 --json state`.

---

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:201-217`; the input is
  built at `byroredux/src/streaming_helpers.rs:73` and passed with
  `max_full_cell_radius: state.radius_unload` at `:80-81`.
- **Status**: NEW
- **Description**: The hole predicate is
  ```rust
  cell_is_full_detail(gx, gy, player_grid, max_full_cell_radius)
      || resident_full_cells.contains(&(gx, gy))
      || cells_map.get(&(gx, gy)).and_then(|cell| cell.landscape.as_ref()).is_none()
  ```
  `cell_is_full_detail` is `chebyshev((gx, gy), player_grid) <= max_full_cell_radius`
  (`:118-125`), and `max_full_cell_radius` **is** `state.radius_unload`.
  `resident_full_cells` is `state.loaded.keys()` — and
  `compute_streaming_deltas` (`streaming.rs:1423-1431`) evicts every loaded cell
  whose Chebyshev distance exceeds `radius_unload` on the same tick the player
  grid changes, before any reconcile runs. So every element of
  `resident_full_cells` satisfies arm 1, and arm 2 can never be the arm that
  returns `true`.

  Because `||` short-circuits, the scan only *runs* for cells arm 1 rejected —
  i.e. exactly the cells for which it is guaranteed to fail — and it runs to
  completion over the full `Vec` every time.
- **Evidence**: the invariant chain is three links, all in this audit's own
  scope: `streaming_helpers.rs:73` (`resident_full_cells = state.loaded.keys()`),
  `:80` (`max_full_cell_radius = state.radius_unload`), and
  `streaming.rs:1423-1431` (`to_unload` = every loaded coord with
  `d > radius_unload`, applied at `app_step.rs:126-148` before the reconcile at
  `:234`). Bootstrap and door-transition both start from an empty `loaded`.
- **Impact**: `block_hole_mask` is called for every desired finest-band quad in
  the invalidation pass, which runs on **every** reconcile frame regardless of
  budget (`terrain_lod.rs:415-431`). At 16 cells per 4×4 block and `|loaded|`
  = 121 at the transition-default radius, that is on the order of 10⁵ tuple
  comparisons per reconcile frame that provably cannot change the result —
  roughly 100–200 µs on top of PERF-D7-2026-08-27-01's cost, on the same frames.
- **Related**: #1871 / LC0703-02 (the fix that moved this gate from
  `radius_load` to `radius_unload` — which is what made arm 2 redundant);
  PERF-D7-2026-08-27-01 (same per-reconcile-frame budget).
- **Suggested Fix**: either drop arm 2 (its `radius_unload` gate subsumes it, and
  the pinned `cell_is_full_detail_covers_hysteresis_band_when_gated_on_radius_unload`
  test at `terrain_lod.rs:1015-1042` is the guard that keeps it subsumed), or —
  if it is wanted as defence-in-depth against a future caller that passes a
  tighter radius — pass a `HashSet`/`FxHashSet` instead of a `&[(i32, i32)]` so
  the probe is O(1). Whichever is chosen, say so in the doc comment; today it
  reads as a live check.

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `PERF-D7-2026-08-27-05`._
