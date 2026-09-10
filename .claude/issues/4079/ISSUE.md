# #4079 — ESM-2026-09-09-D3-03

the DIAL topic-children parent check compares a raw GRUP label against a remapped record FormID, so it always disagrees on a multi-plugin load

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4079 --json state`).

---

- **Severity**: LOW
- **Dimension**: FormID & Load Order
- **Record / Sub-record**: `DIAL` GRUP type 7 (topic children)
- **Location**: `crates/plugin/src/esm/records/grup_walker.rs:169-190`
- **Status**: NEW
- **Description**: A type-7 topic-children GRUP's label is the parent `DIAL`'s
  plugin-local FormID. `last_dial_form_id` is set from `header.form_id`, which
  `read_record_header` has already remapped. The two are then compared directly.
- **Evidence**:
  ```rust
  let parent_form_id = u32::from_le_bytes(sub_group.label);   // raw, from the GRUP label
  let target = last_dial_form_id.unwrap_or(parent_form_id);
  if Some(parent_form_id) != last_dial_form_id {
      log::debug!("DIAL Topic Children sub-group label {:#x} doesn't match \
                   most-recent DIAL form_id {:?}; …  — see #631", …);
  }
  ```
  `last_dial_form_id` is assigned at `:212` (and `:344` in the QUST-nested walker) from
  the already-remapped `header.form_id`.
- **Impact**: two small ones. (a) On any load order where the remap is non-identity, the
  comparison is guaranteed to fail for **every** topic-children group, so the `#631`
  drift diagnostic becomes pure noise at `debug` and stops being able to report the real
  drift it was written for. (b) The `unwrap_or(parent_form_id)` fallback would key an
  INFO list by a raw id — unreachable on well-formed content (a type-7 group always
  follows its `DIAL`), which is what keeps this LOW.
- **Related**: #631, #3400.
- **Suggested Fix**: `let parent_form_id = remap_fid(u32::from_le_bytes(sub_group.label), &remap);`
  — `remap` is already in scope at `:157` and `:287`.

---
