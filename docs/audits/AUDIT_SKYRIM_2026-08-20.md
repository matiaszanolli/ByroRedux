# Skyrim SE Compatibility Audit — 2026-08-20

**Command**: `/audit-skyrim` (run as part of the `comprehensive` `/audit-suite` sweep)
**Repo HEAD**: `bb0b92f2`
**Delta audited**: 335 commits since `adbc3f77` / the 2026-08-16 sweep (`8e7582ed~1..HEAD`)
**Game data**: `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/` (present — `Skyrim.esm`, `Update.esm`, `Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm`, `_ResourcePack.esl` and 16 BSAs)
**Dedup baseline**: `/tmp/audit/issues.json` (400 issues, #2671–#3103) + `docs/audits/AUDIT_SKYRIM_2026-08-16.md` + the four already-confirmed sibling findings handed down in the run brief.

---

## Scope line

All **7 dimensions** were executed by this agent directly (no sub-agents, per the
briefing).

**Method constraint**: no `cargo` invocation and no engine launch were permitted
this pass. Corpus evidence below was produced with a **stand-alone Python ESM
walker** (GRUP recursion + zlib record decompression + subrecord split, incl.
`XXXX` big-size) run directly against the five installed Skyrim masters — no
repo code executed, so every number is an independent measurement of the *data*,
which the source is then read against. The NIF/BSA corpus could **not** be
re-swept (BSA v105 needs LZ4 and the sandbox Python has no `lz4` module; the
`_tmp_sk_*` cargo examples were off-limits).

**Consequently not exercised this pass** and carried unchanged from 2026-08-16:
Whiterun BanneredMare control-bench entity/FPS, `tex.missing`, the Meshes0/1
parse sweep, and the full BSA v105 extraction sweep (Dimension 5 — its source
tree `crates/bsa/src/archive/` has **zero commits in the delta**, so the
100 % / 89 498-file result stands by construction).

**Open investigation left alone**: the diagonal double-image ghost in Skyrim
interiors. No new evidence, no claim, no speculative fix — per the brief.

---

## Executive Summary

The 2026-08-16 report's four findings are **three fixed and one partially
addressed** at HEAD: #3068 (slot-2 glow-map flag) and #3069 (`LVLF` Use All) are
CLOSED and the fixes verified in place; #3070's premise was removed wholesale by
#3036 (the BSXFlags whole-NIF gate is gone) and the issue should be closed;
#3071 (slot-7 back-lighting) is unrouted still but the loss is now *counted* by
`record_unrouted_texture_slot`.

The delta's centre of gravity — 60+ WATAL water commits — landed on the game
WATAL was modelled around, and that is where this pass's yield is. **Five NEW
findings**: one delta-introduced rendering regression that collapses Skyrim's
multi-layer water normal stack to a single layer on 34/34 vanilla `WATR`
records; the **unfixed half** of the closed #3069 (`LVLF` bit `0x02` is treated
as "take all", blowing every outfitted NPC's inventory up ~15×); two parser
fidelity gaps (RACE Magicka/Stamina never read; underwater-fog clamps producing
a degenerate span on `HelgenWater`); and one unreachable-by-construction CHARAL
GMST overlay.

Of the four sibling findings handed down, **three are confirmed on Skyrim data
and one is STALE at HEAD** — `wind_direction` is no longer the raw `90.0`
constant on Skyrim, and a fourth carries a warning against "fixing" the WATR-side
blend flag by analogy with the NIF-side one, because the WATR side is
empirically **correct**.

---

## Dimension roll-call

| # | Dimension | NEW findings | Verdict |
|---|-----------|-------------:|---------|
| 1 | BSTriShape packed geometry + SSE skinned reconstruction | **0** | Delta reviewed (#2578, #2598, #2828); `min_vertex_bytes` mirrors `decode_bs_vertex_stream` exactly, SSE full-precision gate correct |
| 2 | `BSLightingShaderProperty` / `BSEffectShaderProperty` dispatch | **0** | Skyrim arms untouched by the delta; #6d7df853 is Starfield-only; #3068 fix verified |
| 3 | NPC equip + FaceGen (M41) | **1** (D3-01 HIGH) | #3069's `0x04` half fixed; its `0x02` half is the finding |
| 4 | Multi-master load order + TES5 cell-load regression | **2** (D4-01 MEDIUM, D4-02 LOW) | `.STRINGS` / ESL / tombstone guards all live; the gaps are in per-title record semantics |
| 5 | BSA v105 (LZ4) | **0** | Zero delta commits; 2026-08-16 result carried |
| 6 | Specialty blocks + real-data rendering | **1** (D6-01 HIGH) | #838 three-arm split intact, `.btr`/`.bto` gates intact; water.frag is the finding |
| 7 | NIFAL / WATAL canonical translation (Skyrim slice) | **1** (D7-01 MEDIUM) | Single boundary intact; the defect is inside the Skyrim WATR decode |

**Totals — 5 NEW findings: 0 CRITICAL · 2 HIGH · 2 MEDIUM · 1 LOW.**

---

## Findings

### SKY-2026-08-20-D6-01: `water.frag`'s new blend-normals gate discards every authored normal layer but the first on 34/34 vanilla Skyrim `WATR` records

- **Severity**: HIGH
- **Dimension**: 6 — real-data rendering (delta-introduced)
- **Location**: `crates/renderer/shaders/water.frag:699-723` (`blendAuthoredNormals`, and the unconditional `if (!blendAuthoredNormals) { nMix = nA; }` at `:722`); gate source `crates/plugin/src/esm/records/misc/water.rs:1308-1310`; carrier `byroredux/src/render/water.rs:287-291`
- **Status**: NEW — introduced by `1158a916` ("feat(water): honor Skyrim normal blend flag", 2026-08-20), inside this delta
- **Changed File**: yes (`crates/renderer/shaders/water.frag`, #7 on `/tmp/audit/hot_files.txt`)
- **Description**: The commit added a gate on Skyrim `WATR.FNAM` bit `0x10`:
  ```glsl
  bool blendAuthoredNormals = push.noise_falloff.y > 0.5;
  bool hasAuthoredThirdLayer = noiseMapC != noiseMapA && noiseMapC != noiseMapB;
  if (blendAuthoredNormals && (kind == WATER_RAPIDS || hasAuthoredThirdLayer)) {
      ... nMix = normalize(nA + nB + nC * thirdWeight);
  } else {
      nMix = normalize(nA + nB);
  }
  if (!blendAuthoredNormals) {
      nMix = nA;          // <-- discards layer B unconditionally
  }
  ```
  Pre-commit the surface was always `normalize(nA + nB)` (or `+ nC`). The new
  trailing statement is not a *blend-mode* switch: it deletes the second normal
  layer outright. The in-code comment claims "legacy records use the canonical
  `1.0` default and retain the layered path" — true for every game *except*
  Skyrim, which is the only game that supplies the flag at all, and which
  supplies it **clear**.
- **Evidence**: `WATR.FNAM` first byte over the installed masters (Python ESM walk, all records, no sampling):
  ```
  Skyrim.esm      34 records   FNAM = 0x00 on 34/34
  Update.esm      38 records   0x00 ×34, 0x08 ×2, 0x18 ×2
  Dawnguard.esm   11 records   0x00 ×11
  Dragonborn.esm   7 records   0x00 ×5,  0x01 ×2
  ----------------------------------------------------
  TOTAL           90 records   bit 0x10 SET on 2  (2.2 %)
  ```
  So `blend_normals = Some(false)` on **34/34 of `Skyrim.esm`** and 88/90
  across the whole install, and `nMix = nA` fires on essentially all Skyrim
  water. `nA` and `nB` are *not* redundant samples even when the three
  `NAM2/NAM3/NAM4` slots collapse to one texture (only 3/34 `Skyrim.esm`
  records author all three, 0/34 author three *distinct* ones): they are
  sampled with the separately-authored per-layer scroll and UV scale that the
  same delta went to considerable length to decode —
  `DNAM[100/104/108]` wind directions (26/25/25 distinct values across 34
  records), `DNAM[112/116/120]` wind speeds (26/21/27 distinct), and
  `DNAM[172/176/180]` UV scales (21/24/23 distinct). Layer B's entire authored
  parameter set is now dead on Skyrim.
- **Impact**: 100 % of vanilla Skyrim water surfaces render a single-layer
  normal instead of the two-to-three-layer stack — visibly coarser, more
  repetitive, more obviously tiled, and a straight visual regression against
  the pre-delta build. Blast radius is every Skyrim exterior and every water
  interior; `WATR.FNAM` is Skyrim-only in the decoder (`water.rs:1308`), so no
  other title is touched.
- **Related**: sibling finding #4 (mesh-side `BSWaterShaderProperty` bit 16 —
  the *same defect class* on the NIF path: an unsourced bit inverting a
  canonical default); `f933ecbf` / `71c694b0` and the rest of the Skyrim DNAM
  tail work whose decoded per-layer parameters this gate strands.
- **Suggested Fix**: Two separable steps. (1) Delete the trailing
  `if (!blendAuthoredNormals) { nMix = nA; }` — the gate already has a legitimate
  effect through the third-layer branch, and no source says "blend normals off"
  means "one layer". (2) Source the bit before re-adding any semantic: the
  strongest available evidence is the EDID pair
  `Update.esm 01001234 DefaultWaterFlow FNAM=0x08` vs
  `01001235 DefaultWaterFlowBlend FNAM=0x18` (and the identical
  `RiverWaterFlow` / `RiverWaterFlowBlend` pair) — **both** carry `NAM5` flow
  maps, so on this evidence "Blend" reads as *blend the flow-map normal with
  the noise normals*, which is a different switch entirely and touches only
  flow water.

---

### SKY-2026-08-20-D3-01: TES5 `LVLF` bit `0x02` is a per-count re-roll, not "take all" — 192 of 279 outfit-reachable Skyrim leveled lists expand as bundles, blowing outfits up ~15×

- **Severity**: HIGH
- **Dimension**: 3 — NPC equip (M41)
- **Location**: `crates/plugin/src/equip.rs:411` (`let multi_pick = lvli.flags & (0x02 | 0x04) != 0;`); doc premise at `:346-350` and `:398-401`
- **Status**: NEW — this is the **unfixed half** of `Existing: #3069` (CLOSED). #3069 fixed the `0x04` "Use All" half exactly as recommended; its own Impact section also called out "the inverse error is also present … over-equipping", and that half shipped unchanged.
- **Description**: The resolver treats `0x02` and `0x04` as interchangeable
  multi-pick triggers. They are not. TES5 `LVLF` bit 2 (`0x04`, xEdit "Use All")
  means *add every entry*; bit 1 (`0x02`, "Calculate for each item in count")
  means *repeat the single roll `count` times*. Expanding every eligible entry
  on a `0x02` list turns a **level-tier ladder** into a bundle.
- **Evidence**: Python walk of `Skyrim.esm`'s OTFT→LVLI closure (481 OTFT,
  3 075 LVLI, 5 118 NPC_):
  ```
  OTFT-reachable LVLI:                    279
    LVLF histogram: 0x03 ×162, 0x04 ×49, 0x02 ×30, 0x00 ×37, 0x01 ×1
    bit 0x02 set AND bit 0x04 clear:      192   (69 %)
  OTFTs containing >= 1 such list:      70 / 481
  NPC_ whose default outfit does:     1491 / 5118  (29 %)
  ```
  A representative list — unambiguously a tier ladder, not a set:
  ```
  LVLI 000FDA10 LItemEnchArmorHeavyGauntletsNoDragon  flags=0x03
      lvl= 1 SublistEnchArmorIronGauntlets01
      lvl= 4 SublistEnchArmorIronGauntlets02
      lvl= 7 SublistEnchArmorSteelGauntlets01
      lvl=13 SublistEnchArmorDwarvenGauntlets02
      lvl=19 SublistEnchArmorSteelPlateGauntlets02
      lvl=26 SublistEnchArmorOrcishGauntlets03
      lvl=33 SublistEnchArmorEbonyGauntlets03      (18 entries total)
  ```
  and each sublist is itself `flags=0x03` with five enchantment variants, so the
  expansion multiplies. Simulating both rules over every NPC that has an outfit
  (3 633 of them) at `actor_level = 38`:
  ```
  mean flattened outfit size, current rule (0x02|0x04):   38.74 items
  mean flattened outfit size, single-pick on 0x02:         2.50 items
  NPCs whose outfit flattens to > 20 items:             1238 / 3633
  worst case: dunIronbindBeemJa                          1612 items  (single-pick: 5)
              DA13Orchendor                               219 items  (single-pick: 6)
              DA13EncAfflicted05* family (×several)       196 items  (single-pick: 2)
  ```
- **Impact**: 34 % of outfitted `Skyrim.esm` NPCs receive 20+ spurious inventory
  rows, mean 15× over-population, worst case 1 612 `ItemStack`s on a single
  actor. The *worn* result is mostly salvaged by downstream luck —
  `equipment_slots.equip()` is last-write-wins over the expansion order
  (`byroredux/src/npc_spawn.rs:866`) and the weapon picker takes max damage
  (`:846-853`), so the highest tier usually ends up on the body — but every
  inventory-facing surface (loot, barter, pickpocket, save-snapshot size,
  per-actor allocation) is wrong by an order of magnitude, and the "usually"
  is an accident of ordering, not a rule. FO3/FNV/Oblivion `LVLF` has no
  `0x04`, so the shared `0x02` arm makes this Skyrim/FO4-shaped.
- **Related**: #3069 (closed; fixed the complementary half), #896 (LVLI dispatch
  in outfits), #2094 (occupancy filter — operates on meshes, does not prune the
  inventory), #2956 (`Use Stats` template inheritance, same delta)
- **Suggested Fix**: Make `0x04` the sole multi-pick trigger and route `0x02`
  back to the single-pick arm (its faithful meaning — repeat the roll `count`
  times — is the same base form ID `count` times, which for `count == 1`, the
  vanilla case in every list sampled above, is exactly single-pick). Pin with a
  fixture LVLI carrying `flags = 0x03` and level-1/4/7 entries asserting one
  expansion, alongside the existing `flags = 0x04` all-expand guard at `:784`.

---

### SKY-2026-08-20-D4-01: TES5 `RACE.DATA` starting Magicka and Stamina are never parsed, so no Skyrim actor ever gets a Magicka or Stamina actor value

- **Severity**: MEDIUM
- **Dimension**: 4 — this title's data through the shared parser
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:441` (`starting_health` is the only resource read from `RACE.DATA`); consumer `crates/plugin/src/esm/records/actor_value_derive.rs:140-158` (`derive_skyrim_actor_values`)
- **Status**: NEW (no issue in #2671–#3103; no prior `docs/audits/AUDIT_SKYRIM_*` or `AUDIT_CHARACTER_*` report raises it)
- **Description**: `NpcStatModel::RaceBaseOffsets` is documented as "race resource
  bases plus signed NPC offsets" (plural), and the NPC side holds all three:
  `magicka_offset` (ACBS `i16 @ 4`), `stamina_offset` (`i16 @ 6`),
  `health_offset` (`i16 @ 20`) — with `magicka_offset`'s own doc saying it is
  "parsed alongside Health … so the three TES5 resource offsets stay one
  verified wire-layout unit". The race side never got the other two: only
  `RACE.DATA` `f32 @ 36` is read. `derive_skyrim_actor_values` therefore returns
  a one-element vector, `vec![(health_key, health)]`.
- **Evidence**: All 99 `Skyrim.esm` `RACE` records carry a 164-byte `DATA`, and
  the three floats at 36/40/44 are the resource triple:
  ```
  ManakinRace       @36,40,44 = [ 50.0,  50.0,  50.0]
  UndeadDragonRace              [500.0, 150.0, 100.0]
  DraugrMagicRace               [ 50.0,   0.0,  80.0]
  FoxRace                       [ 12.0,   0.0, 200.0]
  distinct values across 99 races:  @36: 21   @40: 8   @44: 13
  ```
  `@40` and `@44` vary independently of `@36` and take only plausible resource
  magnitudes (0/4/5/15/50/100/150/200 and 0/10/15/20/25/50/75/80/…), which is
  what identifies them. `AVMagicka 0x3E9` and `AVStamina 0x3EA` are both
  authored in `Skyrim.esm` and resolve fine — nothing ever asks for them.
- **Impact**: 100 % of Skyrim actors (5 118 `NPC_` records plus the player)
  carry an `ActorValues` map containing exactly one entry. Every consumer that
  keys off Magicka/Stamina is silently inert on Skyrim: the CTDA evaluator's
  `GetActorValue` (`crates/scripting/src/condition.rs:419-431`, a direct
  `ActorValues` lookup), `setav`/`modav`, `pool_regen_tick_system`, and the
  Skyrim ruleset's own `CarryWeight = f(Stamina)` derivation
  (`crates/core/src/character/skyrim.rs:134`). Not a regression — the data has
  never had a reader — but it is the load-bearing prerequisite for wiring the
  Skyrim CHARAL ruleset, so it will block that work.
- **Related**: sibling finding #3 (CHARAL Skyrim roster — see the dedup section:
  same subsystem, also currently dormant), D4-02 below, `/audit-character`
  (owns `CharacterRulesProfile::SKYRIM`; this finding is the *parser* half only)
- **Suggested Fix**: Add `starting_magicka` / `starting_stamina:
  Option<f32>` to `RaceRecord` reading `DATA` `f32 @ 40` / `@ 44` with the same
  finite/positive guard `starting_health` uses, and extend
  `derive_skyrim_actor_values` to emit all three `(AVIF, base + offset)` pairs
  through `actor_value_form_id("Magicka")` / `("Stamina")`. Guard with a
  fixture RACE carrying a distinct triple so a future edit cannot re-collapse
  them onto Health.

---

### SKY-2026-08-20-D7-01: the Skyrim underwater-fog clamps erase 22 authored near planes and turn `HelgenWater`'s authored pair into a 1-unit fog span

- **Severity**: MEDIUM
- **Dimension**: 7 — WATAL canonical translation (Skyrim slice)
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:764-771` (`apply_skyrim_dnam_tail`); consumer `byroredux/src/systems/water.rs:469-476` + `underwater_extinction`
- **Status**: NEW
- **Changed File**: yes (`crates/plugin/src/esm/records/misc/water.rs`, #2 on `/tmp/audit/hot_files.txt`)
- **Description**: The decoder reads the under-water fog pair and then clamps:
  ```rust
  p.underwater_fog_near = near.max(0.0);
  p.underwater_fog_far  = far.max(p.underwater_fog_near + 1.0);
  ```
  Skyrim authors **negative** under-water near planes as a matter of course, and
  the second clamp is not a sentinel — it fabricates a value. The canonical
  layer already has a correct answer for an inconsistent pair: leave the
  `0.0` sentinel, which `systems/water.rs:469` detects
  (`if mat.underwater_fog_far > mat.underwater_fog_near`) and falls back to
  `mat.fog_near/fog_far`. The clamp defeats that fallback by manufacturing a
  pair that passes the test.
- **Evidence**: All 34 `Skyrim.esm` `WATR` records, `DNAM[144]` / `DNAM[148]`
  (layout confirmed by the distribution — 11 distinct near values, 10 distinct
  far, monotone per-record):
  ```
  near < 0 on 22/34 records, spanning 8 distinct authored values (-10000 … -40)
  all 22 collapse to near = 0.00

  DefaultMarshWater      near = -10000  far = 1000   -> 0.00 / 1000   (span 11000 -> 1000)
  RiftenWater            near = -10000  far = 1600   -> 0.00 / 1600
  DefaultMarshWaterTrans near =  -4000  far = 1200   -> 0.00 / 1200
  DefaultWater           near =   -500  far = 1600   -> 0.00 / 1600
  HelgenWater            near =  -1000  far =  -172  -> 0.00 /    1.00   <-- degenerate
  ```
  `HelgenWater` (`000C1D45`) is the only record with both planes negative. Its
  parsed output is `(0.0, 1.0)`, which passes the `far > near` fallback test,
  yields `span = 1.0` in `underwater_extinction`, and therefore saturates —
  `ramp` clamps to 1 and extinction reaches `1 - e^-2 ≈ 0.86` — at **one unit**
  of depth. The shader's own ramp gate agrees it is live:
  `water.frag:458 bool hasUnderwaterRamp = push.scroll_c.w > push.scroll_c.z + 0.001;`
- **Impact**: Helgen's water renders as an opaque wall the instant the camera
  submerges — the game's opening area. The 22 negative-near records lose their
  authored ramp offset (fog that should already be partly applied at the eye
  starts clear at the surface instead), flattening `DefaultMarshWater`'s
  authored 11 000-unit span to 1 000. Skyrim-scoped: this is the only decoder
  arm that clamps this pair.
- **Related**: sibling finding #1 (the `DNAM` one-field misalignment in the same
  function — see dedup below); #2790 / #2785 (`WaterMaterial::fog_near` travel)
- **Suggested Fix**: Reject rather than repair — when `far <= near`, leave both
  fields at the `0.0` sentinel so the documented `fog_near/fog_far` fallback
  engages, and keep the authored sign on `near` (the ramp arithmetic in
  `underwater_extinction` is `(depth - near) / span`, which is well-defined for
  negative `near` and is exactly the authored intent).

---

### SKY-2026-08-20-D4-02: the Skyrim leveling GMST overlay is unreachable, and one of its three GMST names exists in no Skyrim master

- **Severity**: LOW
- **Dimension**: 4 — per-title data semantics
- **Location**: `crates/core/src/character/leveling.rs:81-98` (`with_gmst`); reachability gate `crates/core/src/character/profile.rs:100-105` (`SKYRIM { ruleset: RulesetBuilder::None }`) and `:150-157` (`build_ruleset` early-returns `None`)
- **Status**: NEW. `1c9b8d7a` ("Source Skyrim leveling values from GMST", *Fix #2942*) landed inside this delta; #2942 is outside the `/tmp/audit/issues.json` window (#2671–#3103 — it is in range and not listed, i.e. closed before the snapshot).
- **Description**: Two independent reasons the overlay never runs on Skyrim.
  (1) **Unreachable.** The only production call site of `with_gmst` is
  `build_ruleset`, and `CharacterRulesProfile::SKYRIM` carries
  `ruleset: RulesetBuilder::None`, so `build_ruleset` returns `None` *before*
  the overlay line. `LevelingModel::SKYRIM` is constructed nowhere in
  production — the sole reference is the unit test at `leveling.rs:322`.
  (2) **One name is fabricated.** `gmst("fXPPerSkillRank")` matches no GMST in
  any installed Skyrim plugin, so even once wired it is a permanent no-op that
  silently retains the hard-coded `1.0`.
- **Evidence**: GMST EDID sweep over every installed master:
  ```
  Skyrim.esm 1584 GMST   fXPLevelUpBase  = 75.0   fXPLevelUpMult = 25.0   fXPPerSkillRank MISSING
  Update.esm   53        (no fXP*)
  Dawnguard 7 / HearthFires 10 / Dragonborn 5 / _ResourcePack.esl 0 — no fXP* in any
  every GMST beginning "fXP" in the whole install:  ['fXPLevelUpMult', 'fXPLevelUpBase']
  ```
  Note the two that *do* exist match `LevelingModel::SKYRIM`'s hard-coded
  `xp_base: 75.0, xp_mult: 25.0` exactly — the sourced constants are right; only
  the third name and the wiring are wrong.
- **Impact**: None at runtime today (the code is dead on Skyrim), which is
  precisely why it is LOW. It matters because #2942 is closed as fixed while
  the fix has no Skyrim reachability, and because a fabricated GMST name is the
  `feedback_no_guessing` failure mode — the next person to wire
  `RulesetBuilder::Skyrim` will inherit a silent no-op and a false "GMST-sourced"
  claim.
- **Related**: D4-01 above (the other half of the unwired Skyrim CHARAL path),
  sibling finding #3, #2945 (Skyrim/Oblivion leveling constants sourced only to
  `charal.md` prose — same sourcing concern), `/audit-character` (owns the
  profile table)
- **Suggested Fix**: Either drop the `fXPPerSkillRank` lookup or replace it with
  a name verified present in `Skyrim.esm`; and either add
  `RulesetBuilder::Skyrim` (routing to the existing
  `crates/core/src/character/skyrim.rs` ruleset) or move `LevelingModel::SKYRIM`
  behind `#[cfg(test)]` so the table does not advertise a path that cannot be
  reached.

---

## Dedup — prior report and sibling findings

### Prior `AUDIT_SKYRIM_2026-08-16.md` findings, re-verified at HEAD

| Prior ID | Issue | State at HEAD |
|---|---|---|
| D2-01 — slot 2 bound as glow map without `SLSF2_Glow_Map` | **#3068 CLOSED** | **FIXED, verified.** `TextureSlotContext` now carries `glow_map: bool`; the Skyrim/Starfield slot-2 arm returns `Emissive` only when it is set and `None` otherwise (`slot_role.rs:231-241`), pinned by `skyrim_slot_two_requires_the_glow_map_flag`. |
| D3-01 — `expand_leveled_form_id` ignores `LVLF 0x04` | **#3069 CLOSED** | **`0x04` half FIXED, verified** (`equip.rs:411`, test at `:784`). The `0x02` half is **not** fixed — filed above as SKY-2026-08-20-D3-01. |
| D1-01 — `BSXFlags` bit-5 carve-out comment | **#3070 OPEN** | **Premise removed.** #3036 deleted the whole-NIF rejection gate; the comment at `references/import.rs:68-79` now correctly says no era may use bit 5 as a file-level gate. **Recommend closing #3070** — there is nothing left to fix. |
| D2-02 — 1 564 slot-7 back-lighting maps dropped, untracked | **#3071 OPEN** | **Half addressed.** The role is still unrouted (`slot_role.rs`, Skyrim slot 7 non-MSN → `None`) — correctly, per the no-fabrication rule — but the "nothing tracks the loss" half is gone: `record_unrouted_texture_slot` / `unrouted_texture_slot_bindings` now count every unrouted authored binding per layout+slot. Not re-filed; leave #3071 open for the `BackLighting` role feature. |

### The four sibling findings handed down in the run brief

1. **WATR `normal_magnitude` reads the displacement simulator's starting size — CONFIRMED, and the blast radius is wider than stated.** Not re-derived; verified and extended.
   - `DNAM[92]` is bit-identically `0.05` on **34/34** `Skyrim.esm` records (the brief said 31/31 — that counts only the 228-byte layout; the three 232-byte SSE records carry the same constant).
   - Consumption confirmed live: `byroredux/src/env_translate.rs:815-822` clamps it to `[0.01, 8.0]` and multiplies **every** canonical noise amplitude by it — a uniform 20× flattening.
   - **Second consequence, not in the brief**: the same one-field slip also mis-sources the canonical `displacement` triple. `apply_skyrim_dnam_tail:805` reads `[72, 84, 88]` for a field documented at `water.rs:207` as "starting size, radial falloff, and dampener", but `DNAM[72]` is the **rain** simulator's starting size, not the displacement simulator's. The corpus reads exactly as two five-field blocks — rain `56/60/64/68/72`, displacement `76/80/84/88/92` — with `56–68` constant at `(0.1, 0.6, 0.985, 2.0)` across all 34 records and `72–88` varying (`72 ∈ {0, .01, .1, .7, 1}`, `80 ∈ {0, .6, .85}`, `88 ∈ {0, .98, .99, 1.1, 3.7, 7}`). Meanwhile `p.rain_start_size` — which has a field, a doc, a clamp at `env_translate.rs:799` and a shader consumer at `water.frag:785` — is **never assigned on the Skyrim path** and stays at its `0.0` sentinel on all 34 records.
   - **Warning for whoever fixes it**: `crates/plugin/src/esm/records/misc/water.rs:1788` currently `assert_eq!(w.params.normal_magnitude, 0.05)` — the regression test pins the defect and must be updated in the same change.
2. **`wind_direction` is a constant `90.0` stored raw into a radians field — STALE on Skyrim at HEAD.** `DNAM[4] = 90.0` on 34/34 and `decode_dnam_pre_fo4` does write it raw, but `apply_skyrim_dnam_tail` runs immediately afterwards (`water.rs:1353-1356`) and overwrites both fields from the noise-layer block: `p.wind_direction = p.noise_wind_directions[0]` (`DNAM[100]`, `.to_radians()` applied, 26 distinct values) and `p.wind_speed = p.noise_wind_speeds[0]` (`DNAM[112]`, 26 distinct). Every `Skyrim.esm` record has a ≥228-byte `DNAM`, so the tail always runs. **Not reported for Skyrim** — the finding stands only for the games that stop at `decode_dnam_pre_fo4`.
3. **CHARAL spells Illusion `"Illusion"` but Skyrim authors `AVMysticism 0x45B` — CONFIRMED, currently dormant.** `Skyrim.esm` has 149 `AVIF` records; `AVIllusion` is **absent** (only `AVIllusionMod 0x616`, `AVIllusionPowerMod 0x63D`, `AVIllusionSkillAdvance 0x628` exist), while `AVMysticism 0x45B` sits exactly in the 18-skill block between `AVDestruction 0x45A` and `AVRestoration 0x45C`. The other 17 entries of `SkillSet::SKYRIM` all resolve. **Blast radius on Skyrim today: none** — `CharacterRulesProfile::SKYRIM` uses `NpcStatModel::RaceBaseOffsets`, whose derivation (`derive_skyrim_actor_values`) never iterates the skill roster, and `RulesetBuilder::None` means no `CharacterRuleset` is ever built. It becomes live the moment either is wired, i.e. together with D4-01/D4-02 above. Owned by `/audit-character`; not re-filed.
4. **Mesh-water `blend_normals` gated on an undefined bit 16 — not re-derived (NIF path, `byroredux/src/material_translate.rs:142`).** One warning for the fix, from this audit's WATR-side measurement: **do not "fix" the `WATR.FNAM` bit `0x10` decode by analogy.** That one is empirically supported — `Update.esm` ships the paired EDIDs `DefaultWaterFlow` (`FNAM=0x08`) / `DefaultWaterFlowBlend` (`FNAM=0x18`) and `RiverWaterFlow` / `RiverWaterFlowBlend` with the same 0x08→0x18 delta, which names bit `0x10` "Blend" from the data itself. The *decode* is right; what is wrong is what the shader does with it (SKY-2026-08-20-D6-01).

---

## Shader-Type Coverage Matrix (Skyrim `BSLightingShaderType`)

Unchanged from 2026-08-16 — `parse_shader_type_data` and `parse_skyrim` have no
delta commits, and the arms `{1, 5, 6, 7, 11, 14, 16}` still exactly match
nif.xml's `cond="Shader Type == N"` set for `#NI_BS_LTE_FO4#` (no over-read, no
under-read; all other numeric types fall through to `ShaderTypeData::None`).
`6d7df853` (BSSPLuminanceParams) touched only `parse_fo76_plus` /
`read_wetness_block`'s `>= STARFIELD` gate and does not reach the Skyrim path.

Two rows carry corrections earned since the last matrix:

| # | Name | Change since 2026-08-16 |
|---:|---|---|
| 2 | Glow Shader | Slot 2 is now gated on `SLSF2_Glow_Map` (#3068) — the "dispatched by data presence" note in `triangle.frag:1240` is now backed by a flag, not by slot occupancy alone. |
| 16 | Eye Envmap | The glowing-eye emissive-mask defect (D2-01) is resolved by the same fix. |

`BSEffectShaderProperty`'s `env_map_min_lod` remains an unconsumed dead-end —
**Existing: #2582**, still OPEN, all vanilla blocks author `0`. Not re-filed.

---

## Cell-Load Regression Status

| Guard | Result |
|---|---|
| `parse_real_skyrim_esm` (real `Skyrim.esm`) | Source unchanged in the delta except for the `3ac08105` CELL-identity additions, which *added* assertions; not re-run (no cargo) |
| `.STRINGS` per-plugin wiring (#1553) | **LIVE** — `install_strings_guard` still inside the per-plugin loop, RAII-scoped (`load_order.rs:283-284`) |
| ESL / light-master FormID decode (#1554) | **LIVE** — `allocate_global_slot(header.light_master, …)` → `GlobalSlot::Light` (`load_order.rs:274`, `:299-325`), and `7fd85326` added explicit slot-exhaustion rejection before FormID aliasing |
| Deleted-REFR tombstones (#1660) | **LIVE** — `RECORD_FLAG_DELETED` still consulted in `cell/walkers.rs` |
| Repeatable `--master` FormID remap (#561 / #2583) | **LIVE**, and extended by `7fd85326` — VMAD / REFR / XPRI embedded references now remap into global load-order space too |
| Skyrim `WATR` corpus (this audit, Python) | 34 records in `Skyrim.esm`; 90 across all masters; `DNAM` 228 B ×31 / 232 B ×3 — matches the decoder's declared layouts |
| Skyrim `RACE` corpus | 99 records, `DATA` 164 B ×99, resource triple at `@36/40/44` — **two of three unread** (D4-01) |
| Skyrim `AVIF` corpus | 149 records; 17/18 `SkillSet::SKYRIM` entries resolve, `AVIllusion` absent (sibling #3) |
| Skyrim `GMST` corpus | 1 584 in `Skyrim.esm`; `fXPPerSkillRank` absent install-wide (D4-02) |
| Whiterun control-bench entity/FPS | **NOT RUN** (no engine launch permitted — see scope line) |
| Meshes0/1 parse sweep, BSA v105 extraction sweep | **NOT RUN** (no cargo, no Python LZ4) — `crates/bsa/src/archive/` has zero delta commits, so 2026-08-16's 100 % / 89 498-file result is carried |

---

## Disproved / rejected candidates

Recorded so they are not re-investigated:

1. **`wind_direction` constant `90.0` on Skyrim.** Disproved at HEAD — the `DNAM` tail overwrites it from `DNAM[100]` in radians. See dedup item 2.
2. **`WATR.FNAM` bit `0x10` is an invented flag.** Disproved — `DefaultWaterFlow`/`DefaultWaterFlowBlend` and `RiverWaterFlow`/`RiverWaterFlowBlend` in `Update.esm` differ by exactly that bit and by the word "Blend" in the EDID.
3. **`min_vertex_bytes` (#2598) diverges from `decode_bs_vertex_stream`'s read sequence.** Disproved by line-by-line comparison: position 16 B full / 8 B half, UV 4, normal 4 (+4 tangent), colour 4, skin 12, eye 4 — and `VF_UVS_2` / `VF_LAND_DATA` / `VF_INSTANCE` are excluded from *both*, exactly as `BSVertexData`/`BSVertexDataSSE` declare no field for them (the #336 deferral, re-confirmed).
4. **Skyrim SE positions decoded as half-float.** Disproved — `full_precision = bsver < FALLOUT4 || VF_FULL_PRECISION`, so SSE (bsver 100) is always full precision, matching `BSVertexDataSSE`'s `Vector3` + `Bitangent X` float.
5. **The #2828 degenerate-tangent Gram–Schmidt can emit NaN.** Disproved — `normalize_inplace` zeroes below `1e-12` rather than dividing; the only degenerate input (`n ∝ (a,a,a)`) yields a zero tangent, identical to nifly's own behaviour.
6. **`BSLODTriShape` folded back into `BsTriShape` (#838 regression).** Disproved — three distinct arms remain (`blocks/mod.rs:473`, `:478`, `:489`) plus `:505` for `BSDynamicTriShape`.
7. **Sibling-archive auto-load re-narrowed (`821a425b`).** Disproved — `crates/bsa/src/archive/` and `asset_provider/archive.rs`'s sibling expansion are untouched by the delta; `archive_siblings.rs` gained tests, not restrictions.
8. **`.btr` / `.bto` LOD gates dropped Skyrim.** Disproved — `terrain_lod_btr.rs` and `object_lod.rs` both still name `GameKind::Skyrim` first, and the band ladder was *widened* (4/8/16 Skyrim) by the delta's LOD work.
9. **`SLSF2_Glow_Map` still unread (prior D2-01).** Disproved — fixed by #3068.
10. **Skyrim vanilla content reaching the Disney/Burley lobe.** Unchanged from 2026-08-16 — `is_pbr`'s only writers remain BGSM/BGEM/`.mat`-driven in `asset_provider/material.rs`; Skyrim ships none.
11. **The Skyrim interior-ghosting investigation.** Deliberately untouched per the run brief.

---

## Evidence artifacts

No `_tmp_*` cargo examples were added this pass (cargo was off-limits). The
corpus numbers above came from a self-contained Python ESM walker at
`<scratchpad>/esm.py` (GRUP recursion, zlib record decompression, `XXXX`
big-size subrecords) driven by throwaway one-liners; it reads the shipped game
files only and depends on nothing in this repo.

---

TALLY: CRITICAL=0 HIGH=2 MEDIUM=2 LOW=1
