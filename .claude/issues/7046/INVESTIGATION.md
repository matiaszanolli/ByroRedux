# #7046 investigation: Starfield.exe detection formula (2026-11-18)

Steam `Starfield.exe`, sha256 `7e9adb14…0f9d28`, PE32+, image base `0x140000000`, `.text` readable (61 MB).
Earlier notes: `.claude/issues/6996/INVESTIGATION.md` (Fallout 4; the tools in `.claude/issues/6996/tools/`).

## Step 1 — settings and defaults (done)

Every setting has its own initializer, 5132 float and 427 integer ones:

| Form | Bytes before the call |
|---|---|
| float | `vmovss xmm2,[rip+d]` (`C5 FA 10 15`) or `vxorps xmm2,xmm2,xmm2` (`C5 E8 57 D2`), `lea rdx,name` (`48 8D 15`), `lea rcx,obj` (`48 8D 0D`), `call 0x1402d65e0` |
| integer | `mov r8d,imm32` (`41 B8`), `lea rdx,name`, `lea rcx,obj`, `call 0x1402d66a0` |

The setting's value is at `obj + 8`; code reads it through a RIP-relative load of `obj + 8`. `scripts/exe-setting-defaults.py`
now reads both forms (the `initializer64` path). Cross-check: 151 sneak / detection / combat-stealth float settings read
by the script equal an independent bulk decode of every initializer (0 mismatches); Fallout 4 and Skyrim output unchanged.

Units are metres: `fSneakMaxDistance` 64 (Fallout 4: 4096 units), `fSneakVisualMinDistanceMax/Min` 4 / 2 (300 / 150),
`iLightLevelMax` **150** (Fallout 4: 300). Other defaults that differ from Fallout 4's table: `fSneakCombatMod` −0.4 (same),
new settings `fDetectionSprintingMult` 2, `fDetectionRunningMult` 2, `fSneakAmbientSoundMult` 1,
`fSneakTargetObservedDistanceMult` 1.25, `fSneakFlyingDistanceMult` 3.

## Step 2 — the formula (pair variant `0x140d7e2a0`, partly read)

Readers of the sneak settings cluster in `0x140d7de00..0x140d7e900` (the formula) and `0x141943c00..0x141944190`
(a second copy, not yet read). The context struct is Fallout 4's with a few bytes shifted:

| Offset | Use |
|---|---|
| +0x08 i32 | ambush mode |
| +0x0c i32 | size class |
| +0x10 i32 | action count |
| +0x18 f32 | light level |
| +0x1c / +0x20 f32 | visual / sound max distance |
| +0x24 f32 | Perception; +0x28 out P |
| +0x2c / +0x30 f32 | Blindness / Deafness |
| +0x34 / +0x38 f32 | distance / angle |
| +0x40 / +0x44 f32 | Agility / agility bonus; +0x48 out A |
| +0x4c / +0x50 f32 | equipped weight / movement-noise mult |
| +0x54 / +0x5c / +0x60 f32 | out darkness / visual / sound |
| +0x64 sees, +0x65 LOS, +0x66 exterior, +0x67 alert, +0x68 asleep | |
| +0x6a moving, +0x6b running, **+0x6c sprinting (new)**, +0x6d sneaking, **+0x6e stealth boy**, +0x71 target is combat target | |
| **+0x74 i32** | **ambient sound level class of the detector (new)**, 0 = none, 1..5 |

Differences from Fallout 4 (`crates/core/src/stealth/fallout4.rs`), each read at the address named:

1. **Ambient sound** (`0x140d7dd80`, called at `0x140d7e54c`). `ambient = trunc(fSneakAmbientSoundMult × table[class])`,
   `table` = `iSoundLevelSilent` 10, `Quiet` 25, `Normal` 50, `Loud` 100, `VeryLoud` 200 for class 1..5, 0 otherwise.
   The sound term is `(soundsMult · att · los · (s + action) − ambient) · (1 + asleep · sleepBonus) · (100 − Deafness) · 0.01`
   with **no clamp at 0** in the pair variant, so the sound value can fall below `fSneakSoundBaseValue`. The event variant
   (`0x140d7dff0`) clamps at 0 (`vmaxss` at `0x140d7e1d7`).
2. **Movement multiplier** (`0x140d7e478..0x140d7e4bc`): sprinting (+0x6c) → `fDetectionSprintingMult`; running (+0x6b) →
   `fSneakRunningMult` when sneaking (+0x6d), else `fDetectionRunningMult`; otherwise 1.
3. **Skill-movement term only while moving** (`0x140d7e523`): `s = max(0, [moving] · (att·los·(ambush+1)·D − A) · fSneakSkillMovementMult + move)`.
4. **Stealth boy** (+0x6e) skips the whole visual addition (`0x140d7e7e7` → `0x140d7e86f`), not just its first term.
5. **Range** (`0x140d7de10`): `fSneakMaxDistance · [exterior: fSneakExteriorDistanceMult] · [observed target: fSneakTargetObservedDistanceMult]
   · [flying: fSneakFlyingDistanceMult] · [size > 1: fSneakLargeActorDistanceMult · size]`.
6. Units in metres; `iLightLevelMax` 150.

Unchanged: P, D, A, the visual terms, the close-range floor, `fSneakSkillMult`, the perk entry 0x30 (48) on the light-move term
(`0x140d7e6d8`: `call 0x140c94060(0x30, …)`).

## Not yet read

The context filler (the Starfield counterpart of Fallout 4's `0x140cc00c0`), the rounding caller, the stealth-point
thresholds and drain/refill, the sneak-attack table, `0x140d7e8e0` (a routine that inverts the attenuation, probably the
"distance at which the target becomes visible" for the HUD), and the second copy near `0x141943c00`.
