# PERF-D1-02: Residual per-frame heap allocations on the render tick

**Issue**: #2680
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`


- **Severity**: LOW
- **Dimension**: 1 — CPU Hot Paths
- **Location**: [lights.rs](byroredux/src/render/lights.rs) — `collect_lights` (the `sort_scratch.sort_by` call, ~line 293); [fog_volumes.rs](byroredux/src/render/fog_volumes.rs) — `collect_fog_volumes` (`out.sort_by`, ~line 49); [interaction.rs](byroredux/src/interaction.rs) — `InteractionState::prompt`, reached from `build_interaction_prompt` in [main.rs](byroredux/src/main.rs)
- **Status**: NEW
- **Description**: Three small per-frame heap allocations survive in code whose
  surrounding lines were explicitly rewritten to eliminate exactly this pattern.
  (a) `collect_lights` builds its decorate buffer allocation-free (`sort_scratch` is
  caller-owned and `clear`+`extend`ed per #2172 — guard intact), then immediately
  sorts it with `sort_by`, the **stable** sort, which allocates a temporary above the
  insertion-sort cutoff. The decorate tuple is `(f32, GpuLight)`, so the temp scales
  with the light count. Stability buys nothing here: the array is a freshly built
  decoration, and `sort_unstable_by` is still deterministic for a given input, so
  no frame-to-frame GI-prefix flicker is introduced. (b) `collect_fog_volumes` has
  the same `sort_by`-then-`truncate` shape. (c) `build_interaction_prompt` runs on
  **every** frame — including the overlay-hidden path, which is the only field
  #1376 deliberately left populated when hidden — and `InteractionState::prompt` is
  `format!("[E] {}", target.kind.verb())`, one `String` per frame whenever the
  player is looking at an activatable.
- **Evidence**:
  ```rust
  // render/lights.rs
  sort_scratch.clear();
  sort_scratch.extend(suffix.iter().map(|l| (gi_priority_score(l), *l)));
  sort_scratch.sort_by(|a, b| b.0.total_cmp(&a.0));   // stable → allocates
  ```
  ```rust
  // interaction.rs
  pub(crate) fn prompt(&self) -> Option<String> {
      self.target.map(|target| format!("[E] {}", target.kind.verb()))
  }
  ```
- **Impact**: Small — order of one to two allocations per frame plus a
  light-count-sized temp. Reported at LOW precisely because the magnitude is small;
  it is included because these are the last three sites in the per-frame render tick
  still doing what #1372 / #1725 / #2034 / #2172 removed everywhere else, and
  because **no quantitative guard exists for any per-frame render/ECS site** — there
  is nothing that would flag them growing.
- **Related**: #2034 / #2172 (decorate-sort-undecorate + caller-owned scratch for
  `collect_lights`); #1376 (debug-UI snapshot visibility gate, intact).
- **Suggested Fix**: Swap both `sort_by` calls to `sort_unstable_by`. For the
  prompt, return a `&'static str` verb plus a formatting decision at the UI layer,
  or cache the composed string on `InteractionState` and rebuild it only when
  `target` changes.

---


---
*Filed from [`docs/audits/AUDIT_PERFORMANCE_2026-08-12.md`](docs/audits/AUDIT_PERFORMANCE_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `PERF-D1-02`.*

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or a bench delta vs the checked-in baseline) pins this fix
