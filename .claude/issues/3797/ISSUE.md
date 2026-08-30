# #3797: LC-2026-08-30-D7-01: AlphaFlags bit 13 'No Sorter' is parsed into the flags word and never decoded, so the engine back-to-front-sorts every alpha-over draw including the ones the author opted out of

**Labels**: bug, nif-parser, renderer, low, legacy-compat
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-30.md` — LC-2026-08-30-D7-01 (LOW)
**Dimension**: 7 — subsystem coverage vs legacy
**Location**:
- `crates/nif/src/import/material/mod.rs:1517-1542` — `apply_alpha_flags`
- `byroredux/src/render/mod.rs:508` — slot 3 of the alpha-over sort key

## Description

nif.xml's `AlphaFlags` (`nif.xml:1554-1563`) defines eight members. `apply_alpha_flags` decodes five of them — `Alpha Blend` 0x0001, `Source Blend Mode` 0x001E, `Destination Blend Mode` 0x01E0, `Alpha Test` 0x0200, `Test Func` 0x1C00 — plus the `Threshold` byte.

**`No Sorter` (bit 13, mask 0x2000) is never read.** A grep for `0x2000` / `no_sorter` / `NoSorter` across `crates/nif`, `crates/core` and `byroredux` returns only unrelated shader-flag constants.

`No Sorter` is Gamebryo's per-property instruction to `NiAlphaAccumulator` to draw the shape in **accumulation order** rather than depth-sorted.

## Evidence

Redux implements exactly the ordering this flag opts out of, **unconditionally**. `byroredux/src/render/mod.rs:508` puts `!cmd.sort_depth` in **slot 3** of the alpha-over sort key — ahead of render layer, two-sidedness, blend factors, depth state and mesh — i.e. a global back-to-front order. The module doc (`:376-387`, `:490-503`) records that this ordering was chosen deliberately for correctness at a measured batching cost (FNV `FreesideAtomicWrangler`, 25 → 8 GPU calls).

There is no per-draw exemption, so a shape whose author disabled the sorter is sorted anyway.

This is **not** a "reference-only legacy shading param": draw ordering for alpha-over is a behaviour Redux implements on purpose, and this is the one authored control over it that the mapping ignores.

Verified at HEAD (`64f64480`): `apply_alpha_flags` is at `mod.rs:1517` and reads no 0x2000 bit; the sort key's slot-3 doc at `render/mod.rs:382` still reads *"Alpha-over — slot 3 = !sort_depth (global back-to-front order)"*.

**Confidence**: CERTAIN that the bit is unread and the sort is unconditional; **PLAUSIBLE** that it changes any vanilla frame.

## Impact

LOW **as filed**. The premise that any shipped content sets the bit is **unverified — no occupancy census has been run.**

> **Correction to the source report**: the report stated no game archive was mounted, having checked only `/media/matias`. That is wrong — all seven games are mounted under `/mnt/data/SteamLibrary/steamapps/common/<game>/Data/`. The census below is therefore straightforwardly runnable, and this issue should not be actioned past step (1) until it is.

If a census finds authored occupancy, this **escalates to MEDIUM** (a visual-ordering artifact on shipped content).

## Suggested Fix (in order)

1. **Census `flags & 0x2000`** across the Oblivion / FO3 / FNV mesh archives with a throwaway `crates/nif/examples` probe — the same method #3530 used for `APPLY_HILIGHT2` (1,433 hits / 741 meshes) and #3516 used for `clamp_mode` (2236/2258). Archives are at `/mnt/data/SteamLibrary/steamapps/common/`.
2. **If non-zero**: surface it as `MaterialInfo.no_sorter` → `Material` → `DrawCommand`, and make slot 3 of the alpha-over key `(!no_sorter, !sort_depth)` so opted-out draws keep their state-clustered order.
3. **If zero**: record the measurement beside `apply_alpha_flags` the way `properties.rs` records its other deliberate-skip decisions, so the next sweep does not re-derive it.

`Clone Unique` (0x4000) and `Editor Alpha Threshold` (0x8000) are editor/instancing hints with no render-state meaning and need no such note.

## Completeness Checks
- [ ] **CENSUS FIRST**: step (1) is run before any code change — the severity and the whole fix depend on it
- [ ] **CANONICAL-BOUNDARY**: if surfaced, `no_sorter` travels `MaterialInfo` → canonical `Material` → `DrawCommand`; the sort key reads the canonical field, never re-derives it at render time. See `/audit-nifal`.
- [ ] **SIBLING**: the same alpha-over sort key is consumed by the particle path — confirm slot 3's meaning stays consistent across both
- [ ] **TESTS**: a regression test pins the sort key's slot-3 ordering for a `no_sorter` draw against a plain alpha-over draw
