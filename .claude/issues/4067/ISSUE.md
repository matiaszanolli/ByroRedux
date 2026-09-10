# #4067 — ESM-2026-09-09-D7-01

`CommonNamedFields` stores `SCRI` raw, so `base_record_script` returns two different FormID spaces — every scripted ACTI / TERM / item in a non-first plugin either dangles or attaches the *wrong* script

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4067 --json state`).

---

- **Severity**: HIGH
- **Dimension**: ESM→ECS Handoff
- **Record / Sub-record**: `ACTI` / `TERM` / `WEAP` / `ARMO` / `AMMO` / `MISC` / `KEYM` / `ALCH` / `INGR` / `BOOK` / `NOTE` — `SCRI`
- **Location**: `crates/plugin/src/esm/records/common.rs:311-314` (the raw read),
  `crates/plugin/src/esm/records/common.rs:395` (`CommonItemFields` copy),
  `crates/plugin/src/esm/records/misc/world.rs:1437` (`parse_acti`),
  `crates/plugin/src/esm/records/misc/world.rs:1515` (`parse_term`);
  consumer `byroredux/src/cell_loader/references/attach.rs:240-252`
- **Status**: NEW
- **Description**: `CommonNamedFields::from_subs_with_remap` takes the load-order
  `remap` and applies it to the `VMAD` payload it decodes on the very next arm —
  but reads `SCRI` with a bare `u32_or_default()`. Three of its four consumers then
  copy that raw value straight onto the indexed record; only `parse_container`
  remaps it. `parse_npc` (`actor/mod.rs:1032`) and the statics-family builder
  (`cell/support.rs:106-112`, #3941) remap their own `SCRI`. The result is that
  `EsmIndex::base_record_script` — the single accessor the ObScript attach lane
  goes through — returns **global-space** FormIDs for CONT / NPC_ / CREA / statics
  and **plugin-local** FormIDs for ACTI / TERM / items, while `EsmIndex::scripts`
  is keyed entirely in global space (`dispatch_misc_gameplay_a.rs:42` inserts under
  the header-remapped `fid`).
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/records/common.rs:311-314  — inside from_subs_with_remap
                  b"SCRI" if sub.data.len() >= 4 => {
                      out.script_form_id =
                          crate::esm::sub_reader::SubReader::new(&sub.data).u32_or_default();
                  }
  ```
  ```rust
  // crates/plugin/src/esm/records/misc/world.rs:1437 (parse_acti) and :1515 (parse_term)
          script_form_id: common.script_form_id,
  ```
  ```rust
  // crates/plugin/src/esm/records/container.rs:109 — the one caller that does remap
          script_form_id: remap_fid(common.script_form_id, remap),
  ```
  ```rust
  // crates/plugin/src/esm/records/index.rs:798-800 — the mixed-space accessor
          if let Some(r) = self.items.get(&base_form_id) {
              return nonzero(r.common.script_form_id);
          }
  ```
  ```rust
  // byroredux/src/cell_loader/references/attach.rs:240-243 — the live lookup
      let Some(script_form_id) = index.base_record_script(base_form_id) else {
          return false;
      };
      let Some(script) = index.scripts.get(&script_form_id) else {
  ```
  `FormIdRemap`'s own doc comment names the exact collision this creates
  (`reader.rs:381-384`): *"multi-plugin loads silently collided on the shared
  base-game-index 0x01 (Anchorage / BrokenSteel / PointLookout / Pitt / Zeta all
  use 0x01 for their own new forms)"*.
- **Impact**: on any multi-plugin ObScript load order (Oblivion + SI/Knights,
  FO3 + its 5 DLCs, FNV + its 4 DLCs + GRA — i.e. the reference title's normal
  configuration), a DLC-defined scripted activator or terminal resolves its `SCRI`
  in the DLC's own local space. Two outcomes, both silent: (a) the raw id names no
  record and the attach logs at `log::debug!` and returns `false` — terminals,
  levers, traps and quest activators never get a script; or (b) the raw id
  *collides* with a real global-space SCPT belonging to whichever plugin occupies
  that top byte, and the wrong script is attached and executed. (b) is the worse
  half and there is nothing in the pipeline that can detect it. The dangling half
  is misreported by the existing comment at `attach.rs:244-247` as *"genuinely a
  broken plugin / parser bug"* — it is a parser bug, but not the one that comment
  predicts. Single-master runs (`--esm FalloutNV.esm` alone) are unaffected because
  the remap is identity there, which is why this survives every default CLI invocation.
- **Related**: #3714 / #3715 (the same defect class, closed for ARMO/ARMA/RACE and
  the 11-site sweep); #3941 (added the statics arm, and correctly remapped it);
  ESM-2026-08-30-D3-02.
- **Suggested Fix**: move the remap into `CommonNamedFields::from_subs_with_remap`
  itself (`out.script_form_id = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap)`),
  matching what the sibling `VMAD` arm already does, and delete the now-redundant
  `remap_fid` at `container.rs:109`. That makes the field un-bypassable rather than
  fixing three call sites and waiting for a fourth. Add a regression test that
  builds an ACTI under a non-identity `FormIdRemap::regular(2, vec![0])` and asserts
  `base_record_script` returns the composed global id.

---
