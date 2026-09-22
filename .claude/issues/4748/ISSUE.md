# #4748: SAVE-D1-2026-09-22-02: Saved resources (unlike component columns) serialize HashMap/HashSet fields in hash-iteration order — the reproducible-CRC doc claim is unqualified

**Labels**: medium, save-load, gameplay, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4748

## Description

`crates/save/src/snapshot.rs`'s `Snapshot` doc comment claims: *"`BTreeMap` keeps the JSON output deterministic (stable diffs, reproducible CRCs across runs at equal state)."* `docs/engine/save-load-roundtrip.md` repeats the determinism claim ("rows are sorted by entity id first for a reproducible CRC").

Both statements are true for **component columns** — `Snapshot.components`/`.resources` are `BTreeMap`s (column keys sorted) and component rows are sorted by entity id at write time (the `#1708` fix, `registry.rs:130,288`). They are **not** true for **resources**: each resource's own `Serialize` impl is whatever `#[derive(Serialize)]` produces, and for any `HashMap`/`HashSet`-backed resource that is Rust's per-process-randomized hash-iteration order, not a content-sorted one. Two saves of bit-identical game state can therefore differ byte-for-byte — and in CRC32 — purely from hash-table iteration order, with nothing in the doc comment disclaiming this.

Five saved resources currently have this shape, two of them added this same audit window (`ReferenceLockState`, `PersistentReferenceStates`), growing the affected set from 3 to 5:

- `Globals(HashMap<u32, f32>)` — `crates/scripting/src/globals.rs:22`
- `QuestStageState { quests: HashMap<QuestFormId, QuestStageData>, .. }` — `crates/scripting/src/quest_stages.rs:76`
- `ReferenceEnableState { disabled: HashSet<u32> }` — `crates/scripting/src/fragment/state.rs:17`
- `ReferenceLockState { overrides: HashMap<u32, LockOverride> }` — `crates/scripting/src/fragment/state.rs:66`
- `PersistentReferenceStates { rows: HashMap<FormIdPair, ReferenceState> }` — `byroredux/src/cell_loader/reference_state.rs:41-44`, serialized via a custom `pair_rows::serialize` that does `rows.iter().collect::<Vec<_>>().serialize(serializer)` — a direct hash-iteration-order dump, with no sort step.

## Evidence

`crates/save/src/snapshot.rs:227-229`:
```rust
/// Columns are keyed by the stable registry name. `BTreeMap` keeps the
/// JSON output deterministic (stable diffs, reproducible CRCs across
/// runs at equal state).
```

`docs/engine/save-load-roundtrip.md:59`: "rows are sorted by entity id first for a reproducible CRC" — scoped to component rows in context, but doesn't disclaim resources, and reads as a blanket claim.

`byroredux/src/cell_loader/reference_state.rs:72-84`:
```rust
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct PersistentReferenceStates {
    #[serde(with = "pair_rows")]
    rows: HashMap<FormIdPair, ReferenceState>,
}
...
mod pair_rows {
    ...
    pub(super) fn serialize<S: serde::Serializer>(
        rows: &HashMap<FormIdPair, ReferenceState>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        rows.iter().collect::<Vec<_>>().serialize(serializer)
    }
    ...
}
```
No sort of the collected `Vec` before serializing — confirmed by direct read at HEAD `c3f298a24`.

`Globals`, `QuestStageState`, `ReferenceEnableState`, `ReferenceLockState` all confirmed at their cited lines to carry a plain `#[derive(Serialize, Deserialize)]` over a `HashMap`/`HashSet` field, no custom ordered serializer.

## Impact

No data loss — `decode`'s CRC check still correctly catches genuine corruption/truncation of whatever bytes were actually written, and nothing in the codebase currently diffs, dedups, or content-hashes whole save files (`SaveRing`/`slots_by_recency` key off mtime and slot number, not content hash). The risk is precedent-quality: the doc comment states a false invariant that a future feature (save-sync, corruption telemetry, save-dedup-on-disk) could rely on without re-checking, and the affected resource set is growing (3 → 5 this window alone, both new additions again `HashMap`-backed).

## Related

`#1708` (closed) — fixed the same general claim at the component-row level only (entity-id sort); does not touch resources. Not a duplicate of `#4705` (GAME-D7-2026-09-21-02 / SAVE-D1-2026-09-22-01), which is a *different* mechanism (the registry-completeness guard's `#[cfg(test)]`-truncation blindness) — that finding is about whether a type is save-registered at all, this one is about serialization order for types that already are.

## Suggested Fix

Narrow `snapshot.rs`'s doc claim to "component columns" explicitly (cheapest, and matches what's actually true today), or sort each affected resource's map/set at its own `Serialize` boundary (a `BTreeMap`/sorted-`Vec` shim per resource, mirroring `PersistentReferenceStates::pair_rows`'s existing custom row serializer, with a sort added). No test currently exercises this; a same-state-two-runs-equal-CRC test would need a controlled hasher to be meaningful and is probably not worth adding unless a consumer of the doc claim actually materializes.

Source: docs/audits/AUDIT_SAVE_2026-09-22.md (SAVE-D1-2026-09-22-02)

## Completeness Checks
- [ ] **UNSAFE**: N/A — no `unsafe` involved
- [ ] **SIBLING**: Check every other `HashMap`/`HashSet`-backed saved resource, not just the five named here, if new ones are added later
- [ ] **DROP**: N/A — no Vulkan objects involved
- [ ] **LOCK_ORDER**: N/A — no `RwLock` scope change
- [ ] **FFI**: N/A — no cxx bridge involved
- [ ] **CANONICAL-BOUNDARY**: N/A — not a NIFAL/material translation change
- [ ] **TESTS**: A regression test pins the fix (either the narrowed doc claim, or a same-state-two-runs-equal-serialization test with a fixed hasher)
