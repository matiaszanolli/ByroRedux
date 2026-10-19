# SF-SD-01: Starfield small / medium master FormIDs use a file-local `0xFE` / `0xFD` encoding the remap treats as a master-list index; multi-plugin loads collide and mis-slot

Labels: high,esm-plugin,bug,game:starfield,legacy-compat

Severity HIGH: silent wrong-record merging under a realistic load order; the workaround is to load only regular plugins.

## Description
`FormIdRemap::remap` (`crates/plugin/src/esm/reader.rs`, owner `/audit-esm`) decodes `top byte == master count` as "self" and `top byte < master count` as `master_slots[top byte]`, the Skyrim SE / FO4 rule that `form_id_remap_esl_plugin_own_forms_decode_into_light_space` pins. Starfield's real files follow a different rule:
1. A small master's own records are `0xFE` + a 12-bit field + a 12-bit object id. The 12-bit field is the number of small masters in the plugin's own `MAST` list.
2. A medium master's own records are `0xFD` + an 8-bit field (the count of medium masters in its `MAST` list) + a 16-bit id.
3. A reference to a small or medium master uses the same `0xFE` / `0xFD` form, with the master's ordinal among the small (or medium) masters of this file.
4. A regular plugin's top byte indexes regular masters only. Its own records use the count of regular masters; overrides of a regular master use that master's regular ordinal, skipping small and medium entries.
5. As a result `remap` mis-routes every `0xFE` / `0xFD` FormID. The top byte exceeds the master count, so it takes the "out-of-range" arm and passes through unchanged with a `log::warn!`, whatever the plugin's real slot is.
6. For a regular plugin with a small master in its `MAST` list, `remap` mis-slots even the regular references: the top byte is a regular ordinal but is used as a `MAST` position.

Also affected downstream: `byroredux/src/cell_loader/load_order.rs` `build_remap_for_plugin` (builds `master_slots` positionally from the `MAST` order), `plugin_for_form_id`, and the `GlobalFormIdResolver` identity built from the same slots. Re-validated: `remap` still has only the `plugin_slot` / `master_slots` chain, no Starfield decode. #4639 (closed) fixed the TES4 flag bits and slot *allocation*, not how Starfield addresses those slots inside the files.

## Evidence (install under `Starfield/Data`, byte-level probes)
- **Own records**: `Constellation.esm` (small, masters `[Starfield]`): `0xFE000002`, `0xFE00001A` .. (sub field 0). `SFBGS019.esm` (small, masters `[Starfield, SFBGS004(small)]`): all 29 records `0xFE001xxx` (sub 1). `SFBGS047.esm` (small, masters `[Starfield, SFBGS004(small), SFBGS00D(regular)]`): all 40 own records `0xFE001xxx` (sub 1). `SFBGS003.esm` and `SFBGS006.esm` (medium): 64,384 and 60,890 records, all `0xFD00xxxx`. The NPC `0xFD000E90` in `SFBGS003.esm` has its FaceGen file named `00000e90.nif`.
- **References**: `SFBGS047.esm` `KWDA` entries `0xFE0002A5` and `0xFE000017` resolve to `KYWD` records in `SFBGS004.esm` (its first small master, ordinal 0), not in `Constellation.esm`. `outpostmissionboards.esm` (masters `[Starfield, BlueprintShips, SFBGS003(medium)]`) references `0xFD000050`.
- **Regular-ordinal rule**: `SFBGS047.esm`'s two override records of `SFBGS00D.esm` (`WRLD 0x010079E3`, `CELL 0x0100873F`) have top byte `0x01`; `MAST` position 2 holds `SFBGS00D`, and both low-24-bit ids exist in `SFBGS00D.esm`. `better_living.esm` (regular, masters `[Starfield, BlueprintShips, SFBGS004(small)]`) writes its own 12,217 records with top byte `0x02`, while `master_slots.len()` is 3: `remap` sends them to `master_slots[2]` (`SFBGS004`'s light slot) and `compose` truncates the id to 12 bits.
- **Collision measurement** (22 official plugins, 18 of them small or medium): pass-through leaves 67,217 distinct `0xFE` / `0xFD` FormIDs, and **61,477 of them are claimed by two or more plugins**. E.g. `0xFE000002` is a record in 13 plugins. `EsmIndex::merge_from` keeps the later record, so one plugin's record silently replaces another's.
- Disproofs: no Starfield pre-pass normalizes the encoding (`FormIdRemap` has only `plugin_slot` and `master_slots`); the 12-bit field is file-local (several plugins share sub 0 and sub 1), not a global slot; the supported vanilla path (`Starfield.esm` alone, regular DLCs `ShatteredSpace`, `SFBGS00D`, `SFBGS050`, `BlueprintShips-*`) has only regular masters and is untouched, and the first small master loaded alone works by luck.
- Loads that hit it: `--master Constellation.esm --master SFBGS004.esm ...` (any two) or a Creations-style order. The install also holds many third-party plugins flagged small (`0x101`) or medium. Established from file bytes plus a static read of `remap`, not a live multi-plugin run.

## Impact
Any Starfield load order with two or more small (or two or more medium) masters merges distinct forms onto one global FormID (record, NPC, REFR, KYWD, GLOB ...). Cross-plugin references and quest / script identity resolve to the wrong record. `plugin_for_form_id` then names the wrong plugin, which feeds FaceGen paths. A regular plugin with a small master in its list loses its own records into that master's slot. The log fills with a `FormID ... has mod_index 254 but plugin has N masters` warning per record.

## Related
#4639, the DLC FaceGen path issue (FO4-SD-01 family, canonical issue created in this run), SF-SD-02 / SF-SD-03.

## Suggested Fix
Give `FormIdRemap` a Starfield decode. Partition the `MAST` list into regular / small / medium ordinals. Route a `0xFE` top byte by `(raw >> 12) & 0xFFF` (ordinal among the file's small masters; equal to the small-master count means self when the plugin is small) and `0xFD` by `(raw >> 16) & 0xFF`, with the object id masked to 12 / 16 bits. Index a regular top byte among regular masters only, with self at the regular-master count. Pin it with fixtures built from the `SFBGS019` / `SFBGS047` / `better_living` shapes above, and add a default-lane test that loads two synthetic small masters and asserts no shared global id.

Source: docs/audits/AUDIT_STARFIELD_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
