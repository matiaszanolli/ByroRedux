# TD6-2026-10-19-02: eight typed ESM record maps (+3 side maps) are parsed and retained on every load but have zero readers anywhere in the workspace; `EsmIndex` docs describe them in the present tense

Labels: low,esm-plugin,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: LOW
- **Dimension**: 6 Stub & Placeholder Implementations
- **Location**: `crates/plugin/src/esm/records/index.rs:406-412` (`enchantments` ENCH), `:497-531` (`reputations` REPU "Drives the faction-reputation system and quest gating", `explosions` EXPL, `combat_styles` CSTY "per-NPC AI behavior profile", `impacts` IPCT, `impact_data_sets` IPDS, `recipes` COBJ), `body_parts` BPTD; side maps `navi_info`, `object_mod_loose_items`, `deleted_record_metadata`. Insert sites only: `dispatch_misc_gameplay_b.rs:32,149-183`, `dispatch_misc_gameplay_a.rs:94`, `dispatch_items.rs:85`, `parse.rs:460`.
- **Age**: `089ba022cf`, 2026-05-16 (#808/#809; 5 months); ENCH from #629
- **Effort**: small (doc annotation) / large (consumers)
- **Status**: NEW (same family as closed #4376 TD6-009 / #4214; those covered SPEL/MGEF/LVSP/IMGS only)
- **Description**: `rg '\.(enchantments|reputations|body_parts|recipes|combat_styles|explosions|impact_data_sets|impacts)\b'` over crates/byroredux/tools, minus tests and `dispatch_*`, returns nothing (the only non-test mention is a comment at `actor/faction.rs:180`; `explosions` is read only by `crates/plugin/examples/probe_form.rs:45`). The typed parsers (`parse_expl`, `parse_csty`, `parse_ipds`, `parse_cobj`, `parse_bptd`, `parse_repu`) run on every ESM load for FNV/FO3/Skyrim/FO4 and the maps live for the session. `docs/engine/esm-records.md:408-435` lists them as wired maps; only the 29 EDID-only stubs get the "no consumer" caveat (`:440`, `memory-budget.md:124`). Weapon enchantments (ENCH) resolve nowhere at runtime. ROADMAP M24 line lists PACK/QUST/DIAL/MESG/PERK/SPEL/MGEF as "fully parsed"; none of these eight is claimed wired there, so no roadmap lie, only a doc-honesty gap.
- **Evidence**: grep above; `EsmIndex` field docs at `index.rs:509-531` ("Drives ...", "per-NPC AI behavior profile") with no consumer caveat.
- **Proposed fix**: Add a "parsed, no runtime consumer yet" sentence per field in `index.rs` and a typed-but-unconsumed sub-list in `esm-records.md` (mirroring the stub caveat), so a consumer author and the next audit see it. Do not delete the parsers (they back `probe_form`/census tooling and cross-ref resolution). Optionally track REPU/CSTY as the next CHARAL / AI consumers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
