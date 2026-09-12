# #4222 — TD2-003: `movs.rs`/`scol.rs` re-implement EDID/MODL/VMAD presence logic that `CommonNamedFields::from_subs_with_remap` already generalizes

**Description**: `common.rs` documents `from_subs_with_remap` as the intended single funnel, already followed at several tracked sites (`container.rs` #1045, `misc/world.rs` #2068, `misc/scene.rs` #2414). `movs.rs` hand-rolls its own EDID/MODL/VMAD loop instead. `scol.rs` does call the shared helper but its second SCOL-specific loop still re-adds a redundant `VMAD => has_script = true` arm, discarding the shared result in favor of a local shadow.

**Suggested Fix**: In `movs.rs`, replace the manual loop with `CommonNamedFields::from_subs_with_remap`; in `scol.rs`, drop the redundant arm and use the already-computed `common.has_script`.

## Completeness Checks
- [x] **SIBLING**: Checked `container.rs`/`misc/world.rs`/`misc/scene.rs` for the correct pattern to mirror.
- [x] **TESTS**: Existing regression tests (`crates/plugin/src/esm/cell/tests/movs.rs`, `scol.rs`'s own `#[cfg(test)] mod tests`) already exercise EDID/MODL/VMAD/has_script and confirmed passing unchanged after the refactor.

**Disposition**: `movs.rs::parse_movs` now calls `CommonNamedFields::from_subs_with_remap` for EDID/MODL/VMAD instead of a hand-rolled loop; the MOVS-specific LNAM/ZNAM/DEST sub-records keep their own loop. `scol.rs::parse_scol` dropped the redundant `b"VMAD" => has_script = true` arm and now returns `common.has_script` directly. 11 MOVS tests + 17 SCOL tests all pass unchanged.

---

# #4223 — TD4-001: `audit-esm/SKILL.md` backticks a test helper that no longer exists — the paragraph already names its replacement

**Description**: `record_parsers_with_embedded_form_ids_take_a_remap` no longer exists (`48acaea2` inverted the guard to `all_record_parsers()`); the same paragraph already narrates the rename two sentences later.

**Suggested Fix**: Replace the backticked dead name with italics or the current `all_record_parsers()` name.

## Completeness Checks
- [x] **TESTS**: N/A — documentation-only fix.

**Disposition**: Italicized the dead name and added an explicit cross-reference to the current `all_record_parsers()` name, per the path-validation gate's advisory convention (backticks assert liveness; italics mark historical/renamed names). `_audit-validate.sh` passes clean.

---

# #4224 — TD4-003: `audit-fnv/SKILL.md`'s own historical `boot.rs` mention is backticked, contrary to the gate's advisory convention

**Description**: Prose is correct ("the former `boot.rs`") but still backticks a name that resolves nowhere in the tree; the path-gate's convention is italics for historical/dead names, not backticks.

**Suggested Fix**: Change to italics: "the former *boot.rs*".

## Completeness Checks
- [x] **TESTS**: N/A — documentation-only fix.

**Disposition**: Changed to italics exactly as suggested. `_audit-validate.sh` passes clean.

---

# #4225 — TD6-006: `PerkRecord` doc comment is stale — claims CTDA/EPFD decode are still "follow-ups"

**Description**: `parse_perk` already fully handles per-entry CTDA (`push_ctda`) and EPFD-by-function_type decode (types 1-5), contradicting the struct doc comment's claim these are deferred follow-ups.

**Suggested Fix**: Update the doc comment to reflect that CTDA/EPFD are implemented; name any specific unhandled `function_type` values explicitly if any remain.

## Completeness Checks
- [x] **TESTS**: N/A — documentation-only fix; existing tests already pin the decode this comment now accurately describes.

**Disposition**: Verified all five documented `function_type` values (1=None, 2=Float, 3=Range, 4=FormId — remapped per #4069, 5=LString — not remapped, an lstring index) are fully decoded in `parse_perk`'s EPFD arm, with an explicit `_ => PerkFunctionData::None` catch-all for anything else (no specific values are left unhandled — the catch-all is the deliberate fallback, not a gap). Rewrote the `PerkRecord` doc comment to state CTDA/EPFD are implemented, naming `push_ctda` and the function_type range directly instead of describing them as follow-ups.
