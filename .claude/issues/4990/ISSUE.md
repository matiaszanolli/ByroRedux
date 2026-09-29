# #4990: CONC-D3-2026-09-28-01: weather_system's #4416 image-space block says `wd` is still live and that it follows the canonical order; `wd` was dropped 110 lines earlier

- **Repo**: matiaszanolli/ByroRedux
- **Labels**: low,concurrency,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_CONCURRENCY_2026-09-28.md
- **Filed**: 2026-09-28

- **Severity**: LOW (the comment is wrong about lock state; no edge is recorded)
- **Dimension**: ECS Lock Ordering
- **Location**: `byroredux/src/systems/weather.rs:1209-1234`. `drop(wd)` is at `:1099`.
- **Status**: NEW. Introduced by afd6a73f7 (#4416).
- **Description**: The comment reads: "The cross-fade target is re-read in the canonical
  `WeatherDataRes -> WeatherTransitionRes` order while `wd` is still live". But `wd` (the
  `WeatherDataRes` read) is dropped at `:1099`. Between the drop and this block, the only
  `WeatherDataRes` access is the scoped `try_resource_mut` at `:1104-1106`. At `:1219` the
  `WeatherTransitionRes` read is therefore taken with no guard held. It is then released at the
  end of the `if transition_t > 0.0` arm, before `ImageSpaceBase` (write) at `:1231`. The code is
  safer than the comment says.
- **Evidence**: The acquisition sequence in the block is:
  1. `CellLightingRes` (R; consumed by `is_none_or`)
  2. `WeatherTransitionRes` (R; scoped)
  3. `ImageSpaceBase` (W)

  No two of these guards overlap. The only other `ImageSpaceBase` reader,
  `image_space_modifier_system` (`crates/scripting/src/cinematic.rs:396-399`), copies the value
  out before taking `CinematicPresentationState`.
- **Trigger Conditions**: None at runtime. The risk is that a maintainer relies on the comment.
  An edit that trusts "wd is live" could, for example, re-read `wd` fields here by re-acquiring
  `WeatherDataRes` under `tr`. That would record `WeatherTransitionRes → WeatherDataRes`, the
  reverse of the `:798`→`:893` order that #3263 documented.
- **Impact**: Documentation rot in a lock-order comment. It sits on the exact pair #3263 closed
  as undocumented.
- **Verification Path**: n/a for today's code. A regression of the shape described above fires
  under `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux -- weather`, because
  `:798`/`:893` already record the forward edge.
- **Related**: #3263 (closed; the WeatherDataRes→WeatherTransitionRes order), #4416, #1103.
- **Suggested Fix**: Reword the comment to "no weather guard is live here; `tr` is scoped to the
  cross-fade arm and drops before the `ImageSpaceBase` write".

Source: `docs/audits/AUDIT_CONCURRENCY_2026-09-28.md` (CONC-D3-2026-09-28-01) · HEAD `9e6f08870`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related systems / CI steps
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition and the `docs/engine/ecs.md` canonical order are preserved
- [ ] **TESTS**: A regression test pins this specific fix
