# CHAR-2026-09-11-D5-02: `Use Factions` and `Use AI Packages` are parsed, have live consumers, and are never resolved — the shell's own empty list wins

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4093
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4093 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: MEDIUM
- **Dimension**: 5 — Population boundary
- **Game**: all (FNV/FO3/Skyrim/FO4 all parse `template_flags`)
- **Location**: `byroredux/src/npc_spawn.rs:76-84` (`stamp_faction_ranks`); `byroredux/src/npc_spawn/ai_package.rs:525`, `:537`, `:553`
- **Source**: `crates/plugin/src/esm/records/actor/mod.rs:443-447` — `0x0004` Factions and `0x0020` AI Packages are enumerated as parsed template-inheritance bits with "no consumer yet"; `crates/plugin/src/equip.rs:471-472` — `resolve_inherited_record`'s `flag` parameter is already category-agnostic ("`flag` only gates *whether* to keep following the chain, not how").

## Description

the spawn tail honours 3 of the 12 parsed flags (`Use Traits`, `Use Stats`, `Use Inventory`). `stamp_faction_ranks` reads `npc.factions` straight off the shell and returns early when it is empty; `apply_ai_package_behavior` reads `npc.ai_packages` the same way. A `Lvl*` shell that sets `Use Factions` / `Use AI Packages` carries an empty `SNAM` / `PKID` list *by design* — that is what the flag means — so both stamps silently no-op for exactly the actors the flag exists to serve. The `actor/mod.rs` doc's "no consumer yet" is the premise that made this look benign; it describes the *flag* having no consumer, but the underlying **fields** each have a live one.

## Impact

faction-gated dialogue, quest aliases and AI-package selection evaluate a
  structural "not in faction" for templated actors — the same failure shape #3158
  described for `HasPerk` on Skyrim ("every `HasPerk` CTDA evaluated a structural 0.0").
  For AI packages the actor simply gets no ambient behaviour. Population size is
  **unmeasured** — a census needs a full-ESM parse, which this pass is forbidden to run —
  so MEDIUM, not HIGH.

## Related

#2956 / #3381 / #3382 / #3480 are the same defect resolved for three other flags; `CHAR-2026-09-11-D1-01` and D5-01 are siblings.

## Suggested Fix

add `resolve_inherited_factions` / `resolve_inherited_packages` wrappers over the existing `resolve_inherited_record` (it already takes the flag as a parameter, so this is two three-line fns) and call them from `stamp_faction_ranks` and `apply_ai_package_behavior`. Measure first with a census example so the fix ships with a number.

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix