# CHAR-2026-09-21-D1-01: Vanilla --hud bars key on fallout.rs's synthetic test FormIDs (0x2C9/0x2D0) — FO3/FNV/FO4 bars never move

**Severity**: MEDIUM
**Dimension**: Ruleset Seam
**Game**: FO3 / FNV (MenuXml `--hud`), FO4 (Scaleform `--hud`), Skyrim (actor-identity half only)

## Description

AVIF identities are AUTHORED and must be resolved per load (CHARAL doctrine; `EsmIndex::actor_value_form_id`). The FO3/FNV MenuXml and FO4 Scaleform HUD profiles instead hardcode ids copied from a unit-test fixture. Separately, `fraction()` (`byroredux/src/hud.rs`) has two problems:
- It scans `world.query::<ActorValues>()` and returns the first entity in `SparseSetStorage`'s dense order (insertion order, perturbed by swap-remove) that carries the key. It never reads `PlayerEntity`.
- That "any stamped actor" fallback predates `eb3784309` and is obsolete now that the player carries `ActorValues`.

The native vitals path one module over (`inventory.rs`'s `build_player_vitals` + `vitals_snapshot`) already does both things correctly: it resolves keys through `actor_value_form_id` and reads the player.

## Evidence

Verified at HEAD `ee6d3fb39`. `hud.rs`'s `FALLOUT_BARS`:
```rust
static FALLOUT_BARS: &[HudBar] = &[
    HudBar { label: "hp", av: Some(0x2C9) },
    HudBar { label: "ap", av: Some(0x2D0) },
];
```
`byroredux/src/scaleform_hud.rs` mirrors this with `ScaleformBar { label: "health", av: 0x2C9 }` / `{ label: "ap", av: 0x2D0 }`, commented "FO4 keys per `crates/core/src/character/fallout.rs`". Those two ids are the FormIDs of the test-only `fn full()` resolver in `crates/core/src/character/fallout.rs` (`Health => 0x2C9`, `ActionPoints => 0x2D0`, beside `Strength => 0x05`), not any real AVIF.

Real-master AVIF tables (read-only probe, per the audit report): FNV/FO3 `AVHealth 0x450`, `AVActionPoints 0x44C`, and neither `0x2C9` nor `0x2D0` is an AVIF; FO4 `Health 0x2D4`, `ActionPoints 0x2D5`, **`0x2C9 = Experience`**, `0x2D0` = none; Skyrim `0x3E8/0x3E9/0x3EA` = AVHealth/AVMagicka/AVStamina (correct).

`fraction()` falls back to `1.0` when no entity carries the key, and otherwise takes the first hit from an unordered `world.query::<ActorValues>()` scan — never `PlayerEntity`.

## Impact

- On real FO3/FNV/FO4 content no entity carries these keys, so the HP/AP bars always draw full, regardless of damage. On FO4 the "health" bar is keyed on the `Experience` AV.
- On Skyrim the keys are right, but the bar can track an NPC's health rather than the player's.
- The smoke gates pin bar fractions via `hud.values`, so none of them catches it.

## Related

CHAR-2026-09-21-D1-02, CHAR-2026-09-21-D4-01 (the player's `ActorValues` this fix should read); PERF-D1-2026-09-21-03 and #3429 (HUD cost, not keys). `AUDIT_UI_2026-09-21.md` (Dim/§4, "existing findings re-confirmed") independently confirmed this exact bug and explicitly assigns the filing to `/audit-character`, adding three UI-side extensions: the same wrong keys are stated as fact in `docs/engine/ui.md:855-858,957-958,865-867`, the ROADMAP M48.5/M48.7 rows and `hud.rs:43-45`/`scaleform_hud.rs:83-84`; `hud.status` prints a constant `1.00 (auto)` for every unpinned bar so no MenuXml observable reports the live-derived fraction; and both drivers call `fraction()` before the change-signature check every frame, so the full-storage `ActorValues` scan runs per bar per frame even on a static HUD (reading `PlayerEntity` removes that cost too).

## Suggested Fix

Resolve each bar's key once per load via `index.actor_value_form_id(editor_id)`, reusing `build_player_vitals`'s resolution. Read `PlayerEntity`'s `ActorValues` rather than scanning all actors. Delete the literal ids and the doc claim at `hud.rs:43-45`/`scaleform_hud.rs:83-84`, and the stale claims in `docs/engine/ui.md` and the ROADMAP M48.5/M48.7 rows. Fix `hud.status` to print the live fraction. The fix spans `/audit-character` (this issue) and `/audit-ui` (`hud.rs`/`scaleform_hud.rs` owners, doc sites, and the per-frame scan cost).

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D1-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix
