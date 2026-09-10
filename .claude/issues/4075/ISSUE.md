# #4075 — ESM-2026-09-09-D7-05

four stale blocks in `docs/engine/plugin-loading.md` — an example that does not compile, an "in progress" status for shipped work, an `EsmCellIndex` missing 3 of its 13 fields, and three parsed record types listed as unparsed

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4075 --json state`).

---

- **Severity**: MEDIUM
- **Dimension**: ESM→ECS Handoff (doc rot)
- **Record / Sub-record**: `SOUN` / `MUSC` / `ASPC` — and the `EsmCellIndex` shape
- **Location**: `docs/engine/plugin-loading.md:233`, `:239-251`, `:284-291`, `:293-294`
- **Status**: NEW — re-verified against **code + doc** at HEAD. Carries
  `AUDIT_ESM_2026-08-13.md` ESM-D7-07 (its two halves are items 3 and 4 below);
  no matching GitHub issue exists in `/tmp/audit/esm/issues.json`.
- **Description**: four independent staleness sites in the one document that is
  ground truth for this tier.
  1. **`EsmCellIndex` block (`:239-251`)** lists 10 fields; the struct at
     `crates/plugin/src/esm/cell/mod.rs:1143-1230` has **13**. Missing:
     `worldspace_persistent_cells`, `landscape_texture_sets`, `worldspace_climates`.
     `worldspace_persistent_cells` is not cosmetic — it is where quest actors such
     as Hadvar/Ralof live, and the doc's own reader would conclude persistent
     exterior refs have no home.
  2. **"Not yet parsed" (`:233`)** reads *"SOUN, SNCT, SOPM, MUSC, MUST, ASPC,
     REVB, AECH (audio)"*. Three of the eight are parsed at HEAD:
     `dispatch_misc_stub.rs:100` (`SOUN`, into a typed `SounRecord`),
     `:91` (`MUSC`), `:43` (`ASPC`). SNCT / SOPM / MUST / REVB / AECH are genuinely
     unrouted, so the line is half-right, which is the worst kind.
  3. **Legacy Bridge example (`:284-291`)** does not compile:
     `lo.register(0x00, PluginId::from_filename("FalloutNV.esm"))` against
     `pub fn register(&mut self, slot: u8, filename: &str)` — the argument is a
     `&str`, not a `PluginId`.
  4. **Status line (`:293-294`)** — *"the call site that passes it into
     `parse_esm_with_load_order` for multi-master stacks is in progress"*. That work
     shipped under a different design: `FormIdRemap` + `GlobalSlot`
     (`crates/plugin/src/esm/reader.rs:385-430`, #1554), and `LegacyLoadOrder` was
     never wired in — `crates/plugin/src/lib.rs:30` keeps the whole module
     `pub(crate)` for that reason.
- **Evidence**:
  ```
  docs/engine/plugin-loading.md:233  **Not yet parsed**: SOUN, SNCT, SOPM, MUSC, MUST, ASPC, REVB, AECH (audio).
  docs/engine/plugin-loading.md:286  lo.register(0x00, PluginId::from_filename("FalloutNV.esm"));
  docs/engine/plugin-loading.md:293  **Status:** The bridge type exists; the call site that passes it into
  ```
  ```rust
  // crates/plugin/src/legacy/mod.rs:141-148 — the real signature
      pub fn register(&mut self, slot: u8, filename: &str) {
          assert!(
              slot <= 0xFC,
              "slot 0x{slot:02X} is reserved (0xFD=ESH, 0xFE=ESL, 0xFF=save-generated)"
          );
          self.slots[slot as usize] = Some(PluginId::from_filename(filename));
      }
  ```
- **Impact**: this document is what a contributor reads before touching the tier.
  Item 3 hands them code that will not build; item 4 sends them to re-implement a
  wiring that already exists under another name; item 1 hides the persistent-cell
  map; item 2 will send an audio-subsystem author to write parsers that exist.
  The crate-side rustdoc is accurate throughout — only the engine doc is wrong.
- **Related**: `AUDIT_ESM_2026-08-13.md` ESM-D7-07; #1554.
- **Suggested Fix**: regenerate the `EsmCellIndex` block from the struct, split the
  audio line into parsed-vs-unparsed, fix the `register` example to pass `&str`, and
  replace the Status paragraph with a pointer to `FormIdRemap` / `GlobalSlot` plus
  the `pub(crate)` rationale from `lib.rs:30`.

---
### LOW
