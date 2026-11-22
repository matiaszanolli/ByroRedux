# #6996 investigation: Fallout4.exe detection formula (2026-11-16)

Steam `Fallout4.exe` (PE32+, image base 0x140000000). Setting objects are static
`{vtable, value, name*}`: the value sits 8 bytes before the name pointer, so a
RIP-relative load names its setting (`tools/fdis.py <exe> <va> <len>` annotates
them; `tools/xref64.py <exe> <va>...` lists `E8`/`E9` callers). Defaults below are
the executable's (`scripts/exe-setting-defaults.py`). 137 strings match
`^[fi](Sneak|Detection|Detect|CombatStealth|CombatSneak)`.

## Status (built 2026-11-16)

Fallout 4 is fully decoded and built as `crates/core/src/stealth/fallout4.rs` (its module docs carry the
formula); `tools/ripref.py <exe> <setting>...` lists every RIP-relative reader of a setting, which is how each
routine below was found.

| Routine | What |
|---|---|
| `0x1406c8770` | per-pair formula: sound value `+0x60`, visual value `+0x5c` |
| `0x1406c8580` | the same for a detect event (noise / sight event) |
| `0x1406c82a0`, `0x1406c8350` | detection range (exterior, observed, flying, size class) and its attenuation |
| `0x140cc00c0` | fills the context: range, LOS reach, view cone, visual range falloffs, visibility, light, AVs |
| `0x140c98900` | rounds both values up (0 → −1), falls back to projectiles / detect events |
| `0x140e94420` | stealth-point thresholds, attack / event caps, known / lost bookkeeping |
| `0x140e9f170` | stealth-point drain (with the sound floor) and the eased refill |
| `0x140479660`, `0x140e3a1ae` | sneak-attack class table (12 types) and entry 18 |
| `0x140d38f49` | light level: 100·luminance − exterior mod or min(100·ambient, interior mod) |
| `0x14101b560` | lerp(x, x0, x1, from, to), clamped |

The actor values are named from CommonLibF4's `RE::ActorValue` singleton (`ActorValueInfo.h`): `+0x018` Agility,
`+0x050` Blindness, `+0x0B0` Deafness, `+0x108` Invisibility, `+0x160` MovementNoiseMult, `+0x168` NightEyeBonus,
`+0x170` Perception, `+0x228` Sneak. `Fallout4.esm` authors only Perception, Agility and Invisibility as `AVIF`.

Not modelled by the engine yet (inputs stay neutral): ambush packages (`+0x08`), the combat group's "observed"
target and flight (range mults), cover rays (`fDetectionCover*`), the occluder count, the
`fDetectionCombatNonTargetDistanceMult` (non-player targets only), the search-location refill delay, and
per-combat-group stealth points (one pool for the player).

**Starfield.** `Starfield.exe`'s `.text` is readable, but no setting is a static object: each has an initializer
(`vmovss xmm2, [default]` / `vxorps`, `lea rdx, name`, `lea rcx, object`, `call 0x1402d65e0`; e.g.
`fSneakMaxDistance` at `0x1400f1700`). Its row needs that form in `scripts/exe-setting-defaults.py` and a decode
of the AVX formula; not done here.

## Context struct (`r8` / `rbx`), as the formula reads it (first pass; names above)

| Offset | Type | Use (inferred) |
|---|---|---|
| +0x08 | i32 | ambush mode: non-zero adds `fSneakAmbushTargetMod`; 1 also skips the sound LOS penalty; the sound skill term is multiplied by (mode + 1) |
| +0x0c | i32 | target size class: 0 Small, 1 Normal, 2 Large, 3 VeryLarge (`fSneakDetectionSize*`), else 1 |
| +0x10 | i32 | action count (× `fSneakActionMult`) |
| +0x18 | f32 | light level at the target (0..`iLightLevelMax` = 300) |
| +0x1c | f32 | visual max distance |
| +0x20 | f32 | sound max distance |
| +0x24 | f32 | detector Perception (1..10) |
| +0x28 | f32 | out: perception term P |
| +0x2c | f32 | detector blindness 0..100 (visual × (100 − x)/100) |
| +0x30 | f32 | detector deafness 0..100 (sound skipped at ≥ 100, else × (100 − x)/100) |
| +0x34 | f32 | distance |
| +0x38 | f32 | angle off the view axis (radians) |
| +0x40 | f32 | target Agility (1..10) |
| +0x44 | f32 | target agility-term bonus (added to `fSneakAgilitySkillMin`) |
| +0x48 | f32 | out: agility term A |
| +0x4c | f32 | target equipped weight |
| +0x50 | f32 | target noise multiplier (clamped ≥ 0) |
| +0x54 | f32 | out: darkness term |
| +0x5c | f32 | out: visual value |
| +0x60 | f32 | out: sound value |
| +0x64 | u8 | detector sees the target (LOS and view cone) |
| +0x65 | u8 | line of sight (for the sound penalty) |
| +0x66 | u8 | exterior |
| +0x67 | u8 | detector alert |
| +0x68 | u8 | detector asleep |
| +0x6a | u8 | target moving |
| +0x6b | u8 | target running |
| +0x6c | u8 | target sneaking |
| +0x6d | u8 | stealth boy |
| +0x70 | u8 | target is the detector's combat target |

## Formula (`0x1406c8770`)

```
P    = fSneakPerceptionSkillMin + (Max − Min) · Perception · 0.1             # 10 + 80·Per/10
mod  = 1 + alert·fSneakAlertMod + asleep·fSneakSleepMod
         + (alert ∧ ¬combatTarget)·fSneakCombatMod + (ambush ≠ 0)·fSneakAmbushTargetMod
D    = mod · P
A    = sneaking ? fSneakAgilitySkillMin + bonus + (AgiMax − AgiMin) · Agility · 0.1 : 0   # 0..50

sound = fSneakSoundBaseValue                                                  # −20
if deafness < 100:
    los   = (LOS ∨ ambush = 1) ? 1 : fSneakSoundLosMult                       # 0.3
    att_s = max(0, (soundMax − d) / soundMax) ^ fSneakDistanceAttenuationExponent
    move  = moving ? (fSneakEquippedWeightBase + fSneakEquippedWeightMult · weight)
                     · max(0, noise) · (running ? fSneakRunningMult : 1) : 0  # 12 + 0.5·w; ×2
    s     = max(0, ((ambush + 1) · att_s · los · D − A) · fSneakSkillMovementMult + move)   # 0.25
            + action · fSneakActionMult
    sound += fSneakSoundsMult · att_s · los · s · (1 + asleep · fSneakSleepBonus)
             · (100 − deafness) · 0.01

visual = fSneakVisualBaseValue                                                # −20
if sees ∧ ¬asleep:
    att_v = max(0, (visualMax − d) / visualMax) ^ exponent
    lm    = moving·fSneakLightMoveMult + running·fSneakLightRunMult           # 0.2, 0.4
    lm    = perk entry point 0x30 (48) applied to lm (call 0x140618860(0x30, rdx-arg, ctx, &lm))
    light = light · (exterior ? fSneakLightExteriorMult : fSneakLightMult) + fDetectionSneakLightMod   # 0.5 / 0.33, +15
    size  = fSneakLargeActorDistanceMult · fSneakDetectionSize[class]         # 1 · {0.5, 1, 1.5, 2}
    boy   = stealthBoy ? fSneakStealthBoyMult : 1                             # 0
    dark  = max(0, (iLightLevelMax − lightLevel) / iLightLevelMax) · A · fSneakSkillMult   # 300, 0.5
    visual += (1 + lm) · att_v · light · (100 − blindness) · 0.01 · boy · size
              + att_v · D · fSneakSkillMult − dark
    # Close-range floor (not under a stealth boy): within fSneakVisualMinDistanceMax (300)
    # and inside fSneakVisualMinAngle (45°), raise it toward fSneakVisualMinValue (25)
    if ¬stealthBoy ∧ visual < 25 ∧ 0 < d < 300 ∧ angle < 45°:
        t = clamp((d − 300) / (fSneakVisualMinDistanceMin − 300), 0, 1)       # Min = 150
        visual += t · (25 − visual)
```

`0x1406c8580` is the same shape for a detect event (a noise or sight event, not the
actor). Its range is `min(fDetectEventMaxDistanceMult (2) · eventRange,
fSneakMaxDistance (4096)) · (exterior ? fSneakExteriorDistanceMult (1.25) : 1)`. It
has no agility, movement, size or close-range terms.

## Caller `0x140c98900`

- Visual and sound are each rounded **up** to an integer, and 0 becomes −1. They
  are stored as i16 at ctx +0x80 (visual) and +0x82 (sound), and the detector's
  position at +0x74..+0x7c.
- When max(visual, sound) < 0, the caller checks:
  - projectiles within `fDetectProjectileDistancePlayer` / `NPC` (512 / 256);
  - detect events within `fDetectEventDistancePlayer` / `NPC` (2048 / 512), × 2 for
    `iSoundLevelVeryLoud` (200);
  - sight of the event inside `fDetectionViewCone` (190°).
  It runs the event formula and sets +0x80 / +0x82 to 0 (an event heard / seen) or
  −1000, with +0x84 = 2.

## Defaults read

`fSneakSkillMovementMult` 0.25, `fSneakAmbushTargetMod` 0.5, `fSneakAmbushNonTargetMod`
−0.25 (not read by `0x1406c8770`), `fSneakStealthBoyMult` 0, `fSneakLargeActorDistanceMult`
1, `fSneakVisualMinValue` 25, `iLightLevelMax` 300, `fCombatSneakGunMult` /
`GrenadeMult` / `MineMult` 2, `iCombatStealthPointSneakDetectionVisualThreshold` 70,
`…SoundThreshold` 40, `fCombatStealthPointDrainSoundMult` 1.5, `…VisualMult` 3.
