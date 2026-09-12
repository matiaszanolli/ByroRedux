# D5-07: Six of eleven documented template_flags bits are parsed and stored with no consumer

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4137
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11b.md` (HEAD `b3db49fa`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4137 --json state`.

---

Reported by `/audit-character` (second, independent re-verification pass) —
see `docs/audits/AUDIT_CHARACTER_2026-09-11b.md` (HEAD `b3db49fa`).

- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: all (FO3/FNV/FO4/Skyrim — wherever `template_flags` is parsed)
- **Location**: `crates/plugin/src/esm/records/actor/mod.rs:445-448` (doc comment enumerating the bits); `crates/plugin/src/equip.rs:355-372` (`TEMPLATE_FLAG_*` constants — only 4 of the 11 documented bits have a named constant/consumer)

## Description

Six of the eleven documented `template_flags` (ACBS template-inheritance
bitmask) bits are parsed and stored on `NpcRecord` with no consumer
anywhere in the codebase: `0x0008` Actor Effects, `0x0010` AI Data, `0x0040`
Model/Animation, `0x0080` Base Data, `0x0200` Script, `0x0400` Def Pack
List. This is disclosed, not silent — the doc comment on
`NpcRecord::template_flags` names all eleven bits and states the six above
have "no consumer yet" (the comment is slightly stale in still also
listing `0x0004` Factions and `0x0020` AI Packages in that same trailing
sentence — those two gained consumers, `resolve_inherited_factions` /
`resolve_inherited_ai_packages`, in `6bcd1666` this session).

No CHARAL-owned component (`ActorValues`, `CharacterLevel`, `Background`,
`Perks`, `FactionRanks`, `AmbientPackageRuntime`) currently depends on any of
the six remaining bits. `AIDT` (aggression/confidence) is not even parsed
onto `NpcRecord` yet, and the "Use Script" flag's real consumer is a
different subsystem (M47 scripting's VMAD attach path via `cell_loader`, not
the NPC_-record TPLT walk) — so the gap is intentional/deferred rather than
an oversight.

## Evidence

Only four `TEMPLATE_FLAG_*` constants exist in `crates/plugin/src/equip.rs`:

```rust
pub const TEMPLATE_FLAG_USE_TRAITS: u16 = 0x0001;
pub const TEMPLATE_FLAG_USE_STATS: u16 = 0x0002;
pub const TEMPLATE_FLAG_USE_FACTIONS: u16 = 0x0004;
pub const TEMPLATE_FLAG_USE_AI_PACKAGES: u16 = 0x0020;
pub const TEMPLATE_FLAG_USE_INVENTORY: u16 = 0x0100;
```

No named constant or reader exists for `0x0008`, `0x0010`, `0x0040`,
`0x0080`, `0x0200`, or `0x0400` anywhere in `crates/plugin/src/` or
`byroredux/src/`.

## Impact

None currently measurable — no reader exists that could read the wrong
(unresolved) copy of any of these six categories, because no reader exists
at all yet. Filed so the gap stays visible for whoever adds the first
consumer.

## Suggested Fix

No fix needed now. When the first consumer of one of these six categories is
added:
1. Give it a named `TEMPLATE_FLAG_*` constant in `crates/plugin/src/equip.rs`
   alongside the existing four, following the same pattern.
2. Route it through `resolve_inherited_record` with that flag constant from
   the start, rather than reading the shell's own field unconditionally.
3. Per the companion finding on hoisting TPLT resolution (#4133), receive an
   already-resolved record from `spawn_placement_root` rather than resolving
   independently.
4. Update the trailing sentence of the `template_flags` doc comment
   (`crates/plugin/src/esm/records/actor/mod.rs:445-448`) to drop the newly
   consumed bit from the "no consumer yet" list.

## Related

Companion to #4133 (TPLT-resolution hoisting). Not a re-opening of
#4091/#4092/#4093 (Factions/AI Packages), which are correctly fixed.

## Completeness Checks
- [ ] **SIBLING**: When any of the six bits gains a consumer, the doc comment
      at `crates/plugin/src/esm/records/actor/mod.rs:445-448` is updated in
      the same change (not left listing a now-consumed bit as unconsumed,
      the way `0x0004`/`0x0020` currently are)
- [ ] **TESTS**: A regression test pins the new consumer's TPLT-resolution
      behavior once one is added
