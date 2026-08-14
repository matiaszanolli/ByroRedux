# #2664 #2665 #2666 #2667 — scripting-domain audit closeout (2026-08-14)

Four findings from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh
scripting pass). One live correctness bug, three maintenance-shaped.

## #2664 (SCR-D7-NEW11-03) — MEDIUM, live
The worldspace persistent-cell loader open-coded `stamp_quest_reference`'s body
for its logical-actor stubs and inserted **no transform**. `resolve_alias_
bindings` ranks distance-anchored aliases with
`world.get::<GlobalTransform>(entity)?` *inside a `filter_map`*, so those
candidates were dropped from the `min_by` entirely — a Find-Matching /
Unique-Actor alias authored "Closest" whose only candidates are persistent
worldspace `ACHR`s silently never filled.

**Fix**: call the shared `spawn_logical_quest_reference` (now `pub(crate)`),
passing the REFR's own placement through the same conversion the reference
loader uses.

**Tests**: behavioural pin in `byroredux-scripting`
(`quest_alias_closest_fill_needs_a_transform_on_its_only_candidate`, asserting
both directions), plus a source pin on the call site — driving
`PersistentCellApplyJob::apply` needs Vulkan + game data.

**Sibling sweep**: `stamp_quest_reference` is now the only production
constructor of `SceneAliasCandidate` (the other two hits are `#[cfg(test)]`
fixtures in `commands/quest.rs` and `scripting/dialogue.rs`).

## #2665 (SCR-D1-NEW11-01) — LOW, docs
`FunctionInfo::line_numbers` claimed the boolean pass consults it to reject
cross-line merges. It has zero readers workspace-wide, and declining that check
is `decompile::boolean`'s deliberate departure 1 — the docstring advertised a
safety guard that does not exist, on the pass where #2655 showed its absence
silently erased a `While`. `function_type` claimed a `Method` fallback; the
reader maps unknown bytes to `None`.

## #2666 (SCR-D2-NEW11-01) — LOW, latent
`rebuild_expression`'s "verified single match must be consumed" postcondition
was a `debug_assert!` — nothing in release. It spans two independently
maintained traversals (`child_nodes` counts, `child_nodes_mut` substitutes); a
divergence would take the success path with the producer unconsumed, dropping
the statement while its consumer keeps a dangling `::tempN`.

**Fix**: return `ExpressionRebuildFailed`, matching the `>1` arm. Added
`node.rs`'s first tests — a parity check over all 16 `NodeKind` variants, with
an exhaustive `match` so a new variant fails to compile until covered.
Verified the test bites by temporarily dropping `else_if` from the mutable
traversal.

## #2667 (SCR-D3-NEW11-02) — LOW, latent
`collapse`'s two `.expect`s held only because the depth cap fires first: a
nested collapse whose rejoin is an on-stack ancestor removes that block, and
the ancestor's own `collapse` then looks it up. Both are now local declines.
Added a guard for a self-referential operand/rejoin edge (sibling of the #2028
degenerate-shape decline) — which is also what makes the two remaining
`get_mut(&current).expect(…)` uses locally sound. Narrowed the module doc's
termination claim to the iterative loop, and gave `RecursionLimit` a `pass`
discriminant so a boolean-pass overflow stops reporting itself as a
control-flow failure.

## Corpus validation
`pex_corpus_smoke` over `Skyrim - Misc.bsa` + `Fallout4 - Misc.ba2`:
**21900/21901 decompiled (100.0%), 0 panics — byte-identical to the
pre-change baseline**, so the new declines and the fail-closed rebuild cost
nothing on real content. The `#[ignore]`d R5 fidelity gate also passes against
on-disk Skyrim SE data.
