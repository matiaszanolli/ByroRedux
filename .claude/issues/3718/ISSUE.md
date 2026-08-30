# #3718 — ESM-2026-08-30-D7-02: EsmIndex residency is 0.6-2.6 GB of RAM per master and appears nowhere in memory-budget.md

*Filed 2026-08-30 from `docs/audits/`. Immutable snapshot of the issue as filed (TD10-001 / #1156); GitHub is authoritative for current state.*

**Severity**: MEDIUM · **Dimension**: ESM→ECS Handoff
**Location**: `crates/plugin/src/esm/records/index.rs` (93 session-lifetime `HashMap`s); `docs/engine/memory-budget.md`
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D7-02)

## Description

`EsmIndex` residency is **0.6–2.6 GB of RAM per master** and appears nowhere in `docs/engine/memory-budget.md`.

## Evidence

Measured with `crates/plugin/examples/esm_dim8_bench` under `/usr/bin/time -f %M`, release build:

| master | file | parse | peak RSS | index ≈ RSS − file |
|---|---|---|---|---|
| Oblivion.esm   | 265 MB | 1.41 s | 1 441 MB | ~1.18 GB |
| Fallout3.esm   | 275 MB | 1.23 s | 1 059 MB | ~0.78 GB |
| FalloutNV.esm  | 234 MB | 1.17 s |   861 MB | ~0.63 GB |
| Skyrim.esm     | 238 MB | 1.27 s |   980 MB | ~0.74 GB |
| Fallout4.esm   | 315 MB | 1.69 s | 1 440 MB | ~1.13 GB |
| SeventySix.esm | 880 MB | 3.41 s | 3 509 MB | ~2.63 GB |
| Starfield.esm  | 1.39 GB | **not run — no safe headroom on this host** | | |

`Starfield.esm` was deliberately **not** parsed: extrapolating the FO76 ratio puts it near 4 GB. That is itself the point.

## Impact

Nothing evicts these maps; they are held for the session and `merge_from` accumulates them across a load order (vanilla FO4 is base + 7 DLC masters). `memory-budget.md` documents VRAM to the byte and has exactly one CPU-side section (the Starfield CDB) — the ESM index, the largest single CPU allocation in a normal run, is absent. Survivable on the dev box; not on a 16 GB machine with a modded FO76/Starfield order.

## Suggested Fix

Add an "ESM Index (CPU-side)" section to `docs/engine/memory-budget.md` with these measured numbers and the dominant maps.

Separate follow-up worth scoping: most of the 93 maps (`camera_shots`, `menu_icons`, `voice_types`, the 30 `MinimalEsmRecord` stub maps) are `EDID`-only stubs with no consumer, each retaining a `String` per record.

## Completeness Checks
- [ ] **SIBLING**: Other undocumented CPU-side allocations checked while the section is added
- [ ] **TESTS**: If a budget figure is asserted, it is pinned by a test or regenerable command, not hand-typed
