# NIF-D3-NEW-07: NiPSysEmitter base under-reads 8 bytes on Oblivion — 219 truncated scenes

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1239

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 3, sole HIGH)
**Severity**: HIGH
**Dimension**: Stream Position Integrity
**Game Affected**: Oblivion (NifVariant::Oblivion, bsver=11)

## ⚠️ Contested premise — read this first

The existing code comment at `crates/nif/src/blocks/particle.rs:47-63` and the **#383 closure** explicitly claim Oblivion uses the *shorter* (12-float) layout: "pre-#383 Oblivion (BSVER 11) parsed cleanly with the shorter layouts, so nif.xml's version gate would have over-included it."

This finding contradicts that claim with concrete trace_block evidence on `Oblivion - Meshes.bsa`. Before landing the fix, **the fixer must empirically reconcile** — run a fresh `nif_stats` on:
1. FNV `Fallout - Meshes.bsa` (must stay at 100% — no regression of #383)
2. Oblivion `meshes/magiceffects/fireball.nif` via trace_block (the smoking-gun file)
3. Oblivion `Oblivion - Meshes.bsa` full sweep (truncated count should drop from 219 → near-0)

If FNV regresses, the gate is BSVER-shaped, not version-shaped, and a different fix is needed.

## Description

`skip_emitter_base` at `crates/nif/src/blocks/particle.rs:46-65` reads 12 base floats unconditionally (48 B) and adds 2 trailing floats (`Radius Variation` + `Life Span Variation`, 8 B) only when `stream.bsver() >= 34`. Per nif.xml's `NiPSysEmitter` definition, both trailing floats apply to every NIF with `version >= 10.4.0.1`, which Oblivion (v20.0.0.5) satisfies. The current `bsver >= 34` gate excludes Oblivion (bsver=11), so every `NiPSys*Emitter` subclass on Oblivion under-reads by exactly 8 bytes.

## Evidence

`meshes\magiceffects\fireball.nif` trace via `cargo run --release -p byroredux-nif --example trace_block`:

```
[167] @  37751  NiPSysAgeDeathModifier   consumed=34         (correct)
[168] @  37785  BSPSysArrayEmitter       consumed=68         (should be 76)
[169] @  37853  NiPSysSpawnModifier      consumed=4 → ERR
      NIF requested 1 065 353 216-byte allocation, exceeds hard cap (268 435 456)
```

The 4-byte name-length field that `parse_spawn_modifier` reads at offset 37853 is actually a float `0x3f800000` (= 1.0) — i.e. the last of two floats `BSPSysArrayEmitter` should have consumed. The hex at offset 37861 (= 37853 + 8) is `15 00 00 00 4e 69 50 53 79 73 53 70 61 77 6e ...` = `[len=21]"NiPSysSpawnModifier:1"`, confirming the 8-byte shift.

Second confirmed reproduction in `meshes\fire\firearcanemedium01.nif`: `NiPSysCylinderEmitter` consumes 96 B at offset 3116, next block starts at 3220 — same 8-byte shortfall.

## Impact

Oblivion-only and cascading. Every emitter under-read shifts the next block's read by 8 bytes, which usually lands on a float (reinterpreted as a u32 count) that trips `check_alloc`'s 256 MB cap or EOF, killing the entire NIF via the no-block-sizes truncation path at `crates/nif/src/lib.rs:632-646`. Aggregate in `Oblivion - Meshes.bsa`: **219 truncated scenes, 15 182 blocks dropped** (= 2.73 pp of the 97.27% clean rate).

Affected NIFs include every torch/fire/spell-impact effect (`fireball.nif`, `firearcanemedium01.nif`, `fxdustcloudfaint01.nif`, dozens more in `magiceffects\`, `effects\`, `fire\`). The 2026-04-26 blessed `Oblivion 96.24%` baseline already reflects this bug; a fix would push the clean rate toward ~99%+.

## Why #383's "Oblivion parsed cleanly" claim may have missed this

#383 was driven by FNV warning counts (5,837 → 1,274 = 78% reduction); the "Oblivion parsed cleanly" assertion looks like aggregate-parse-rate-level evidence. The 219 truncated Oblivion scenes are specifically the particle-heavy NIFs — and Oblivion v20.0.0.5 has no `block_sizes` table for recovery, so the truncation is hard. Static meshes in the Oblivion BSA (the bulk) wouldn't exercise this path. A fresh corpus-wide check that filters to `meshes\fire\`, `meshes\effects\`, `meshes\magiceffects\` is the right empirical baseline.

## Suggested Fix

Tighten the gate at `particle.rs:61`. Per nif.xml the two trailing floats are unconditional once `version >= 10.4.0.1` (Oblivion qualifies). Replace `if stream.bsver() >= 34` with `if stream.version() >= NifVersion::V10_4_0_1`. **Confirm against Gamebryo 2.3 `NiPSysEmitter::LoadBinary` before landing** — if the source disagrees with nif.xml, fall back to a Bethesda-specific predicate that includes Oblivion (`bsver >= 11 && version >= V20_0_0_5`).

After the fix, run `cargo run --release -p byroredux-nif --example nif_stats -- "Oblivion - Meshes.bsa"`; expect truncated count to drop from 219 toward 0, and `NiPSysAgeDeathModifier` / `NiPSysSpawnModifier` partial-unknown counts to fall to 0. Also re-run on `Fallout - Meshes.bsa` to verify FNV stays at the post-#383 100% baseline.

## Related

- #383 (CLOSED 2026-04-18): FNV/FO3 8-byte fix, opposite gate decision. Same root cause but the empirical claim about Oblivion needs revisiting per evidence above.
- #687 / #688 (CLOSED): Oblivion alloc-cap-truncation symptoms — both surfaced the same cascading-truncation pattern but were attributed to specific particle types rather than the emitter base.
- #395 (CLOSED): Oblivion stream drift detector.

## Completeness Checks

- [ ] **UNSAFE**: N/A (gate change only)
- [ ] **SIBLING**: re-check `skip_volume_emitter_base` + every `NiPSys*Emitter` parser that calls `skip_emitter_base` — they all inherit the same gate transitively
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: add a regression test with an Oblivion `NiPSysEmitter` fixture (use `make_header_oblivion` if it exists, or extend `make_header_fnv` pattern). Pin the consumed byte count at 56 (12+2 floats). Also add an FNV regression that pins the existing 56-byte consumption to prevent silent regression of #383.