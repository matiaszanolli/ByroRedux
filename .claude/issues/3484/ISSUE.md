# #3484 — CHAR-2026-08-27b-D6-01: `docs/feature-matrix.md` still records Skyrim NPC population as "Health only" four days after Magicka and Stamina landed

**Labels**: documentation, low, game:skyrim, character, doc-rot
**Filed from**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` via `/audit-publish`

---

**Severity**: LOW
**Dimension**: Coverage, Documentation & Doctrine Drift
**Game**: Skyrim SE
**Location**: `docs/feature-matrix.md:251` (matrix cell) and `:265-268` (prose)
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (CHAR-2026-08-27b-D6-01), HEAD `969d81c8`

## Description

The per-game matrix row "NPC actor-value population at spawn" gives Skyrim `~ Health only`, and the prose a few paragraphs down repeats it: *"Skyrim's NPC population derives Health only (`race.starting_health + NPC_.ACBS.health_offset`) — no skills or other actor values."*

Both are stale. `derive_skyrim_actor_values` (`crates/plugin/src/esm/records/actor_value_derive.rs:201-220`) loops `[("Health", …), ("Magicka", …), ("Stamina", …)]` — Magicka and Stamina are each resolved independently from their own `RACE.DATA` starting value plus their own signed `ACBS` offset, and the AVIF FormIDs (`AVMagicka 0x3E9`, `AVStamina 0x3EA`) were confirmed against the shipped master. That landed in `1d0c5d4b` (2026-08-24) and was verified against `Skyrim.esm` by `AUDIT_CHARACTER_2026-08-24.md` (rows 23–25 of its table).

The section itself is #2961's fix; this is fresh rot inside it.

## Evidence

`docs/feature-matrix.md:251`:

```
| NPC actor-value population at spawn | ✗ | ✓ class auto-calc | ✓ class auto-calc | ~ Health only | ✓ stored `PRPS`+`DNAM` | ~ stored, unverified | ~ stored, unverified |
```

versus the three-element array at `actor_value_derive.rs:206-210`:

```rust
for (name, starting, offset) in [
    ("Health", race.starting_health, npc.health_offset),
    ("Magicka", race.starting_magicka, npc.magicka_offset),
    ("Stamina", race.starting_stamina, npc.stamina_offset),
] {
```

`git log --oneline -1 -- crates/plugin/src/esm/records/actor_value_derive.rs` confirms the file has changed twice since (`9e44a0dd`, `7445506c`) with no matching matrix update.

## Impact

The document `_audit-common.md` designates as the living "what works at runtime per game" reference under-reports shipped Skyrim capability by two of three pools, in the row a milestone-planning read would land on. Two full audits verified the code and neither cross-checked the matrix — the precise cross-check this dimension exists for.

## Related

- #2961 (created the section), #3219 (the parse half)

## Suggested Fix

Update the cell to `~ Health/Magicka/Stamina` and rewrite the paragraph to state the per-pool independent resolution (a race missing `starting_magicka` degrades one pool, not the NPC).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other per-game rows of the same matrix section, and `docs/engine/charal.md`'s coverage table)
