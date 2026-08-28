# #3486 — CHAR-2026-08-27b-D6-03: `template_flags` documentation is scoped to "FNV / FO3" and cites one offset, but the bits now gate Skyrim and FO4 actor-value population at three different offsets

**Labels**: documentation, low, game:fo4, game:skyrim, esm-plugin, character, doc-rot
**Filed from**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` via `/audit-publish`

---

**Severity**: LOW
**Dimension**: Coverage, Documentation & Doctrine Drift
**Game**: Skyrim SE, Fallout 4
**Location**: `crates/plugin/src/esm/records/actor/mod.rs:357-375` (`NpcRecord::template_flags`'s doc comment) and `crates/plugin/src/equip.rs:241-249` (`TEMPLATE_FLAG_*`)
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (CHAR-2026-08-27b-D6-03), HEAD `969d81c8`

## Description

Two doc comments scope the template-flag bits to FNV/FO3:

- `equip.rs:241` — *"**FNV / FO3** `NpcRecord::template_flags` bits. Sourced from xEdit `wbDefinitionsFNV.pas`"*
- `actor/mod.rs:357-358` — *"**FNV / FO3** template-inheritance bitmask from `ACBS` (u16 **at offset 22**)"*

Since `7445506c` those same three constants gate `derive_npc_actor_values` for **every** game, so `0x0002` now decides the stats of 3,182 Skyrim `NPC_` records and the `PRPS` / `DNAM` of the FO4 corpus. The stated offset is right for exactly one of the three families the parser handles, and the cited authority (`wbDefinitionsFNV.pas`) covers only that one.

Created by `7445506c` widening the consumers without widening the provenance.

## Evidence

The three ACBS arms in the same file parse `template_flags` at three different offsets:

- FO4 `actor/mod.rs:919-927` — byte **14**
- Skyrim `:939-949` — byte **18**
- FNV/FO3 `:953-975` — byte **22**

`grep -n "template_flags" crates/plugin/src/esm/records/actor/mod.rs` → assignments at `:927` (FO4), `:948` (Skyrim), `:974` (FNV/FO3), each behind a distinct `SubReader` cursor; the doc comment at `:358` names only offset 22. `actor_value_derive.rs:188` is the single call site that now routes all three.

## Impact

Documentation only — the *parse* is per-game correct (three arms, three offsets, each with its own layout comment), and the TES5/FO4 bit meanings for `0x0001` / `0x0002` / `0x0100` do in fact match FNV's. But the one place a reader goes to learn what these bits mean asserts a game scope and a byte offset that are wrong for two of the three families now depending on them, and names no source for those two.

## Related

- #2956 (introduced the constants), #3381 / #3382 (widened the consumers)
- The `Use Traits` / `Use Stats` chain-selection defect filed alongside this one — the substantive consequence of the bits' semantics being under-documented

## Suggested Fix

Re-scope both doc comments to "FNV / FO3 / Skyrim / FO4", drop the single offset in favour of pointing at the three ACBS arms, and add the `wbDefinitionsFO4.pas` / TES5 xEdit citation the FO4 and Skyrim arms already carry for their own layouts.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `NpcRecord` / `ACBS` field doc comments whose scope widened with a consumer, e.g. the `DNAM` baked-stat sentinel)
