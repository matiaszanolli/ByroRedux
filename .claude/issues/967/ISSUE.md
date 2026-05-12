# OBL-D3-NEW-03: parse_race is FNV-shape only; Oblivion RACE DATA + sub-records dropped

**Labels**: bug, medium, legacy-compat

**Audit**: `docs/audits/AUDIT_OBLIVION_2026-05-11_DIM3.md`
**Severity**: MEDIUM
**Domain**: ESM / TES4 actor records

## Premise

`parse_race` reads exactly 7 × `(u32, i8)` skill-bonus pairs starting at DATA offset 0, plus walks `MODL` entries.

[crates/plugin/src/esm/records/actor.rs:545-561](../../crates/plugin/src/esm/records/actor.rs#L545-L561)

The DATA-reading loop runs 7 iterations of a 5-byte stride and stops; nothing else from the rich Oblivion DATA struct is read.

## Gap

Oblivion RACE DATA carries (after the shared 7 skill bonuses):

- 14 × u8 base attributes (Strength..Luck per gender)
- 2 × f32 base height (male, female)
- 2 × f32 base weight (male, female)
- u32 flags
- 2 × u32 voice forms (male, female)
- 2 × u32 default hair forms
- u8 default hair color
- u8 face-morph count
- u16 unknown / padding

Plus separate sub-records: `XNAM` (race-vs-race reactions), `VNAM` (voices), `DNAM` (default hair), `CNAM` (default eyes), `PNAM` (face-morph values), `UNAM` (body-morph values), `FNAM` (face params), `INDX` per body part.

## Impact

Default scale, default hair/eyes, voice routing silently dropped. Doesn't block rendering. M41.0 Phase 3b FaceGen recipe will need this for NPC head spawn at the right scale and with the right default hair/eye color.

## Suggested Fix

Thread `game: GameKind` into `parse_race` (currently it doesn't take `game`). Branch on `GameKind::Oblivion` to extend `RaceRecord` with:

```rust
pub base_attributes: [u8; 14],   // male 7 + female 7
pub base_height: (f32, f32),     // (male, female)
pub base_weight: (f32, f32),
pub flags: u32,
pub voice_forms: (u32, u32),
pub default_hair: (u32, u32),
pub default_hair_color: u8,
pub default_eyes: Option<u32>,   // from CNAM
pub face_reactions: Vec<(u32, i32)>, // XNAM pairs
```

FNV path stays untouched (gate the new reads on `game == GameKind::Oblivion`).

## Completeness Checks

- [ ] **SIBLING**: Verify FNV RACE parsing still pulls the 7 skill-bonus pairs unchanged.
- [ ] **SIBLING**: Confirm Skyrim RACE doesn't accidentally route into the Oblivion arm (different DATA layout again).
- [ ] **TESTS**: Regression test parses `Oblivion.esm` and asserts `races["nord"]` has non-zero base height + non-default voice forms.
