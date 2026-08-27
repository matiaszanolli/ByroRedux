# CHAR-2026-08-20-D5-01: effective_npc_level is a third copy of effective_actor_level carrying the exact .max(1) divergence #3081 declared wrong

State: OPEN
Labels: bug,medium,legacy-compat,gameplay,esm-plugin,character

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `CHAR-2026-08-20-D5-01`
**Severity**: MEDIUM
**Dimension**: 5 — Population Boundary
**Game**: FO3, FNV

## Location

- `crates/plugin/src/esm/records/actor_value_derive.rs:184-190` — `effective_npc_level`
- vs `byroredux/src/npc_spawn.rs:143-149` — `effective_actor_level`
- Call sites: `actor_value_derive.rs:132` (template resolution) and `:229` (the Health curve's level term)

## Description

The two functions are the same logic with one divergent line:

```rust
// byroredux/src/npc_spawn.rs:147   — the documented, tested original
} else { npc.level.max(0) }

// crates/plugin/src/esm/records/actor_value_derive.rs:188 — the copy
} else { npc.level.max(1) as u16 }
```

`b434e4c0` (Aug 17) introduced the copy. `17b94d2e` (Aug 19) fixed **#3081** by deleting
`inventory.rs`'s copy — the second of what were by then three — and resolved the clamp divergence
**in favour of `.max(0)`**. It did not touch this one. The workspace therefore still carries two
copies whose non-multiplier branches disagree, and **the surviving disagreement is the one the fix
explicitly rejected.**

`17b94d2e`'s own commit body settled the question:

> *"`pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier`'s own `negative` case
> already asserts and comments 'Negative levels still clamp to 0 on the non-mult path
> (pre-existing behaviour, preserved)' — `.max(0)` is the deliberate, tested answer, not an
> oversight."*

And `npc_spawn.rs:135-142` spells out why `1` must **not** be forced:

> *"a plain `level` of `0` is not a documented 'record carries none' sentinel — nothing
> distinguishes it from an authored `0`, so forcing it to `1` would be inventing data the record
> never claimed to have."*

## Evidence

`grep -rn "effective_npc_level\|effective_actor_level"` returns both definitions plus 12 call
sites split across them. `git log -S"fn effective_npc_level"` returns exactly one commit:
`b434e4c0`.

**Measured blast radius**, from a direct `ACBS` scan of `FalloutNV.esm`: **3,816** `NPC_` records
carry an `ACBS`; **268** set `PC Level Mult` (the branch both copies agree on); **30** are
non-multiplier with `level ≤ 0` — the divergent set.

For those 30 the two functions disagree by one level, which splits two ways:

1. **The Health term.** `derive_autocalc_actor_values` (`:227-232`) evaluates the curve at level
   `1`, while `stamp_character_components` writes `CharacterLevel { level: 0 }`. The actor's Health
   is **+5 (FNV) / +10 (FO3)** above what its own recorded level implies.
2. **The template tier.** `derive_npc_actor_values:132` passes `effective_npc_level` into
   `resolve_inherited_stats`, and `equip.rs:318-328` uses that number to filter `LVLN` entries
   (`e.level <= actor_level`). `stamp_character_components:176` passes `effective_actor_level`. A
   `Use Stats` shell with an `LVLN` entry at level 1 therefore resolves to a **different source
   record** for its `ActorValues` than for its `CharacterLevel` / `Background` — the numeric
   substrate and the structural component describing the same actor derived from two different
   NPCs.

## Impact

Small in magnitude (0.8 % of FNV actors, ±1 level) and invisible — **no test covers it**, because
#2955's regression test only ever calls the original, which is exactly the failure mode #3081's
commit body called out.

The reason to fix it is not the 30 actors; it is that **the duplication that produced #3081 is
still live, on the hotter of the two paths**, and the next drift will be found the same way.

## Related

- **#3081** — CLOSED; this is the copy the fix missed (an incomplete close, not a regression).
- **#2955** — CLOSED; the original's semantics.
- `CHAR-D5-01` (prior cycle).

## Suggested Fix

Delete `effective_npc_level` and move `effective_actor_level` down into `byroredux_plugin` — it
takes an `NpcRecord` and belongs beside the record, not in the binary — then import it from both
sides. That is the same resolution `17b94d2e` applied to `inventory.rs`.

Pick `.max(0)`, per that commit's own reasoning. Then extend
`pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier` to call the shared function
**through the plugin crate**, so a future copy has something to fail against.

## Completeness Checks
- [ ] **SIBLING**: no fourth copy exists — `grep` for the `acbs_flags & ACBS_PC_LEVEL_MULT` shape across the workspace after the move
- [ ] **CANONICAL-BOUNDARY**: the shared function lives at the ESM-record boundary (`byroredux_plugin`), with the binary importing it rather than re-deriving the rule
- [ ] **TESTS**: `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier` calls the shared function through the plugin crate so a future copy fails the suite
- [ ] **TESTS**: a case pins that the Health term and `CharacterLevel` agree for a non-mult `level == 0` actor

