# #4071 — ESM-2026-09-09-D7-03

three more parsers read embedded FormIDs with no `remap` parameter at all — `parse_spel`, `parse_ench` (`EFID`→MGEF) and `parse_mesg` (`QNAM`→QUST)

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4071 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: ESM→ECS Handoff
- **Record / Sub-record**: `SPEL` / `EFID`, `ENCH` / `EFID`, `MESG` / `QNAM`
- **Location**: `crates/plugin/src/esm/records/misc/magic.rs:536-546`
  (`MagicEffectAccumulator::feed`), `:576` (`parse_spel`), `:721` (`parse_ench`);
  `crates/plugin/src/esm/records/misc/dialogue.rs:524`, `:539-541` (`parse_mesg`)
- **Status**: NEW
- **Description**: `EFID` is a MGEF cross-reference and `QNAM` is a QUST
  cross-reference. All three parsers take `(form_id, subs)` and nothing else, so
  the values land in `EsmIndex.spells` / `.enchantments` / `.messages` in
  plugin-local space while `EsmIndex.magic_effects` and `.quests` are keyed
  globally. None of the three is in the #3715 allowlist — the same "the allowlist
  simply never named these parsers" shape that finding's own comment describes.
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/records/misc/magic.rs:536-541
      fn feed(&mut self, sub: &SubRecord) {
          match &sub.sub_type {
              b"EFID" if sub.data.len() >= 4 => {
                  self.pending_efid = SubReader::new(&sub.data).u32_or_default();
              }
  ```
  ```rust
  // crates/plugin/src/esm/records/misc/magic.rs:576   pub fn parse_spel(form_id: u32, subs: &[SubRecord]) -> SpelRecord {
  // crates/plugin/src/esm/records/misc/magic.rs:721   pub fn parse_ench(form_id: u32, subs: &[SubRecord]) -> EnchRecord {
  // crates/plugin/src/esm/records/misc/dialogue.rs:524 pub fn parse_mesg(form_id: u32, subs: &[SubRecord]) -> MesgRecord {
  ```
  ```rust
  // crates/plugin/src/esm/records/misc/dialogue.rs:539-541
              b"QNAM" if sub.data.len() >= 4 => {
                  out.owner_quest = SubReader::new(&sub.data).u32_or_default();
              }
  ```
- **Impact**: latent — `grep -rn effect_form_id` finds no consumer outside
  `crates/plugin/src/esm/records/misc/magic.rs`, and `owner_quest` likewise. But `SpelRecord.effects` /
  `EnchRecord.effects` are exactly what a magic-effect runtime and the `eitm`
  weapon-enchantment chain will read (the ENCH doc block at `magic.rs:689-694`
  names that consumer as the reason the record is decoded at all), and MESG→QUST
  is on the terminal/notification path M47 is building toward. Filing now costs
  three parameters; filing after the consumer lands costs the #3714 debugging
  session again.
- **Related**: #3715, #3400, #3401; ESM-2026-09-09-D7-02 (same guard blind spot).
- **Suggested Fix**: thread `remap` into `parse_spel` / `parse_ench` / `parse_mesg`
  and `MagicEffectAccumulator::feed`, apply `remap_fid` at the three read sites,
  and add all three to the `record_parsers_with_embedded_form_ids_take_a_remap`
  list. The list is hand-maintained; consider generating it by scanning
  `records/**` for `u32_or_default()` reads assigned to FormID-named fields
  instead, so a new parser cannot be omitted by forgetting to add a row.

---
