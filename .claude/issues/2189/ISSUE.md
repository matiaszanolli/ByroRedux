# SCR-D7-NEW4-01: Item-record family never gets its VMAD script decoded

**Issue**: #2189
**Labels**: medium, import-pipeline, legacy-compat, bug
**Dimension**: Engine Attach Path & Trigger-Volume Wiring (Dimension 7), root cause in `crates/plugin`
**Untrusted-Input**: No (a real-content-coverage gap, not a hostile-input path)
**Location**: `crates/plugin/src/esm/records/common.rs:298-318` (`CommonItemFields` — only a presence-only `has_script: bool`, no `script_instance` field); contrast `crates/plugin/src/esm/records/common.rs:248-266` (`CommonNamedFields`, which fully decodes `script_instance: Option<ScriptInstanceData>`); `crates/plugin/src/esm/records/items.rs` (every `parse_weap`/`parse_armo`/`parse_ammo`/`parse_misc`/`parse_keym`/`parse_alch`/`parse_ingr`/`parse_book`/`parse_note` builds its `ItemRecord.common` via `CommonItemFields::from_subs`); `crates/plugin/src/esm/records/index.rs:599-616` (`base_record_script_instance` — walks `activators`/`containers`/`npcs`/`creatures` only, can't reach `items` even in principle since `ItemRecord` carries no `script_instance` field to return)
**Status**: NEW

## Description

Two near-identical "common fields" structs exist for ESM record parsing. `CommonNamedFields` (used by `ACTI`, `CONT`, `SCOL`, `PKIN`, `TREE`, and — via `NpcRecord`, shared with `CREA` per #1273 — `NPC_`/`CREA`) fully decodes a `VMAD` sub-record into `script_instance: Option<ScriptInstanceData>` via `ScriptInstanceData::parse(&sub.data)`. `CommonItemFields` — used by every item-family record parser (`WEAP`, `ARMO`, `AMMO`, `MISC`, `KEYM`, `ALCH`, `INGR`, `BOOK`, `NOTE`) — only sets a presence-only `has_script: bool` flag and has **no `script_instance` field at all**.

`base_record_script_instance` (the M47.2 attach path's VMAD accessor, `index.rs:599`) walks `self.activators`/`self.containers`/`self.npcs`/`self.creatures` and returns each one's `script_instance` — it structurally cannot reach `self.items`, because `ItemRecord.common: CommonItemFields` has nowhere to store a decoded VMAD even if the lookup arm were added.

The doc comment on `CommonItemFields::has_script` (`common.rs:312-316`) blames this on work "gated on the scripting-as-ECS work tracked at M30.2/M48" — but that work has demonstrably shipped (this whole audit domain is M47.2's live decompile+recognizer chain), just never extended to this one struct. The referenced tracking issue, #369 ("VMAD sub-records skipped on every Skyrim record"), is CLOSED — its fix evidently covered `CommonNamedFields`'s consumers but not `CommonItemFields`'s.

## Evidence

`common.rs:284-290` (`CommonNamedFields::from_subs`'s `VMAD` arm sets both `has_script = true` AND `script_instance = Some(ScriptInstanceData::parse(&sub.data))`) vs. `common.rs:338-339` (`CommonItemFields::from_subs`'s `VMAD` arm: `b"VMAD" => out.has_script = true,` — one line, no parse call, no field to hold the result even if it wanted to). `items.rs`'s 9 `parse_*` functions all build from `CommonItemFields`. `index.rs:603-615`'s `base_record_script_instance` match arms: `activators`, `containers`, `npcs`, `creatures` — no `items` arm exists, and none could return anything useful even if added under the current `ItemRecord` shape.

## Impact

Any Skyrim+/FO4+ weapon, armor piece, potion/ingestible, book, key, ammo, or ingredient record that carries a `VMAD`-attached Papyrus script (e.g. an `OnEquip`/`OnUnequip` hook granting a temporary effect, a quest book that fires a stage-advance on read, a scripted key) silently never attaches its script — `attach_vmad_scripts` calls `index.base_record_script_instance(base_form_id)`, which returns `None` for every item-family base record regardless of what its `VMAD` actually contains. This is a silent content-coverage gap (a decline, not a wrong lowering — no game state gets corrupted), but it is real and previously unflagged across all seven prior audit passes of this domain. No corpus scan was run to quantify how many real Skyrim/FO4 item records actually carry a non-trivial VMAD; severity is set at MEDIUM (a documented content-family gap, not proven HIGH-frequency) pending that measurement.

## Related

Superficially related to #369 (closed) — this is effectively an unclosed sibling of that fix, in the one record family (`CommonItemFields`) its closure didn't reach.

## Suggested Fix

Add a `script_instance: Option<ScriptInstanceData>` field to `CommonItemFields`, populate it the same way `CommonNamedFields` does (`ScriptInstanceData::parse(&sub.data)` in the `VMAD` arm), thread it into `ItemRecord`, and add an `items` arm to `base_record_script_instance` (returning `r.common.script_instance.as_ref()`, mirroring the existing arms). Update the stale doc comment on `has_script` once the decode lands. Before prioritizing, consider running a quick corpus census (real `Skyrim.esm`/`Fallout4.esm`) of how many `WEAP`/`ARMO`/`ALCH`/`BOOK`/etc. records actually carry a non-empty `VMAD`, to convert the MEDIUM estimate into a measured severity.

## Completeness Checks
- [ ] **TESTS**: A regression test mirroring `common_named_fields_decodes_vmad_script_instance` but for `CommonItemFields`, plus a `base_record_script_instance` test resolving a VMAD-attached script off an `items` entry
- [ ] **SIBLING**: Once fixed, re-check whether any other "common fields"-style struct in `crates/plugin/src/esm/records/` has the same presence-only gap
