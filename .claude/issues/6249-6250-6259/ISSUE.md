# Batch: 6249 6250 6259

---
## # REN-M-01: The shadow filter adds a correction wherever the reservoir slot was claimed, including pixels whose visible surface never claims (unlit screens, water, ground-cover blades, glass, decals) [OPEN]

**Source**: docs/audits/AUDIT_RENDERER_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Denoiser/Composite (shadow filter) · **Location**: `crates/renderer/shaders/shadow_filter.comp (guideAt, storeResult)`

- **Location**:
  - `crates/renderer/shaders/shadow_filter.comp`: `guideAt` takes validity from `histLenAndDepth.x != 0`; `storeResult` writes the correction.
  - `crates/renderer/shaders/composite.frag`: `direct = max(direct + texture(shadowCorrectionTex, fragUV).rgb, 0)` under `caustic_flags.y`.
  - `crates/renderer/shaders/triangle.frag`: the claim block under `if (stableTemporalSurface)`; the `MATERIAL_KIND_NO_LIGHTING` early return.
- **Status**: NEW. The 2026-10-18 report recorded the blade/water instance as an observation only (its Needs-RenderDoc item 3); this finding generalises it and adds the opaque cases and the torn-slot case.
- **Description**:
  - A slot is valid if any `stableTemporalSurface` fragment (`!isAlphaBlend && !isGlass`) that reached the ReSTIR finalize won `atomicMin` this frame.
  - The filter then writes `U · strength · (filtered − raw)` for that pixel, and composite adds it to the visible pixel's direct light.
  - The visible surface is often not the claimant:
    - **Opaque `MATERIAL_KIND_NO_LIGHTING` (102)** surfaces (terminal screens, signs) return at the `outColor = vec4(emit, texColor.a)` early exit and never claim. They run in the late-test pipeline (kind > `MATERIAL_KIND_MAX_LIGHTING_SHADER`), and draw order is front-to-back only coarsely within a group (`draw_sort_key`). A lit surface behind them that was drawn earlier, or an alpha-tested cutout behind them in the late-test pipeline, owns the slot.
    - **Water and ground-cover blades** never claim, so the terrain or lake floor beneath owns the slot (the 2026-10-18 observation).
    - **Glass and blended decals** never claim. The opaque surface behind owns the slot, and its full correction is added to the composited colour although that surface reaches the pixel only through the blend's (1 − α).
  - **Torn slot.** The claim and the two stores are not atomic as a unit. The shader comment accepts that a farther fragment's late store can land after a nearer winner's, which costs only a history reset next frame. But `reservoirsCurr[pixelIdx] = rc` and `directUnshadowed[pixelIdx] = …` are separate stores, so the filter can pair one fragment's ratio and guides with another fragment's U in the same frame, with no gate.
- **Evidence**:
  - `composite.frag`'s comment says the term "replace[s] the shaded U * rawRatio by U * filteredRatio", which holds only if the slot owner is the visible surface.
  - Nothing in `shadow_filter.comp` compares the slot's surface against the visible pixel. The G-buffer mesh ID (opaque = stable surface ID, bit 31 = blended) is available but not bound to the pass.
  - Guards are thin (`bytes_per_pixel_matches_frame_images`, `a_failed_layout_submit_leaks_instead_of_destroying`), and no test covers ownership.
- **Impact**:
  - Light or dark smudges inside about 6 px of the owner's penumbra edges appear on surfaces the owner does not shade.
  - The worst cases are emissive screens in front of lit walls, and water over a shadowed shore floor.
  - Magnitude scales with the hidden owner's U and with `strength`, which is 1 while its history is short, i.e. whenever the camera moves.
  - Not measured (no engine launch).
- **Related**: #5765 (claim), #6035 (billing), REN-EXT 2026-10-18 Needs-RenderDoc item 3.
- **Suggested Fix**: bind the mesh-ID attachment to the filter. Write a zero correction when the visible mesh ID has bit 31 set or does not equal the slot's surface tag (`lightAndSurface` & `RESERVOIR_SURFACE_MASK`, the same identity the claim writes), and use the same test as the centre-pixel validity check. That also rejects torn slots whose reservoir came from a different surface. No barrier or layout change beyond one more sampled input (needs a `BYRO_VALIDATION=1` run when added).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix


---
## # REN-M-02: The ReSTIR mover taint misses pixels a mover un-occludes on its first moving frame or after a teleport, and keys on the first ray hit rather than the occluder; those pixels keep the 1024-frame tail [OPEN]

**Source**: docs/audits/AUDIT_RENDERER_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Ray Queries / Denoiser (ReSTIR direct-light EMA) · **Location**: `crates/renderer/shaders/triangle.frag (moverHit/prevTainted block)`

- **Location**:
  - `crates/renderer/shaders/triangle.frag`: the `moverHit` / `prevTainted` / `deep` block before `Lo += restirUnshadowedSum * accum`, and the `if (sr == 0)` capture of `selectedRayHitInstance`.
  - `crates/renderer/shaders/include/shadow_transport.glsl`: `traceShadowTransmittanceDetailed`, where `committedInstance` is the first hit.
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: `instance_moved`, `restir_rig_static`. From `3934f46d7`.
- **Status**: NEW
- **Description**:
  - Since `3934f46d7`, movers no longer clear `restir_rig_static` scene-wide. A parked camera with a mover anywhere sits in mode 3 (rig static), or mode 2 when nothing moved, and both run the deep tail.
  - A pixel leaves the deep tail only if this frame's selected shadow ray hits an instance flagged `INSTANCE_FLAG_MOVED_THIS_FRAME`, or if it was already tainted. Three holes:
    1. **First moving frame.** `instance_moved` is `previous_source != m`. On the first frame an occluder moves, the pixels it uncovers already miss it and were never tainted, because it was static the frame before. They blend toward lit at α = max(1/histLen, 0.002), about a 346-frame half-life, so a sliver of the old shadow edge lingers. The same applies to every pixel uncovered by a teleport (one frame flagged, then static). An instance that *vanishes* is handled by the set-change gate; one that *jumps* is not.
    2. **First hit, not the occluder.** `committedInstance` is set on the first committed hit, including alpha-tested layers that `rayHitHasCoverage` then skips (foliage, fences) and effect cards. An NPC standing behind a fence relative to the light reports the fence, which is static, so its shadow is never tainted.
    3. **Sample 0 only.** Only `sr == 0` records the hit. With `directShadowSamples > 1`, penumbra pixels whose other cone taps hit the mover stay untainted.
- **Evidence**: the code paths above. The skill states that no test pins the taint, and none does.
- **Impact**:
  - With the camera parked, an NPC that starts walking, a door that starts swinging, or a scripted reposition leaves ghost shadow strips that take seconds to clear. That is the ghosting #4942 removed with the scene-wide gate.
  - Hole 2 makes it systematic for actors seen through foliage or fences.
  - Not measured; the `restir_history` view (orange = tainted) is the on-device check.
- **Related**: #5369, #4942, `3f8194117` (SVGF made the same scene-wide → per-pixel move), REN-M-01.
- **Suggested Fix**:
  - Taint on the *previous* frame's occluder as well. For example, store the hit's stable surface ID or instance in the reservoir, and taint when last frame's recorded occluder carries the moved flag this frame. Or keep `INSTANCE_FLAG_MOVED_THIS_FRAME` set for `RESTIR_MOVER_SETTLE_FRAMES` after motion stops and for one frame before (the previous-frame transform is already uploaded).
  - Record the instance that actually returned zero visibility (the covered or opaque hit), not the first committed candidate. Consider OR-ing the flag across all `sr` samples.
  - Pin with a source-shape test, since the GPU behaviour needs RenderDoc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix


---
## # SF-2026-10-21-D4-01: Essential / protected Starfield actors that go down never get back up — the profile has no bleedout rule, while the data carries one [OPEN]

**Source**: docs/audits/AUDIT_STARFIELD_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 4 (ESM + bring-up: NPC data through the shared mechanisms; the rule itself belongs to `/audit-character` + `/audit-gameplay`) · **Location**: `crates/core/src/character/profile.rs:461-463` (`STARFIELD.essential_death_time_secs: None`)

## Location
- `crates/core/src/character/profile.rs:461-463` (`STARFIELD.essential_death_time_secs: None`)
- `crates/scripting/src/bleedout.rs:129-160` (`engine_defaults`, `install_bleedout_rules`)
- `crates/scripting/src/bleedout.rs:186-215` (`spare_from_death`)
- `crates/scripting/src/bleedout.rs:218-221` (`bleedout_system` early return)
- `crates/plugin/src/esm/records/actor/race.rs` (Starfield RACE `DAT2` bleedout block not decoded)

## Status at filing
NEW. This is the character audit's unfiled lead, now confirmed. Related open/closed issues:
- #5858 (closed) changed the infinite timer to `None`.
- #6120 (closed) made the flags live.
- CHAR-2026-10-21-L-02 (this suite) covers the #5921 profile rows' missing capture lines.

## Description
- Since #6120 (78facba89), Starfield `NPC_` `ACBS` flags decode. `parse_npc` sets `essential` from bit 1 and `protected` from bit 11 (`npc.rs:584-586`), and `ActorProtection::from_record` stamps them at spawn.
- The Starfield `CharacterRulesProfile` row has `essential_death_time_secs: None`, so `engine_defaults` returns `None`, and `install_bleedout_rules` *removes* `BleedoutRules`.
- When an essential (or non-player-killed protected) actor's health reaches zero, `spare_from_death` inserts `Bleedout { remaining_secs: None }`.
- `bleedout_system` returns immediately without `BleedoutRules`, so the actor stays in bleedout (counted unconscious by every #5017 consumer) until a script `Resurrect` (`systems/revive.rs`).
- The state also survives cell eviction (`reference_state.rs`) and saves (`save_io/registry.rs:382`).

The profile comment's rationale ("Starfield's executable carries no `fEssentialDeathTime`" → "the game has no bleedout recovery", `profile.rs:245-249` / `bleedout.rs:133-135`) is contradicted by the game's own data.

## Evidence
- **`Starfield.exe` strings**:
  - `fEssentialHealthPercentReGain`, `fEssentialHostileDeathTimeMult`, `fEssentialNPCMinimumHealth`, `fSpaceshipEssentialDeathTime`;
  - the Papyrus surface `SetNoBleedoutRecovery` / `GetNoBleedoutRecovery` / `NoBleedoutRecovery`, and the event `OnRecoverFromBleedout`.
  - There is no `fEssentialDeathTime` string; `Fallout4.exe` has one.
- **xEdit `wbDefinitionsSF1.pas`**:
  - RACE `DAT2` +160..+184 is `Bleedout { Health %, Chance, Recover Chance, Time Min, Time Max, Health Drain Rate }` (lines 16582-16589).
  - `NPC_` `ONA2` flag 5 carries per-NPC `Bleedout Data` overrides (15358-15365).
  - `ACBS` bit 16 is "Doesn't bleed" and bit 18 is "Bleedout Override" (15016-15050).
- **Probe (`/tmp/audit/starfield/probe/npc_race.py`, read-only over `Starfield.esm`)**:
  - 7,131 `NPC_` `ACBS`: 626 Essential, 20 Protected, 87 "Doesn't bleed".
  - 31 RACE `DAT2` ≥ 184 B. `HumanRace` = (0.25, 0.25, 0.25, **8.0, 12.0**, 0.01). 15 races (`ModelARace`, `MiniBotARace` among them) author all zeros.

## Impact
any Starfield essential actor (companions, quest givers) brought to zero health by combat, a hazard or a script `DamageValue` stays down permanently: no AI, no dialogue, no packages. The state is persisted, so the save is poisoned too. Before #6120 the flags were 0, so this path was unreachable; the flag fix exposed it.

## Related
#6120, #5858, #5921 (profile rows), CHAR-2026-10-21-L-02.

## Suggested Fix
- Do not invent a value (No Guessing policy).
- Decode the SF RACE `DAT2` bleedout block and the `ONA2` override so the data is captured.
- Source the recovery semantics (how `Time Min/Max`, `Recover Chance`, `Health Drain Rate` and `fEssentialHostileDeathTimeMult` combine) from a CK / Papyrus capture before wiring a Starfield `BleedoutRules`.
- Until then, correct the profile and module docs, which claim Starfield has no recovery, and log once when an actor enters a recovery-less bleedout on a game whose rule is unsourced (as opposed to genuinely absent).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix


