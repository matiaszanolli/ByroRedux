# #3421 — FNV-2026-08-27-D5-01: `RaceRecord::head_parts`' doc mis-numbers the FNV head-part table

**Labels**: low, documentation, doc-rot, esm-plugin, game:fnv, legacy-compat

**Filed**: 2026-08-27 · from `docs/audits/AUDIT_FNV_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-27.md` — finding `FNV-2026-08-27-D5-01` (HEAD `969d81c8`)

- **Severity**: LOW
- **Dimension**: 5 — NIF/ESM regression guard (doc rot)
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:436-457`

## Description

Two claims in `RaceRecord::head_parts`' doc comment are falsified by the shipped data. The doc numbers the head parts `0 Head · 1 Ear (male) · 2 Ear (female) · 3 Mouth · 4 Teeth (lower) · 5 Teeth (upper) · 6 Tongue · 7 Left Eye · 8 Right Eye`, and states that `gender_section` is `None` for head entries because *"every 'Head' entry lands here"* — i.e. before any `MNAM`/`FNAM` marker.

## Evidence

Measured `INDX` values in `FalloutNV.esm`'s RACE head section are `0 Head · 1 Ear · 2 Mouth · 3 Teeth lower · 4 Teeth upper · 5 Tongue · 6 Left Eye · 7 Right Eye` — one lower than the doc from `Mouth` on. The `head_part` constants immediately below (`mod.rs:533-541`) already encode the **correct** numbering (`MOUTH = 2`, `LEFT_EYE = 6`, `RIGHT_EYE = 7`), and their own doc comment even records that "UESP's `RACE_HeadPart` table claims 7/8 for eyes — vanilla data disagrees" — so the runtime is right and only the field's prose is wrong.

And the real record opens `NAM0` → **`MNAM`** → `INDX:0` → `MODL`, so the head entries carry `Some(0)` / `Some(1)`, never `None`.

## Impact

No runtime effect. It is a false premise sitting on the exact field that FNV-2026-08-27-D4-02, -D4-03 and -D6-01 must all be fixed through, and it reads as authoritative ("per UESP RACE_HeadPart"). An implementer trusting the prose over the constants would introduce the off-by-one the constants avoid.

## Related

FNV-2026-08-27-D4-02, FNV-2026-08-27-D4-03, FNV-2026-08-27-D6-01.

## Suggested Fix

Renumber the doc table to match the constants, and replace the `None` claim with what the archive shows (`None` occurs only for a record that authors `INDX`/`MODL` before its first `MNAM` — none of the 22 FNV races do).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the `head_part` constants' own doc, and any other UESP-sourced table doc in `records/actor/`)
- [ ] **TESTS**: A regression test pins this specific fix (a real-data assertion on the measured `INDX` numbering would make the prose falsifiable)
