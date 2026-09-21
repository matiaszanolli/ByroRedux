# Near-term action plan — 2026-09-20

**As of**: HEAD `42725fdcc` (pushed), post `/fix-issue` batch #4398–#4407.
**Tracker**: 101 open issues; exactly **2 labeled HIGH** (#4396, #4397).
**Spine**: [playable-vertical-slice.md](playable-vertical-slice.md) — P0/P2-core/P3
closed; P2 tail, P4, P5 remain.

Waves are sequenced by dependency and context freshness, not by size. Wave 1
is a short issue batch best run while the #4406 guard-family context is warm;
Wave 2 is the milestone arc; Wave 3 clusters interleave between Wave-2
checkpoints; Wave 4 items are slotted when measurement is next needed.

---

## Wave 1 — Animation NaN-safety HIGHs (#4396, #4397)

**STATUS: DONE 2026-09-20** — fixed in `d53be91be` (single rotation
sanitizer + NaN-inclusive pose gates), both issues closed. Zero HIGHs
remain open on the tracker.

**Why now**: they are the last two open HIGHs, and both live in the same
sanitizer family #4406 just touched (`crates/nif/src/anim/bspline.rs` →
`normalized_rotation_sample`, NIFAL-D7-2026-09-14). #4396's own body points at
"a shared sanitizer" as the shape. Context goes cold fast on guard-family
work.

- **#4396** — "components square to inf → zero quaternion" hole on every
  non-B-spline rotation path: mainline KF keys, static poses, HKX. Entry
  points: `crates/nif/src/anim/{keys,transform,channel}.rs`; HKX side via
  `byroredux/src/asset_provider/animation.rs`.
- **#4397** — static-pose fallbacks gate only on `is_flt_max`, false for NaN;
  a NaN pose reaches the canonical clip despite `anim/keys.rs` documenting
  those paths as guarded.

**Suggested shape**: promote the #4406 guard into one shared rotation-sample
sanitizer (non-finite component → reject; `len_sq` overflow → reject;
near-zero → identity, per the #4406 decision) and route all four sub-channel
paths through it. Same for the FLT_MAX/NaN pose gate in #4397 — one predicate,
documented once.

**Done when**: both closed with a shared-sanitizer commit; regression tests
drive the production predicate directly (the `anim/tests/bspline.rs` pattern);
`cargo test -p byroredux-nif` + full workspace green.

**Estimate**: one `/fix-issue` batch.

---

## Wave 2 — Playable vertical slice: P2 tail → P4 → P5

The roadmap's active spine. Sequenced so each step unblocks the next; two
open tracker issues serve this wave directly and should be pulled into it
rather than swept separately.

### 2a. File the unfiled — **DONE 2026-09-21**

1. walk_anim lock-order failures → **#4546** (reproduced: five walk_anim
   tests panic under `BYRO_LOCK_ORDER_CHECK=1`, cycle
   ActorCinematicState → Transform → AnimationPlayer).
2. `p0[oblivion]` contract SKIP gap → **#4547** (fixture declares the
   route; `playable-smoke.yml` can't dispatch it — game choices and
   forwarded data-env both omit oblivion).
3. FO4 facegeom BGSM `msn`-flag=false side finding → **#4548** (with the
   verification steps the original finding deferred).

### 2b. P2 tail — combat feel + loot

- Authored attack / hit / death animation on the frozen `BleakFallsBarrow01`
  fixture (currently the 8-damage unarmed fallback with no anim/sound).
- Corpse interaction + loot transfer. **#4248** (FO4 `attach_container_inventory`
  never expands LVLI) belongs here — placed containers are empty exactly when
  loot first matters.
- Sound hooks (see Tier-2 audio note below if M44 audio is pulled forward
  instead).

### 2c. P4 — player body + authored objective/dialogue

- **Player body = bare capsule → equipped body** (the noted next structural
  step; `equipment_appearance_system` re-equip reconcile and the MS01 fixture
  are already landed/frozen, so this is the render-side body attach).
- Authored objective + dialogue on the MS01 fixture; **#4334** (onlyOnce
  actor-base stage triggers re-arm after cell reload) belongs here — it is a
  quest-stage correctness bug on the exact machinery P4 builds on.
- P3 HUD leftovers only if the slice route demands them.

### 2d. P5 — persistence + soak

- Save → exit → reload continuity across the whole route (the save/load
  component-registry sweep from Session 63 is the base).
- Soak: repeated traversal, combat, save/load loop; file whatever falls out.

**Done when**: the mid-term gate from `playable-vertical-slice.md` — one
console-free Skyrim route: control, E-interaction, door traversal, one
combat/loot loop, inventory/equipment UI, objective, save/reload — passes
end-to-end, with a smoke test joining
[`docs/smoke-tests/`](../smoke-tests/README.md).

---

## Wave 3 — Audit-sweep backlog (interleave between Wave-2 checkpoints)

Clusters, all verified open 2026-09-20. Run as `/fix-issue` batches when a
Wave-2 checkpoint closes or context demands variety. Per the 2026-05-03
priority review: close from this set, don't grow it.

| Cluster | Issues | Notes |
| --- | --- | --- |
| PEX / scripting sweep | #4474–#4479 (low) + #4471–#4473 (medium) | The long-promised sweep; mediums first (#4471 EVENT_NAMES gaps, #4472 peek sites, #4473 auto-state collision) |
| FO3 carryovers | #4468, #4469 (low) | Emitter-rate siblings from the FO3 audit |
| Character audit tail | #4452–#4457, #4459–#4463 (low/med, incl. 2 doc-rot) | The 2026-09-19 batch's declared leftovers |
| Starfield material cluster | #4429, #4428, #4283, #4282, #4279, #4268, #4256 | One coherent SF-materials pass; shares files with each other |
| Flipbook role resolve | #4426 (med, Oblivion) | Same per-role resolver family as #3901/#4403 — good context pairing with Wave 1 |
| Gameplay mediums | #4232 (FNV actor level 0), #4334*, #4248* | *Pulled into Wave 2 where noted |
| Exterior | #4488 (FO76 .bto object-LOD) | Standalone; needs FO76 data on disk |

---

## Wave 4 — Measurement & housekeeping (slot when needed)

- **Bench-of-record refresh** — ~300 commits stale (R6a-stale tracking).
  Re-running the stepped-camera suite re-opens the renderer audit cycle per
  the 2026-05-03 moratorium gate, so this is the *gate* for any new
  renderer-audit appetite, not just hygiene. #2161 (PERF-REGRESSION-6c56e311)
  is closed, so the refresh runs on a clean known state.
- **AO flats 0.89 mystery** — one systematic ndl≈0.88 sample left
  undiagnosed by the SSAO retune (commit `09d9bc6f8`,
  `DBG_VIZ_AO=0x200` instrumentation available). Diagnose-or-file.
- **Water push-const + BC2 overrun pair** — carried renderer arc from the
  earlier options menu; BC2 overrun was also flagged post-#3922.
- **Doc-rot sweep** — #4459/#4460 (character) are open; #4403 closed this
  week. Fold any new ones into one batch.

## Explicitly deferred (unchanged)

ObScript phase 2, M47.2 remainder, RT tail — bigger arcs re-offered and left
unpicked; they stay parked until the vertical slice advances or the bench
refresh reallocates appetite. Anti-scope list in
[ROADMAP.md](../ROADMAP.md#what-we-are-not-doing-anti-scope) still applies.

---

## Driving this plan

- Waves 1 and 3 run as `/fix-issue <numbers>` batches (user-driven, as
  usual).
- Wave 2 items are milestone work, not issue batches — plan/execute per the
  slice doc's own structure.
- `/session-close` should sync this file's as-of header and fold Wave
  progress into ROADMAP/HISTORY (this doc is an execution view, not a second
  source of truth — ROADMAP stays authoritative).
