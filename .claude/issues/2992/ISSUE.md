# FO4-D6-01: Every Fallout 4 weapon decodes to all-zero stats — the 132-byte DNAM is never read

**Issue**: #2992
**Severity**: HIGH
**Dimension**: 6 — ESM item records
**Labels**: `high,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO4_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO4_2026-08-16.md` (Dimension 6 — ESM item records).

**Location**: `crates/plugin/src/esm/records/items.rs`:166-234 (the `b"DATA"` and `b"DNAM"` arms of `parse_weap`)

## Description

`parse_weap` folds `GameKind::Fallout4` into the `GameKind::Fallout3NV` arm of the `b"DATA"` match, and gates the `b"DNAM"` arm on `matches!(game, GameKind::Fallout3NV)` alone.

**Fallout 4 emits no `DATA` sub-record on WEAP** — its entire weapon stat block is a 132-byte `DNAM`. Neither arm ever executes, so `common.value`, `common.weight`, `damage`, `clip_size` and `anim_type` all fall through to their zero initializers on every FO4 weapon.

The parser's own comment acknowledges the mis-bucketing (*"FO4 groups here pending its own per-game arm"*) but the referenced follow-up never landed.

## Evidence — measured, `Fallout4.esm`

```
WEAP records          252
with a DATA sub-record  0
with a DNAM sub-record 252   (all exactly 132 bytes)
```

The xEdit `wbStruct(DNAM, 'Data', …)` for FO4 WEAP sums to **exactly 132 bytes**, matching the measured length on 252/252. It is a packed, unaligned struct:

| Off | Type | Field | | Off | Type | Field |
|---|---|---|---|---|---|---|
| 0 | u32 | Ammo (`AMMO`\|NULL) | | 69 | u32 | Sound Level |
| 4 | f32 | Speed | | 73 | u32 | Sound - Attack (`SNDR`) |
| 8 | f32 | Reload Speed | | 77 | u32 | Sound - Attack 2D |
| 12 | f32 | Reach | | 81 | u32 | Sound - Attack Loop |
| 16 | f32 | Min Range | | 85 | u32 | Sound - Attack Fail |
| 20 | f32 | Max Range | | 89 | u32 | Sound - Idle |
| 24 | f32 | Attack Delay | | 93 | u32 | Sound - Equip |
| 28 | f32 | Unused | | 97 | u32 | Sound - UnEquip |
| 32 | f32 | Damage - OutOfRange Mult | | 101 | u32 | Sound - Fast Equip |
| 36 | u32 | On Hit (enum) | | 105 | u8 | Accuracy Bonus |
| 40 | u32 | Skill (`AVIF`\|NULL) | | 106 | f32 | Animation Attack Seconds |
| 44 | u32 | Resist (`AVIF`\|NULL) | | 110 | `[2]` | *unknown* |
| 48 | u32 | Flags | | 112 | f32 | Action Point Cost |
| 52 | u16 | Capacity | | 116 | f32 | Full Power Seconds |
| 54 | u8 | Animation Type | | 120 | f32 | Min Power Per Shot |
| 55 | f32 | Damage - Secondary | | 124 | u32 | Stagger (enum) |
| 59 | f32 | **Weight** | | 128 | `[4]` | *unknown* |
| 63 | u32 | **Value** | | | | |
| 67 | u16 | **Damage - Base** | | | | |

Three independent validations, none of them inference:

1. **FormID membership against the real tables in the same file.** At exactly these offsets, offset 0 is `0` or a genuine `AMMO` FormID on **252/252**, and the eight `SNDR` slots at 73…105 are `0` or a genuine `SNDR` FormID on **2016/2016**. A misalignment control re-running the same membership test shifted by −4/−2/−1/+1/+2/+4 bytes fails in every case (154 / 723 / 425 / 698 / 890 / 239 bad). Only the xEdit offsets are clean.
2. **A semantic anchor independent of any published stat table.** The VR shooting-range clones author their damage as `base + 1000`, and offset 67 reproduces that relationship exactly: 10mm 18→1018, Combat Rifle 33→1033, Combat Shotgun 50→1050, Missile Launcher 15→1015, Minigun 8→1008.
3. **Cross-check of known vanilla values.** `10mm` reads damage 18 / weight 3 / value 45 / capacity 12 / AP 28 / anim 9 (`Gun`); `Fatman` weight 30 / value 500 / capacity 1 / AP 60 (highest in game); `BaseballBat` damage 16 / weight 3 / anim 5 / reach 0.8; `Switchblade` damage 8 / weight 1 / anim 1.

**Explicitly unconfirmed — do not encode these as semantics**: offsets 110 `[2]` and 128 `[4]` are byte-constant across all 252 records (`0x0000`, `ff ff 7f 7f`), so nothing about their meaning is observable from this corpus; xEdit likewise calls both `wbByteArray('Unknown')`. Offsets 40 and 44 (Skill / Resist `AVIF`) are `0` on 100% of vanilla, so their position is supported only by the fields bracketing them.

Re-verified 2026-08-17: the `Fallout3NV | Fallout4` `DATA` arm and the `Fallout3NV`-only `DNAM` gate are both present and unchanged.

## Impact

All 252 FO4 weapons carry damage 0, value 0, weight 0, clip 0. **Live in the P2 gameplay slice, not latent:**

- `byroredux/src/inventory.rs`:224 (`prefer_weapon`) and `byroredux/src/npc_spawn.rs`:782 both select the equipped weapon by "highest base damage, then lowest FormID"; with damage uniformly zero the rule degenerates to lowest-FormID, so *which* weapon an FO4 actor equips is arbitrary.
- `byroredux/src/combat.rs`:269 (`attack_damage`) reads `EquippedWeapon.damage` and clamps with `.max(0.0)` → **0.0**, so an armed FO4 actor deals strictly less damage than the `UNARMED_DAMAGE = 8.0` fallback (which only applies when no `EquippedWeapon` exists at all).
- Inventory presentation (`inventory.rs`:106) renders every FO4 weapon as "Damage 0".

## Suggested Fix

Add a `GameKind::Fallout4` arm to the `b"DNAM"` match decoding the table above, and drop `Fallout4` from the FO3/FNV `b"DATA"` arm (unreachable for WEAP anyway).

Add a real-data assertion in `crates/plugin/tests/parse_real_esm.rs` that no game's WEAP population decodes to a uniformly-zero `damage` — **that single assertion would have caught this, and catches the four siblings at the same time.**

## Related

- `PAT-D6-2026-08-16-01` (`/audit-legacy-compat`, same sweep) first identified the missing-`DATA` mechanism; this adds the validated decode, the misalignment control, and the P2 blast radius
- Sibling gap `PAT-D6-2026-08-16-02` (no canonical reach/speed field) is unblocked by the layout above — reach is offset 12, speed offset 4

## Completeness Checks
- [ ] **SIBLING**: The four other FO4 item records in this cluster (ARMO/AMMO/BOOK, and the `FNAM` gap) fixed in the same pass
- [ ] **NO-GUESSING**: Offsets 110/128 and the Skill/Resist slots left unencoded — they are unverified against shipped data
- [ ] **REAL-DATA-GUARD**: The uniformly-zero-damage assertion added, so this class cannot recur silently
- [ ] **P2-SLICE**: `prefer_weapon` and `attack_damage` verified to see real damage after the fix
- [ ] **TESTS**: A regression test pins this specific fix

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2992 --json state` when live state is needed.*
