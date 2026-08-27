# SKY-2026-08-27-D4-02: `plugin_for_form_id` indexes the load-order list by position, not by global slot — every unresolved-REFR diagnostic names the wrong plugin once an ESL is not last, and never names an ESL at all

Labels: low,esm-plugin,bug,game:skyrim,legacy-compat

- **Severity**: LOW (diagnostic output only — no rendering or parse effect)
- **Confidence**: CONFIRMED (code read + reproduced with `_ResourcePack.esl`)
- **Location**: `byroredux/src/cell_loader/load_order.rs:31-34`
- **Description**: global slots are allocated by `allocate_global_slot` from **two
  independent counters** — regular plugins take `0x00..=0xFD` from `next_regular`, ESLs
  take a 12-bit sub-index in the `0xFE` space from `next_light`
  (`byroredux/src/cell_loader/load_order.rs:293-328`). The remap itself is correct
  because `build_remap_for_plugin` looks a master up by *load-order position* and then
  reads `slots[pos]`, keeping the two in step. But the diagnostic helper

  ```rust
  pub(super) fn plugin_for_form_id(form_id: u32, load_order: &[String]) -> Option<&str> {
      let mod_index = (form_id >> 24) as usize;
      load_order.get(mod_index).map(|s| s.as_str())
  }
  ```

  treats the top byte as a **load-order position**. Those two only coincide when no ESL
  precedes a regular plugin. An ESL anywhere but last shifts every later regular
  plugin's position past its slot byte, and an ESL-owned form (top byte `0xFE` = index
  254) falls off the end of the list entirely.
- **Evidence**: real 5-plugin order with the ESL in the middle
  (`skyrim.esm, update.esm, hearthfires.esm, _resourcepack.esl, dragonborn.esm` —
  `_ResourcePack.esl` declares `["Skyrim.esm","Update.esm","HearthFires.esm"]` so this
  order is legal):

  ```
  order = ["skyrim.esm","update.esm","hearthfires.esm","_resourcepack.esl","dragonborn.esm"]
  statics with top byte 0x03: [03028434, 030384C1, 030185ED]
    03028434 editor_id="DLC2EnchStalhrimGreatswordTurn05" -> plugin_for_form_id says Some("_resourcepack.esl")
    030384C1 editor_id="DLC2DweFacadeBalconyCap01_LOD"    -> plugin_for_form_id says Some("_resourcepack.esl")
    030185ED editor_id="DLC2TreePineForestStump01Ash"     -> plugin_for_form_id says Some("_resourcepack.esl")
    ESL form FE0000E4 editor_id="RP_SWellFreeStanding01CoverStaticAlpha" -> says None
    ESL form FE00014D editor_id="RP_RoadCurveLong45R01DesertLumpy01Light" -> says None
  ```

  `DLC2*` editor IDs are unambiguously Dragonborn.esm content, reported as
  `_resourcepack.esl`. ESL-owned forms report `None`, which the callers render as
  `"???"` / `"Engine.esm"`.
- **Impact**: the #561 "name the missing master" completeness guarantee is false in
  exactly the configuration it is most needed — a mixed ESM/ESL load order. The
  unresolved-base-object breakdown in
  `byroredux/src/cell_loader/references/complete.rs:268` and `:289` and the synth-child
  provenance stamps in `byroredux/src/cell_loader/references/synth_child.rs:58` / `:625`
  will point the user at the wrong plugin, sending them to add a master they already
  have. No rendering impact — the remap that actually places geometry is a separate,
  correct code path.
- **Suggested Fix**: `parse_record_indexes_in_load_order` already computes
  `slots: Vec<GlobalSlot>` parallel to `load_order`; return it (or a prebuilt
  `HashMap<GlobalSlot, String>`) alongside the name list and have `plugin_for_form_id`
  decode the FormID into a `GlobalSlot` first — `Regular(top)` for `top <= 0xFD`,
  `Light((raw >> 12) & 0x0FFF)` for `top == 0xFE` — then look that slot up. That also
  makes ESL-owned forms nameable.
- **Related**: #561 (the "name the missing plugin" requirement), #1554 (the ESL slot
  split this helper was never updated for). Existing coverage
  (`plugin_for_form_id_resolves_top_byte_to_load_order_basename`,
  `byroredux/src/cell_loader/nif_light_spawn_gate_tests.rs:268`) only exercises
  all-regular orders, which is why it stayed green.

---

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
