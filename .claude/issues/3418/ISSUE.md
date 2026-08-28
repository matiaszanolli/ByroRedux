# #3418 — FNV-2026-08-27-D4-02: the FNV race head mesh is `body_models.first()` — always the male head

**Labels**: high, bug, esm-plugin, character, game:fnv, legacy-compat

**Filed**: 2026-08-27 · from `docs/audits/AUDIT_FNV_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-27.md` — finding `FNV-2026-08-27-D4-02` (HEAD `969d81c8`)

- **Severity**: HIGH
- **Dimension**: 4 / 6 — ESM record semantics → NPC spawn
- **Location**: `byroredux/src/npc_spawn/resumable.rs:380-382`

## Description

The kf-era (Oblivion / FO3 / FNV) NPC spawner picks the head NIF as the first entry of `RaceRecord::body_models`, a flat append-ordered list of *every* `MODL` in the RACE record. In the shipped FNV layout the first `MODL` is always the male head, because the record opens `NAM0` (head-data marker) → `MNAM` (male section) → `INDX 0` → `MODL`. The female head follows in the `FNAM` section and is already captured — with a gender tag — in `RaceRecord::head_parts`, which the same function reads two statements later for the eyes. It is never consulted for the head.

## Evidence

`byroredux/src/npc_spawn/resumable.rs:380-382`:

```rust
let head_path = race
    .and_then(|race| race.body_models.first())
    .map(|path| normalize_mesh_path(path).into_owned());
```

Contrast the eye selection 34 lines below, which *does* honour the tag (`resumable.rs:411-421`):

```rust
.filter(|(part_idx, path, section)| {
    (*part_idx == ...::head_part::LEFT_EYE
        || *part_idx == ...::head_part::RIGHT_EYE)
        && !path.is_empty()
        && section.is_none_or(|tag| tag == want_gender_tag)
})
```

Real sub-record sequence of `CaucasianOldAged` (`000987DF`), dumped from `FalloutNV.esm` by this audit:

```
EDID  NAM0  MNAM  INDX:0  MODL:Characters\Head\HeadOld.NIF   ...
                  INDX:6  MODL:Characters\Head\EyeLeftHuman.NIF
                  INDX:7  MODL:Characters\Head\EyeRightHuman.NIF
            FNAM  INDX:0  MODL:Characters\Head\HeadOldFemale.NIF  ...
                  INDX:6  MODL:Characters\Head\EyeLeftHumanFemale.NIF
```

Census over all `FalloutNV.esm` RACE records: **22 of 22 races author a female head that differs from the male head**, and `body_models[0]` is the male head in every one of them (`HeadOld.NIF` / `HeadOldFemale.NIF`, `HeadHuman.NIF` / `HeadFemale.NIF`). 987 of 3 816 FNV NPCs are female.

## Impact

Every female FNV NPC gets a male head. This is worse than a swapped mesh: the runtime-FaceGen path (`spawn_runtime_head`, `resumable.rs:852-930`) then applies that NPC's authored `FGGS`/`FGGA` morph deltas — sliders authored against the *female* base head — to the male base mesh through the `.egm` morph basis, so the resulting face is not the male head either. Every named female character on the reference title is affected.

Secondary fragility worth fixing in the same pass: `body_models` is an ordering-dependent flat list that also contains the body-section meshes and the `.egt` texture paths (`Characters\_Male\UpperBodyHumanMale.egt` is `body_models[12]` on `CaucasianOldAged`). `first()` returning a head at all is incidental.

## Related

FNV-2026-08-27-D4-03 (the same accumulator's other defect) **must land first** — until the RACE body section stops leaking into `head_parts`, a `head_part::HEAD` lookup resolves to `UpperBody.nif`. `humanoid_body_paths` (`npc_spawn.rs:348-379`) is the *body* sibling and is fully gender-aware (#3037), which is why the asymmetry went unnoticed.

## Suggested Fix

Resolve the head from `race.head_parts` by `(head_part::HEAD, gender_section)` with the same `section.is_none_or(|tag| tag == want_gender_tag)` rule the eye filter already uses, falling back to the untagged/male entry. Add a real-data test over `FalloutNV.esm` asserting a female `Caucasian` actor resolves `HeadFemale.NIF`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the eye / hair / brow selectors in the same function, and the Oblivion arm of the same spawner)
- [ ] **TESTS**: A regression test pins this specific fix
