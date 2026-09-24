# P2 Combat Tail — Animation & Sound Fixture

**Status:** scoped 2026-09-21 (decode verdicts verified against the real
archives; wiring not started)

Companion to [`p2-combat-fixture.md`](p2-combat-fixture.md) (actor/weapon,
frozen 2026-08-10) and the P2 closure gate in
[`playable-vertical-slice.md`](playable-vertical-slice.md): *"Play one
attack/hit/death animation family and spatial sound family."* This document
pins which content that gate uses, and records the decode evidence that the
shipped import machinery handles it — so the wiring work starts from known
assets instead of a content hunt.

Everything below was verified on 2026-09-21 against the local SE install
with `crates/bsa/examples/bsa_grep.rs` / `bsa_extract_one.rs` and the new
`crates/hkx/examples/probe_clip.rs`.

## Verdict in one line

The Draugr combat family is **standalone spline-compressed HKX clips plus
plain PCM WAV one-shots** — the exact shape the shipped `hkx` decoders
(`decode_skeleton` / `decode_spline_animation`) and the M44 audio path
(symphonia WAV) already handle. No behavior-graph parsing is needed for the
P2 closure gate; the behavior layer remains where it is for everything else.

## Pinned animation set (Skyrim - Animations.bsa)

All clips are standalone `hkaSplineCompressedAnimation` tagged files
(hk_2014 container, magic `0x57E0E057`) carrying **84 tracks** — exactly the
bone count of the Draugr rig, so `convert_hkx_clip`'s track→bone pairing
applies with identity indices (the cart-catalog machinery, unchanged).

| Role | Path (in BSA) | Decode |
|---|---|---|
| Rig (skeleton) | `meshes\actors\draugr\character assets\skeletonf.hkx` | 84 bones, root `NPC Root [Root]` |
| Attack (2HM forward) | `meshes\actors\draugr\animations\2hmattackforwardb.hkx` | 2.50 s, 76 frames, 7 annotations |
| Attack (2GS power chop) | `meshes\actors\draugr\animations\2gsattackforwardpowerchop2.hkx` | 2.17 s, 66 frames, 8 annotations |
| Attack (1HM alt, unfrozen) | `meshes\actors\draugr\animations\1hmattackx2.hkx` | 2.33 s, 71 frames, 8 annotations |
| Hit reaction | `meshes\actors\draugr\animations\mtstaggermedium.hkx` | 2.03 s, 62 frames, 2 annotations |
| Death | `meshes\actors\draugr\animations\special_deathbackward.hkx` | 1.00 s, 31 frames, 3 annotations |

Freeze rule: the gate uses the **2HM/2GS family** — the fixture's concrete
weapon leaves (`0001CB64` Draugr Battleaxe, `000236A5` Draugr Greatsword)
are both two-handed (`p2-combat-fixture.md`), so `1hmattackx2` is listed
only as a decode-verified alternate, not a gate asset. The installed
primary attack is `2hmattackforwardb`: the generic 2HM take matches both
weapon leaves, where the greatsword-specific power chop would fit only one.
`2gsattackforwardpowerchop2` stays the decode-verified power alternate.
(#4564 — the wiring commit `ec3a18d2f` installed the 2HM take and its
real-data gate verified it; the earlier "power chop is primary" freeze
wording is superseded by what ships.)

Clip **annotations** decode with the tracks (7–8 per attack clip). Havok
clip annotations are the natural hit-timing marker for "play the impact
sound / apply damage at contact" — an alternative to hand-timed offsets.
Whether the surviving annotation payloads carry the game's hand events or
something else is a wiring-time question (print them once with
`probe_clip`'s successor); if they are unusable, fall back to a normalized
time constant pinned in this doc's successor edit.

## Pinned sound set (Skyrim - Sounds.bsa)

All plain RIFF/WAVE PCM — symphonia/M44 decode as-is, no xWM/`.fuz`
handling required for the gate:

| Role | Path (in BSA) | Notes |
|---|---|---|
| Swing | `sound\fx\wpn\swing\blade2hand\fx_swing_blade2hand_03.wav` | 2-handed blade family, matches the fixture weapons |
| Impact (flesh-draugr) | `sound\fx\wpn\impact\blade\2hand\fleshdraugr\wpn_impact_blade2hand_fleshdraugr_01..03.wav` | Draugr-specific flesh impact set |
| Death voice | `sound\fx\npc\draugr\death\npc_draugr_death_02.wav` | mono 44.1 kHz PCM, 195 KB |
| Hit reaction voice | `sound\fx\npc\draugr\injured\npc_draugr_injured_02.wav` | same family as death, usable for the stagger |

Deliberately out of the gate: `.fuz` voice containers (draugr dialogue
grunts) and xWM — M44 has no decoder for either, and every sound the gate
needs exists as plain WAV. SNDR descriptor resolution (the game's
randomized per-attack sound selection) is likewise out: the gate pins one
sound per event, wired through `AudioWorld::play_oneshot` directly.

## Wiring shape (dependency order)

1. **Clip attach**: load the rig + frozen clips through the cart-catalog
   path's producer (`convert_hkx_clip` + `idle_animation_candidates`-style
   archive lookup, generalized to the Draugr paths above). The Draugr
   fixture's `SkeletonID`/rig selection must resolve to
   `skeletonf.hkx` — verify at wiring time that the placed `000383F7`
   uses the female-rig file name convention or whether a `_0ma`/male
   variant must be pinned instead (the rig file name is the one content
   question left open; the fixture Draugr is male — `HeadM06` — so pin
   the male rig path if one exists beside `skeletonf.hkx`).
2. **Playback**: the M42 walk-takeover machinery
   (`byroredux/src/systems/walk_anim.rs` take/abandon + `AnimationPlayer`
   swap) is the playback precedent; the attack/death takes follow the same
   shape — take on the canonical `AttackEvent` edge, death take on the
   existing `Dead` transition (which already drives the 18-body ragdoll).
   Seated/Dead/cinematic guards in `walk_anim` are the ownership rules to
   mirror, not bypass.
3. **Sounds**: swing one-shot fires at take start (or the attack clip's
   annotation marker, if usable); impact at the hit-marker/`HitEvent`
   application; death voice on the `Dead` transition. All through
   `play_oneshot` at the actor's position (spatial sub-tracks, existing
   reverb send applies in the interior cell).
4. **Gate**: extend [`p2-melee-core.sh`](../smoke-tests/p2-melee-core.sh)
   (or add `p2-combat-feel.sh`) to assert the death take actually started —
   `AnimationPlayer.clip_handle`/`playing` on the fixture actor after the
   killing blow — plus non-zero `play_oneshot` queue observations, so the
   gate proves playback, not just import.

## Out of scope (unchanged)

Behavior-graph-driven combat (attack decisions, root-motion authority,
kill-moves), `.fuz`/xWM decode, SNDR randomization, and the 1HM/ bow /
magic families. The gate is one family, played end to end — breadth comes
later, and only if the slice route demands it.
