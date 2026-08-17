# FO4-D6-04: FO4 AMMO DATA is decoded through the 13-byte FO3/FNV layout, and its DNAM has no arm

**Issue**: #2995
**Severity**: MEDIUM
**Dimension**: 6 — ESM item records
**Labels**: `medium,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO4_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO4_2026-08-16.md` (Dimension 6 — ESM item records).

**Location**: `crates/plugin/src/esm/records/items.rs`:433-438 (`DATA`), :454-460 (`DAT2`, FNV-gated)

## Description

FO4 AMMO `DATA` is 8 bytes — `Value(u32) @0, Weight(f32) @4`. The shared FO3/FNV arm reads `speed(f32)` over the value and `flags_pad(u32)` over the weight, consuming the whole buffer; the subsequent `value` and `clip_rounds` reads hit EOF and take their `_or_default` zero.

The `DAT2` weight fallback is gated `matches!(game, GameKind::Fallout3NV)` and **FO4 emits no `DAT2`**.

Separately, FO4 AMMO's 16-byte `DNAM` (`Projectile(FormID), Flags(u8), unused[3], Damage(f32), Health(u32)`) has **no parser arm of any kind**.

## Evidence

`DATA` 8 B on 57/57, `DNAM` 16 B on 57/57.

Authored values the parser discards: `Ammo10mm` value 2 / weight 0.025; `Ammo556` 2 / 0.035; `AmmoMissile` 25 / 7.0; `AmmoFusionCore` 200 / 4.0 with `DNAM` health 500.

Re-verified 2026-08-17: the shared arm's comment even admits the bucketing — *"FO4 grouped here pending its own arm; weight comes from DAT2"* — while `DAT2` is FNV-gated.

## Impact

All 57 FO4 ammo types decode to value 0 and weight 0, so ammo is weightless and worthless in inventory. The `DNAM` `Health` field (fusion-core charge) is unreachable.

`damage` staying 0 is **not itself wrong** — FO4 moved per-shot damage onto the weapon — but nothing records that, so the zero is indistinguishable from the two real bugs beside it.

## Suggested Fix

Add a `GameKind::Fallout4` `DATA` arm (`value, weight`) and a `DNAM` arm for projectile/health. **Comment the deliberate `damage = 0`** so it reads as a decision rather than a fourth bug.

## Related

- #2992 (FO4-D6-01 — same "FO4 shares the FO3/FNV arm" root)

## Completeness Checks
- [ ] **SIBLING**: The `DAT2` FNV gate re-checked — is it right for every non-FO4 game?
- [ ] **LEGIBLE-ZERO**: The intentional `damage = 0` is commented, so it is distinguishable from a bug
- [ ] **TESTS**: A regression test pins a known FO4 ammo value and weight

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2995 --json state` when live state is needed.*
