# #3419 — FNV-2026-08-27-D4-03: `RaceRecord::head_parts` also absorbs the RACE body section

**Labels**: medium, bug, esm-plugin, game:fnv, legacy-compat

**Filed**: 2026-08-27 · from `docs/audits/AUDIT_FNV_2026-08-27.md`

---

**Source**: `docs/audits/AUDIT_FNV_2026-08-27.md` — finding `FNV-2026-08-27-D4-03` (HEAD `969d81c8`)

- **Severity**: MEDIUM
- **Dimension**: 4 — ESM Record Parser
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:1403-1409` (accumulator) · `:436-458` (the field's contract) · `:533-541` (the constants)

## Description

The FNV RACE walker pairs every `INDX` with the next `MODL` and pushes it into `head_parts` unconditionally. But FO3/FNV RACE records re-use `INDX` — and re-use the `MNAM`/`FNAM` gender markers — for the **body** section that follows `NAM1`, with its own indices 0..3. The typed map documented as "FNV / FO3 `INDX` + `MODL` head-part pairs" therefore also contains body meshes and a texture path keyed under head-part indices.

## Evidence

`crates/plugin/src/esm/records/actor/mod.rs:1403-1409`:

```rust
b"MODL" => {
    let path = read_zstring(&sub.data);
    if let Some(idx) = pending_indx.take() {
        record.head_parts.push((idx, path.clone(), gender_section));
    }
    record.body_models.push(path);
}
```

There is no `NAM1` arm anywhere in `parse_race` to close the head section (`grep -n 'b"NAM1"' crates/plugin/src/esm/records/actor/mod.rs` → no match; the only `MNAM`/`FNAM` arms are the two gender-section markers at `:1397` / `:1400`). Real tail of `CaucasianOldAged`, after `NAM1`:

```
NAM1  MNAM  INDX:0  MODL:characters\_Male\UpperBody.nif
              INDX:1  MODL:characters\_Male\LeftHand.nif
              INDX:2  MODL:characters\_Male\RightHand.nif
              INDX:3  MODL:Characters\_Male\UpperBodyHumanMale.egt
      FNAM  INDX:0  MODL:Characters\_Male\FemaleUpperBody.NIF
              ...
```

So `head_parts` for that race contains `(0, "characters\_Male\UpperBody.nif", Some(0))` under `head_part::HEAD = 0`, `(2, "…\RightHand.nif", Some(0))` under `head_part::MOUTH = 2`, and `(3, "…UpperBodyHumanMale.egt", Some(0))` under `head_part::TEETH_LOWER = 3`.

## Impact

Latent today rather than live — the only consumer (`resumable.rs:414-421`) filters on `LEFT_EYE = 6` / `RIGHT_EYE = 7`, and the body section authors no `INDX` above 3, so no current lookup misfires. But `head_part::{HEAD, MOUTH, TEETH_LOWER, TEETH_UPPER, TONGUE}` are public constants sitting beside `LEFT_EYE`/`RIGHT_EYE` with a doc comment inviting exactly that lookup, and the fix for FNV-2026-08-27-D4-02 is the natural first consumer of `head_part::HEAD` — which would resolve to `UpperBody.nif` on a first-match search unless the contamination is removed first. **These two findings must be scheduled together, this one first.**

## Related

FNV-2026-08-27-D4-02 (blocked on this); FNV-2026-08-27-D5-01 (the same field's doc rot); FNV-2026-08-27-D6-01.

## Suggested Fix

Track section state in `parse_race` — `NAM0` opens the head-data section, `NAM1` opens the body-data section — and push into `head_parts` only while inside the head section (`body_models` can keep taking everything, or gain a parallel `body_part_models`). Pin with a real-data assertion that no `head_parts` entry on any `FalloutNV.esm` RACE ends in `.egt` or lives under `characters\_male\`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the Oblivion RACE arm of the same walker, and any other `pending_indx`-style accumulator)
- [ ] **TESTS**: A regression test pins this specific fix
