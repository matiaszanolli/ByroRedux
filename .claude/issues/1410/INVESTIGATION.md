# #1410 Investigation — TS-02 lock-order detector absent from CI

## Premise correction (audit-finding-hygiene)

The issue's recommended fix says **"Zero code change required"** — just add
`BYRO_LOCK_ORDER_CHECK=1` to a CI job. That premise is **false**. Running
`BYRO_LOCK_ORDER_CHECK=1 cargo test --workspace` locally surfaces **two real
ABBA lock-order violations** (invariant #4 in CLAUDE.md). A CI job added
naively would be immediately red. The detector must first be made green by
fixing the two cycles.

## Detector output (2026-06-02)

```
ABBA: acquiring `CellLightingRes` while holding `SkyParamsRes`
ABBA: acquiring `Name` while holding `NameIndex`
```
5 failing tests across the two pairs. Both pairs are the **complete** set
(the global graph accumulates across the whole test process; only two
cycles ever fire).

## Pair 1 — `CellLightingRes` ↔ `SkyParamsRes`

- **Forward `Cell→Sky`**: `byroredux/src/render/lights.rs:62-64` — `collect_lights`
  holds `CellLightingRes` (read) and acquires `SkyParamsRes` (read) inside the
  scope, only to read a single `sun_intensity: f32`.
- Reverse `Sky→Cell` is observed on the weather/test path. Eliminating the
  `Cell→Sky` simultaneous hold breaks the cycle; `render/lights.rs` is the only
  `Cell→Sky` site (`render/mod.rs` never imports `SkyParamsRes`).
- **Fix**: snapshot `sun_intensity` *before* acquiring `CellLightingRes` so the
  two are never held simultaneously.

## Pair 2 — `Name` ↔ `NameIndex` (self-contained in `animation_system`)

`NameIndex` is acquired **only** in `byroredux/src/systems/animation.rs`.

- **Forward `Name→NameIndex`**: line 321 binds `name_query = world.query::<Name>()`
  (held to :374); the rebuild block reads `NameIndex` (:346) and write-locks it
  (:365) while `name_query` is still held.
- **Reverse `NameIndex→Name`**: line 376 binds `name_index = NameIndex` (read,
  held to end of fn); :448/:557 call `ensure_subtree_cache` →
  `build_subtree_name_map` (`anim_convert.rs:22,30`) which acquires
  `world.query::<Name>()` while `name_index` is held.
- **Fix**: restructure the prelude so `NameIndex` is never acquired while a
  `Name` query is held — compute the count in a scoped acquire+drop, then do the
  rebuild as `NameIndex`(write)→`Name`(read), matching the subtree path's
  `NameIndex`-before-`Name` order. This removes the `Name→NameIndex` edges.

## CI

`.github/workflows/ci.yml` `cargo-test` job runs `cargo test --workspace`
(default features → `parallel-scheduler` on). Add a dedicated `lock-order-check`
job that re-runs the suite with `BYRO_LOCK_ORDER_CHECK=1`. Runs in parallel with
other jobs (no wall-clock cost) and keeps the detector signal isolated from the
main test signal. The detector itself is the regression guard.

## Scope

3 files: `render/lights.rs`, `systems/animation.rs`, `.github/workflows/ci.yml`.
Verified by the detector going green under `cargo test --workspace`.
