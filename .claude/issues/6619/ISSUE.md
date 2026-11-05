# PRDR2-2026-11-03-01: `ChangePerk` never raises the rank of a perk the actor already holds (FO3/FNV/FO4 `AddPerk`)

Labels: bug,medium,gameplay,scripting,game:fnv,game:fo3,game:fo4

**Severity**: MEDIUM (wrong gameplay result on shipped content: a scripted second AddPerk is silently lost)
**Dimension**: Gameplay / Scripting
**Source**: found during the perk-entry-points PRD review round 2 (2026-11-03, rev 3), re-verified at HEAD `eba979dfb`
**Status**: NEW

## Location
`crates/scripting/src/fragment/effects/actor_lists.rs:19-40` (`Effect::ChangePerk` apply):

```rust
let owned = perks.get_mut(actor).expect("present or inserted");
if !*add {
    owned.remove(perk);
} else if owned.rank(perk) == 0 {
    owned.set_rank(perk, 1);
}
```

Producers: FO3/FNV ObScript `AddPerk` (`crates/scripting/src/obscript_effects/actor_state.rs:134-140`) and the Papyrus `AddPerk` lowering (`crates/scripting/src/translate/effects/prims/actor_state.rs:214`).

## Description
Adding a perk the actor already holds leaves its rank unchanged. The original engines raise it:
- GECK *AddPerk*: "Increments the rank of the specified perk on an Actor. If the specified Actor doesn't already have the perk, then it will be added with rank 1." (`/mnt/data/src/reference/geck-uesp-wiki/(main)/AddPerk.wiki`)
- Fallout 4 CK *Perk*: each time a perk is added, its rank will increase (`/mnt/data/src/reference/falloutck-uesp-wiki/(main)/Perk.wiki`).
- Skyrim CK *AddPerk - Actor* says nothing about ranks (and "does nothing for NPCs"), so Skyrim's behaviour is unverified.

## Impact
Multi-rank perks granted by script (quest rewards, implants, magazines that grant ranks, mods that level perks by script) stop at rank 1 on FO3/FNV/FO4, and every entry point that reads the rank composes the wrong value.

## Suggested Fix
Increment the rank (capped at the perk's `Num Ranks`) when `add` is true and the perk is already held, for the games whose sources say so. Keep Skyrim's behaviour behind a verified rule; don't guess it. The perk-entry-points PRD (`docs/prd/perk-entry-points.md`, PERK-BODY-001) owns the rank semantics for entry bodies, so cite it, not restate it.

## Completeness Checks
- [ ] **SEMANTICS**: second `AddPerk` on a held perk raises the rank by one, capped at `Num Ranks` (FO3, FNV, FO4)
- [ ] **SKYRIM**: Skyrim's rule settled from a primary source or left unchanged with a comment naming the open question
- [ ] **RANK_CONSUMERS**: entry-point composition reads the new rank (one test through `compose_entry_point`)
- [ ] **TESTS**: unit test on `ChangePerk` apply for add-twice, add-at-cap and remove

