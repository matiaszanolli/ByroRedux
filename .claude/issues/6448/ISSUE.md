# ESM-2026-10-27-01: BPTD `BPND` decoded with Flags<->Part Type and To-Hit<->Health Percent swapped

Labels: medium, esm-plugin, bug, game:fnv, game:fo3
Source: docs/audits/AUDIT_ESM_2026-10-27.md

**Report**: `docs/audits/AUDIT_ESM_2026-10-27.md` · **Finding**: ESM-2026-10-27-01 · **Severity**: MEDIUM · **Dimension**: Sub-Record Byte Accounting
**Location**: `crates/plugin/src/esm/records/misc/equipment.rs:274-293` (`parse_bptd`); field docs `:174-196`. Consumer: `byroredux/src/systems/vats.rs` (`vats_scan`).
**Games**: FO3 / FNV (`BPTD` / `BPND`)

## Description
xEdit gives one `BPND` layout for both games (`wbDefinitionsFNV.pas:5963-5982`, `wbDefinitionsFO3.pas:5368-5386`):

| Offset | Field (xEdit) |
|---|---|
| @0 | Damage Mult `f32` |
| @4 | **Flags** `u8` (Severable, IK Data, IK Biped, Explodable, IK Is Head, IK Headtracking, To-Hit Absolute) |
| @5 | **Part Type** `s8` (`wbBodyLocationEnum`) |
| @6 | **Health Percent** `u8` |
| @7 | Actor Value `s8` |
| @8 | **To Hit Chance** `u8` |
| @9 | Explosion Chance `u8` |
| @10 | Debris Count `u16` |
| @12 / @16 | Debris / Explosion FormIDs |

`parse_bptd` reads `part_type = @4`, `flags = @5`, `to_hit_chance = @6` and `health_percent = @8`, and reads debris count as a `u8`. The struct doc says the layout is "corpus-established" by value-shape clustering (#6350 spike) and that "bit0 tracks severable exactly (head = 1, torso = 0)". That pattern is the Part Type enum (Torso 0, Head 1), not a flag; Right Leg is Part Type 10, so bit 0 is clear on it.

## Evidence
Raw `BPND` bytes, `DefaultBodyPartData` (`0x1D`); identical in FalloutNV.esm and Fallout3.esm:

| Part | b4 (Flags) | b5 (Part Type) | b6 | b8 |
|---|---|---|---|---|
| Head | 59 = Sev+IK+Expl+IsHead+Headtrack | 1 | 20 | 30 |
| Torso | 8 = Explodable | 0 | 60 | 60 |
| Left Arm | 9 | 3 | 25 | 45 |
| Left Leg | 15 | 7 | 25 | 50 |
| Right Leg | 15 | 10 | 25 | 50 |
| Right Arm | 9 | 5 | 25 | 45 |

- b4 decodes cleanly as the flag set: only the head carries IsHead and Headtracking, and the torso is not severable.
- b5 is exactly the body-location enum.
- b8 is the same on `PlayerBodyPartData` (`0x1C`): 30/60/45/50, which fits a per-part to-hit chance.
- b6 differs between the two records: player 75/255/100/150, NPCs 20/60/25/25, which fits a share of the actor's health.

Current code (verified at HEAD `fa6be4a6b`): `part.part_type = u8_at(4); part.flags = u8_at(5); part.to_hit_chance = u8_at(6); ... part.health_percent = u8_at(8); part.debris_count = u8_at(10);`.

## Impact
- `byroredux/src/systems/vats.rs` (`vats_scan`, `part_to_hit: part.to_hit_chance as f32`) feeds the health share into `VatsModel::to_hit_chance`. Every NPC limb is priced at 25 instead of 45-50, the head at 20 instead of 30.
- `flags` and `part_type` are wrong for every part, which matters for the Phase-4 dismemberment consumer the doc names. The FNV VATS gate has been live since `59d69f763`.

## Related
ESM-2026-10-27-02 (same formula's weapon term); #6350 (VATS tracker; its spike table records the swapped reading); #6403 (to-hit ground-truth pin is `#[ignore]`d, so nothing caught this). The real-data pins encode the wrong reading: `crates/plugin/tests/parse_real_esm.rs:6311-6346`.

## Suggested Fix
- Read the struct per xEdit: flags @4, part_type @5, health_percent @6, to_hit_chance @8, debris_count `u16` @10.
- Re-pin the census test against the values above.
- Drop the "severable = bit0" claim, or re-derive it from the real flags byte.

## Completeness Checks
- [ ] **SIBLING**: Other `BPND`/`DNAM`-style offset decoders touched by #6350 (see ESM-2026-10-27-02, -03) re-checked against xEdit
- [ ] **TESTS**: Real-data pins at `parse_real_esm.rs:6311-6346` re-pinned to the xEdit reading; a synthetic `BPND` byte-layout test added

