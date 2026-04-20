# Issue #439

FO3-3-01: HEDR=0.94 misclassified as Fallout4 — latent FO3 corruption risk

---

## Severity: Critical

**Location**: `crates/plugin/src/esm/reader.rs:110-137` (`GameKind::from_header`)

## Problem

Fallout3.esm (GOTY) ships `HEDR = 0.94f32` (bytes `d7 a3 70 3f` at offset 0x1e — verified across all six FO3 masters: Fallout3, Anchorage, BrokenSteel, PointLookout, ThePitt, Zeta). The band `(0.94..=0.955)` at line 126 routes this to `GameKind::Fallout4`. Meanwhile Fallout4.esm actually ships `HEDR = 1.0` — the existing FO4 band matches **no real FO4 file**.

The stale comment at lines 114-120 still says `FO3 = 0.85`, which is the pre-patch value bumped to 0.94 pre-GOTY.

## Impact

Silent today: WEAP/ARMO/AMMO DATA arms (`items.rs:150/253/274/322`) bucket Fallout4 with FO3NV/Oblivion, so current parsing happens to work. Latent: the first FO3↔FO4 schema split after this lands (BGSM refs, dual-weapon SCOL, BOD2 armor types) corrupts FO3 data silently.

## Fix

Reorder bands so FO3 catches 0.935..=0.945, FO4 catches 0.96..=1.04, Starfield stays at 0.955..=0.959. Update comment block at 114-120 with correct values.

## Completeness Checks

- [ ] **TESTS**: Regression test pinning HEDR → GameKind for one real master of each game
- [ ] **SIBLING**: Review all `GameKind::Fallout4` arms (`items.rs`) — verify none silently rely on FO3 misbucketing
- [ ] **DOCS**: Update comment block with correct values

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-3-01)
