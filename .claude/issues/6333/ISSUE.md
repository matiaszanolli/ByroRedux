# PERF-D1-2026-10-23-03: obscript_object_system::collect_due per-frame HashSet rebuild

- **GitHub**: #6333
- **Severity**: LOW
- **Source**: docs/audits/AUDIT_PERFORMANCE_2026-10-23.md

Found by `/audit-performance` 2026-10-23 (PERF-D1-03). Derived/primitive-probe numbers only; no system-level benchmark.

## Location
`crates/scripting/src/obscript_objects.rs`: `collect_due` ~290-299 (`loaded`, the new `not_loaded`, `died` via `entities_with`), ~337 (`table.malformed.contains` inside the instance × block loop), ~99-106 (`ObScriptObjectTable`, std `HashMap`/`HashSet`).

## Description
`collect_due` runs every frame on FNV/FO3/Oblivion once any `ObScriptInstance` exists. `335994b4c` (#6220) added `not_loaded`: every `UnloadedReferenceStub` and `RemoteSceneActorStub` is collected into a `Vec` and then a SipHash `HashSet` each frame, only so each GameMode instance can ask `contains(entity)`. `loaded`/`died` have the same shape but are empty on most frames. Separately each scripted instance probes `malformed: HashSet<(u32, BlockInfo)>` (SipHash of a tuple) once per block per frame to guard a case that essentially never occurs.

## Evidence
Measured primitive (rustc -O, hot cache; Vec collect + std `HashSet<u32>` build): 1.5 µs @ 200 entities, 6.6 µs @ 1,000, 36.6 µs @ 5,000. Stub count per worldspace is unmeasured (engine logs `N logical-only actor identities` at persistent-CELL load). No quantitative guard exists for this site.

## Impact
Marginal (< ~10 µs/frame for a few hundred stubs; < 0.05 ms at 5,000) — but it is the "whole-world rebuild with no change key" shape in a system that runs every frame. Related: #6220, #5764.

## Suggested Fix
Replace `not_loaded` with per-instance `world.has::<UnloadedReferenceStub>(entity)` / `RemoteSceneActorStub` (or hold the two query guards and `.get(entity)`); build `loaded`/`died` only when their storages are non-empty; move `malformed`/`scripts` to `FxHashMap`/`FxHashSet`.

## Completeness Checks
- [ ] **LOCK_ORDER**: Query guards held across the instance loop keep TypeId-sorted acquisition
- [ ] **SIBLING**: Same pattern checked in related files/systems
- [ ] **TESTS**: A regression test pins this specific fix
