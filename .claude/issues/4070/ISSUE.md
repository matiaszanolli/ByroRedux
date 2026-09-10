# #4070 — ESM-2026-09-09-D7-02

`parse_mgef` remaps one of the three embedded FormIDs in the same eight-line block — `associated_item` and `effect_shader_id` stay raw, and the #3715 guard is structurally blind to it

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4070 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: ESM→ECS Handoff
- **Record / Sub-record**: `MGEF` / `DATA`
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:674`, `:680` (raw);
  `:678` (remapped); guard at `crates/plugin/src/esm/records/tests.rs:2242-2305`
- **Status**: NEW
- **Description**: `parse_mgef` was given the `remap` parameter by #3715 and is
  named in the `record_parsers_with_embedded_form_ids_take_a_remap` allowlist. It
  applies `remap_fid` to `light_form_id` only. `associated_item` (MGEF DATA @8) and
  `effect_shader_id` (MGEF DATA @32) are FormIDs by the struct's own offset table
  (`magic.rs:80,86`) and are copied through raw. The guard cannot see this: it
  greps the *signature* for `remap: &Option<FormIdRemap>`, not the body.
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/records/misc/magic.rs:671-681
                  if let Ok(header) = read_sub::<MagicEffectHeader>(sub) {
                      out.effect_flags = header.effect_flags;
                      out.base_cost = header.base_cost;
                      out.associated_item = header.associated_item;      // <- raw
                      out.magic_school = header.magic_school;
                      out.resistance_av = header.resistance_av;
                      // #3715 — embedded light-effect FormID.
                      out.light_form_id = remap_fid(header.light_form_id, remap);
                      out.projectile_speed = header.projectile_speed;
                      out.effect_shader_id = header.effect_shader_id;    // <- raw
                  }
  ```
  ```rust
  // crates/plugin/src/esm/records/tests.rs:2228-2240 — why the guard passes anyway
  fn parser_signature_takes_remap(source: &str, parser: &str) -> Result<bool, String> {
      ...
      Ok(signature.contains("remap: &Option<FormIdRemap>"))
  ```
- **Impact**: latent today — `grep` finds no consumer of `associated_item` or
  `effect_shader_id` outside `crates/plugin/src/esm/records/misc/magic.rs`. The defect is that the project's own
  countermeasure reports this parser as swept when two thirds of its embedded
  FormIDs are not, so the next consumer inherits a silently wrong key. Note the
  `associated_item` default is `0xFFFF_FFFF` (`magic.rs:645`), the "none" sentinel —
  a naive `remap_fid` would rewrite it, so the fix needs the sentinel guard that
  `remap_fid`'s `raw == 0` early-out does not cover.
- **Related**: #3715 (ESM-2026-08-30-D3-02) — this is the residual of that sweep.
- **Suggested Fix**: remap both fields (guarding `0xFFFF_FFFF` on `associated_item`),
  and strengthen the guard from a signature scan to a body scan: assert that inside
  each listed parser, every `u32_or_default()` / `read_form_id` whose result is
  assigned to a `*_form*` / `*_item` field passes through `remap_fid`. A
  signature-shaped guard cannot express the property it exists to protect.

---
