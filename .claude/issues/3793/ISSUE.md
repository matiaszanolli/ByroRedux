# #3793 — SAVE-D1-2026-08-30-02 (narrowed): the #3488 companion guard documents itself as a tree scan but is a hand-maintained two-string list

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, save-load, test-gap, bug

---

**Audit**: `/audit-save` — `docs/audits/AUDIT_SAVE_2026-08-30.md` (Dimension 1 — Snapshot Completeness & Determinism), HEAD `64f64480`
**Finding ID**: `SAVE-D1-2026-08-30-02` (**narrowed** — see Scope)

- **Severity**: LOW
- **Status**: NEW
- **Data-Loss Class**: none today — this is a tripwire-integrity defect

## Scope — what this issue is and is not

The audit report filed this as MEDIUM on the premise that *"six of the seven production delta-column removal sites go through a `remove_component::<T>` helper the guard cannot see"*, implying those six need reconcilers.

**That wider premise was contested by a sibling `/audit-regression` pass and is deliberately NOT asserted here.** This issue is narrowed to the part that is verifiable from the guard body alone and is not in dispute: **`delta_columns_removed_at_runtime_have_a_load_reconciler` documents itself as a tree scan and is in fact a hand-maintained two-string list.**

Whether any of the `npc_spawn/ai_package.rs` removals additionally warrants a reconciler is left open and is **not** claimed by this issue — the report itself concluded the additive-only overlay is *correct* for them, since NPCs are destroyed and rebuilt by the cell reload and the one entity that outlives a reload (the process-lifetime player body) never carries them.

## Location

`byroredux/src/save_io/round_trip_tests.rs:86-131` — the guard and its doc comment

## Description

`delta_columns_removed_at_runtime_have_a_load_reconciler` is the tripwire meant to stop #3488 recurring. Its doc comment states the mechanism:

> Rust has no reflection for "which components does this crate remove", so this **scans the tree** for production `world.remove::<T>` sites the same way the sibling above pins the column set: by hand-audited list. **Adding one makes this fail** and forces the maintainer to write the reconciler.

**It does not scan the tree.** The body iterates a one-entry `RECONCILED` list and greps two fixed strings:

```rust
let save_io = include_str!("../save_io.rs");
for (column, reconciler) in RECONCILED { … assert!(save_io.contains(reconciler), …) }
let inventory = include_str!("../inventory.rs");
assert!(inventory.contains("world.remove::<EquippedWeapon>(player)"), …);
```

Adding a second production removal site does not make it fail; nothing enumerates removals at all.

Its audit-half comment is additionally **spelling-scoped**:

> The audit half: `EquippedWeapon` must still be the only delta column a production path removes. `Dead` is removed nowhere; every other `world.remove::<T>` in the tree is inside a `#[cfg(test)]` module.

That sentence is a claim about the literal token `world.remove::<T>`. Removal through any other spelling is invisible to it — for instance the local helper `remove_component::<T>` (`npc_spawn/ai_package.rs:428`, duplicated at `combat.rs:404`), whose body is `world.query_mut::<T>()` + `query.remove(actor)` and therefore contains neither `world.remove::<` nor `remove::<`.

## Evidence

The guard body quoted above is complete — re-read at HEAD, `round_trip_tests.rs:86-131`.

`grep -rn "fn remove_component"` returns `npc_spawn/ai_package.rs:428`, `combat.rs:404`, and one test in `world_tests.rs`. The helper's body:

```rust
fn remove_component<T: Component>(world: &World, actor: EntityId) {
    if let Some(mut query) = world.query_mut::<T>() {
        query.remove(actor);
    }
}
```

## Impact

Latent today. What is broken is the **tripwire**: the sole automated defence against the class that produced a HIGH one cycle ago (#3488) is blind to any removal idiom other than the one literal token it greps, and its own doc comment tells a maintainer it scans the tree. Green-ness of this test is currently cited as evidence of coverage.

The day a delta column is removed through a helper — or through any spelling other than `world.remove::<T>` — on an entity that outlives a cell reload, #3488 recurs with the guard still passing.

## Suggested Fix

Make the guard do what it says: walk the same file set the SAVE-D1-12 guard already walks (it has `collect_rs_files` and the `#[cfg(test)]`/`*_tests.rs` stripping ready to reuse), match `remove::<T>` **and** any local removal helper, and assert every hit whose `T` is in `MUTABLE_DELTA_COLUMNS` appears in `RECONCILED` — with an explicit reason string permitted in place of a reconciler name ("no reconciler needed — cell reload rebuilds the carrier"), so an exemption is *stated* instead of invisible.

Failing that, at minimum correct the doc comment and the audit-half comment: they assert a mechanism and a fact about the tree that the code does not implement.

## Related

- #3488 (the HIGH this guard was written for)
- #1847 / SAVE-04 (the additive-only overlay contract)
- `delta_columns_carry_only_session_stable_fields` (the sibling tripwire, which *does* pin its set by equality and is not affected)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the other source-scan guards in `round_trip_tests.rs`, for docstrings promising a scan they do not perform
- [ ] **TESTS**: A regression test pins this specific fix — adding a synthetic production removal of a delta column must make the guard fail
