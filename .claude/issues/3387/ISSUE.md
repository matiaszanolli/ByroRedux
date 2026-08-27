# PERF-D7-2026-08-27-03: `NifImportRegistry::touch_keys` allocates a fresh `String` per already-present key, over an O(placements) input list

- **Issue**: [#3387](https://github.com/matiaszanolli/ByroRedux/issues/3387)
- **Finding ID**: `PERF-D7-2026-08-27-03`
- **Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md`
- **Audit suite preset**: streaming-deep (2026-08-27)
- **Labels**: `low,performance,bug`

> Immutable snapshot of the issue **as filed** (TD10-001 / #1156). GitHub is authoritative
> for current state — query `gh issue view 3387 --json state`.

---

- **Severity**: LOW
- **Dimension**: Streaming & Cells
- **Location**: `byroredux/src/cell_loader/nif_import_registry.rs:437-445`
  (`touch_keys`); producer at
  `byroredux/src/cell_loader/references/synth_child.rs:492`
  (`accum.pending_hits.push(cache_key.clone())`); consumer at
  `byroredux/src/cell_loader/references/complete.rs:121`.
- **Status**: NEW
- **Description**: Two compounding wastes on the same list, both in the
  end-of-cell batched commit that every cell load — interior and streamed
  exterior — funnels through.

  1. `touch_keys` guards with `contains_key(key)` and then, having proved the
     entry exists, calls `insert(key.to_string(), t)`. That allocates a `String`
     and re-hashes the key, to overwrite a `u64` in a slot it already located.
     This is the same shape as #832 (`or_insert(name.to_string())` in the NIF
     per-block counters, which leaked ~150 KB/cell of throwaway short strings on
     Oblivion) — the fix there was the `entry().get_mut()/insert` split.
  2. `pending_hits` is a `Vec<String>` pushed **per placement**, not per unique
     model. A cell with 2 000 static placements over 150 unique meshes stores
     ~1 850 duplicate copies of 150 strings, holds them for the whole (possibly
     multi-frame, resumable) cell apply, and then makes `touch_keys` allocate and
     hash each duplicate again — writing a fresh tick to the same ~150 slots over
     and over, where only the last write survives.
- **Evidence**: `nif_import_registry.rs:438-444`
  ```rust
  for key in keys {
      if self.access_tick.contains_key(key) {
          let t = self.next_tick;
          self.next_tick = self.next_tick.wrapping_add(1);
          self.access_tick.insert(key.to_string(), t);
      }
  }
  ```
  `access_tick` is `HashMap<String, u64>` (`:257`). The producer side is inside
  `spawn_synth_child`, which runs once per synthetic child placement.
- **Impact**: ~2 × (cache-hit placement count) throwaway `String` allocations per
  cell load, on the main thread inside the streaming apply budget. The engine's
  own logged example ("6 new unique meshes parsed, NIF cache hits/misses 156/6
  this cell", quoted at `references/mod.rs:1338-1342`) puts that in the low
  hundreds for a Riverwood-class cell and the low thousands for a dense one — so
  tens to low hundreds of microseconds per cell, three cells per boundary
  crossing. Not a hitch; a clean, guard-shaped win with a named precedent.
- **Related**: #832 (CLOSED — the same anti-pattern, fixed in `crates/nif`);
  #523 / #635 (the batching invariant this code correctly preserves — the fix
  below does not disturb it). The 2026-08-26 FNV audit noted "`touch_keys` only
  bumps ticks for already-present keys", which is semantically right and is
  exactly why the allocation is unnecessary.
- **Suggested Fix**: replace the guard-then-insert with
  `if let Some(slot) = self.access_tick.get_mut(key) { *slot = t; }` — one hash,
  zero allocations. Then make `pending_hits` a `HashSet<String>` (or keep the
  `Vec` and `sort_unstable` + `dedup` before the commit) so the tick loop is
  O(unique models) instead of O(placements).

---
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
_Filed by `/audit-publish` from `docs/audits/AUDIT_PERFORMANCE_2026-08-27.md` (audit-suite preset: streaming-deep). Finding ID: `PERF-D7-2026-08-27-03`._
