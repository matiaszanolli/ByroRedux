# #4069 — ESM-2026-09-09-D3-02

three more parsers read embedded FormIDs outside the remap — `PERK.EPFD` inside a parser that already holds `remap`, plus `WTHR` and `SCPT`

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4069 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: FormID & Load Order
- **Record / Sub-record**: `PERK`/`EPFD`, `WTHR`/`MNAM`+`NNAM`, `SCPT`/`SCRO`+`SCRV`
- **Location**:
  - `crates/plugin/src/esm/records/misc/magic.rs:415-417` — `parse_perk` (`:276`) **takes `remap`**; the `EPFD` function-type-4 payload is built raw
  - `crates/plugin/src/esm/records/weather.rs:929-944` — `parse_wthr` (`:398`) takes no `remap`
  - `crates/plugin/src/esm/records/script.rs:185-196` — `parse_scpt` (`:110`) takes no `remap`
- **Status**: NEW. Sibling of D7-02 / D7-03 (same defect class, different parsers) and successor to ESM-2026-08-30-D3-02 / **#3715**, whose named set is fixed.
- **Description**: The guard `record_parsers_with_embedded_form_ids_take_a_remap`
  (`crates/plugin/src/esm/records/tests.rs:2242-2308`, allowlist now 17 entries) checks only
  that a **named** parser's **signature** contains `remap: &Option<FormIdRemap>`. Two blind
  spots follow, and both are occupied. `parse_perk` is on the list and holds `remap`, but
  its `EPFD` entry-point payload never uses it; `parse_wthr` and `parse_scpt` are simply
  absent from the list, so nothing asks whether they should take the parameter.
- **Evidence**: current `misc/magic.rs:405-421`:
  ```rust
  *function_data = match function_type {
      1 => PerkFunctionData::None,
      2 if d.len() >= 4 => PerkFunctionData::Float(f32::from_le_bytes([d[0], d[1], d[2], d[3]])),
      3 if d.len() >= 8 => PerkFunctionData::Range { … },
      4 if d.len() >= 4 => {
          PerkFunctionData::FormId(u32::from_le_bytes([d[0], d[1], d[2], d[3]]))   // raw
      }
      5 if d.len() >= 4 => PerkFunctionData::LString(…),
      _ => PerkFunctionData::None,
  };
  ```
  and `weather.rs:929-944`, the Skyrim `MNAM` / `NNAM` precipitation- and visual-effect
  references (`SPGD` / `RFCT` FormIDs by the field names the struct gives them):
  ```rust
  b"MNAM" if sub.data.len() >= 4 => {
      record.skyrim_precipitation_effect = Some(u32::from_le_bytes([…]));
  }
  b"NNAM" if sub.data.len() >= 4 => {
      record.skyrim_visual_effect = Some(u32::from_le_bytes([…]));
  }
  ```
  and `script.rs:185-196`, where `SCRV` and `SCRO` — the pre-Papyrus ObScript bytecode's
  cross-record literals, described in the arms' own comments as *"u32 FormID per entry"* —
  are pushed raw into `ScriptRecord::ref_form_ids`.
- **Impact**: **latent at HEAD** — I traced each and none has a live cross-record consumer
  (`grep -rn "PerkFunctionData::FormId\|skyrim_precipitation_effect\|skyrim_visual_effect\|ref_form_ids" crates byroredux`, excluding their own decoders and tests, returns only
  `records/tests.rs:1324`). Their cost is that each is one consumer away from becoming
  D3-01 or D7-01, which is exactly how both of those happened. `PERK.EPFD` is the closest to
  a consumer: perks feed CHARAL, and an entry-point that adds a spell or ability is the
  natural next wiring step.
- **Related**: #3715, #3401, #3400; D3-01, D7-01, D7-02, D7-03.
- **Suggested Fix**: short term, thread `remap` into `parse_wthr` and `parse_scpt`, wrap the
  `EPFD` type-4 arm in `remap_fid`, and add all three to the allowlist. The structural fix is
  the one #3715's own report proposed and deferred: **invert the guard** so it asserts no
  `records/**.rs` file contains a FormID-shaped read outside a `remap_fid(` / `read_form_id(`
  call — an allowlist cannot detect the drift it was written to prevent, and this is the
  third consecutive audit to file the same class. Note for whoever writes that check: a
  purely line-local scan produces false positives on the **post-pass** idiom
  (`parse_regn`, `parse_navm` — see Disproved Candidates), so it must be function-scoped.

---
