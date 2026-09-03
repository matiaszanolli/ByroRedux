# #3793 — SAVE-D1-2026-08-30-02 (narrowed): the #3488 companion guard documents itself as a tree scan but is a hand-maintained two-string list, and its audit-half claim is scoped to one literal spelling

**Severity**: LOW (narrowed from a contested MEDIUM — see Scope in the original body)
**Location**: `byroredux/src/save_io/round_trip_tests.rs:86-131`
**Source**: `docs/audits/AUDIT_SAVE_2026-08-30.md` (SAVE-D1-2026-08-30-02, narrowed)

`delta_columns_removed_at_runtime_have_a_load_reconciler`'s doc comment claimed it "scans the
tree" for production `world.remove::<T>` sites; the body actually iterated a one-entry
`RECONCILED` list and grepped two fixed strings. A removal via the local
`remove_component::<T>` helper (duplicated in `npc_spawn/ai_package.rs` and `combat.rs`) was
invisible to it — the guard would keep passing the day a delta column was removed through that
idiom on an entity that outlives a cell reload, exactly the #3488 class it exists to catch.

**Explicitly out of scope** (per the issue body): whether the six `ai_package.rs` removals
(`WanderState`/`TravelState`/`Traveled`/`GuardState`/`PatrolState`/`Escorted`) actually need
reconcilers. The audit report itself concluded the additive-only overlay is correct for them
(NPC-only carriers, destroyed and rebuilt by the cell reload) — not re-litigated here.

## Fix implemented

- `byroredux/src/save_io/registry_completeness_tests.rs`: widened `collect_rs_files` and
  `discover_scan_roots` to `pub(super)` so the sibling `round_trip_tests` module can reuse them
  instead of duplicating the walk.
- `byroredux/src/save_io/round_trip_tests.rs`: `delta_columns_removed_at_runtime_have_a_load_reconciler`
  now does a real scan — walks every `discover_scan_roots` directory, strips test code the same
  way the SAVE-D1-12 guard does, and matches BOTH `remove::<T>(` (the `world.remove::<T>(`
  idiom) and `remove_component::<T>(` (the helper) against every `MUTABLE_DELTA_COLUMNS` name.
  `RECONCILED` entries now carry a `RemovalDisposition` (`Reconciler("fn_name")` or
  `NoReconcilerNeeded("reason")`) instead of a bare reconciler-name string, so an exemption is
  *stated* rather than simply absent. The scan is bidirectional: an undocumented removal fails
  loud (the core bug this issue fixes), and a stale `RECONCILED` entry whose removal moved or
  was deleted also fails loud.
- Added the six `ai_package.rs` removals as `NoReconcilerNeeded` entries with the audit report's
  own stated rationale (NPC-only carrier, rebuilt by the cell reload) — recording the
  already-concluded verdict, not asserting a new one.

Verified live: planted a synthetic `world.remove::<ActorValues>(e)` in a throwaway
`byroredux/src/_scratch_removal_probe.rs` (not wired into any module — the guard reads `.rs`
files by path, not via the crate's module tree) and confirmed the guard failed with the correct
message; same for a `remove_component::<RigidBodyData>(world, e)` call via the helper idiom
specifically (the exact spelling the pre-fix guard was blind to). Both probes deleted after
verification.
