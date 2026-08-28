# #3428: UI-D3-2026-08-27-03: a lifecycle-class scan miss is a hard menu-load failure, while the sibling scan over the same ABC tags is deliberately non-fatal

- **Severity**: MEDIUM
- **Dimension**: AVM2 Adapter Injection
- **Profile**: `Fallout4Avm2`
- **Location**: `crates/ui/src/avm2_host.rs:99-153` (esp. `:127-129` and `:150-151`) vs `crates/ui/src/avm2_host.rs:175-185`
- **Source**: `docs/audits/AUDIT_UI_2026-08-27.md` (UI-D3-2026-08-27-03)

## Description

`inject_host_object_adapter` decides a movie "declares the contract" with a raw byte scan (`contains_bytes(abc, b"BGSCodeObj") && contains_bytes(abc, b"onCodeObjCreate")`, `:99-103`), then requires an `abc.instances` entry declaring *both* as traits. There are two ways that second step can miss on legal ABC, and both are a hard `Err`:

1. `:150-151` — `"Fallout 4 BGSCodeObj lifecycle class was not found"` when the byte scan hit but no instance carries both traits (strings present only as constants / in a different class / declared as **static class** traits, since the loop walks `abc.instances` and never `abc.classes`).
2. `:127-129` — `Reader::new(data).read().map_err(...)?` propagates, so a single ABC tag anywhere in the movie that this reader cannot parse aborts the whole injection.

Every `SwfPlayer` constructor turns that `Err` into a refused menu (`player.rs:174-175`, `:195-196`, `:221-222`).

## Evidence

The identical parse over the identical tags, ~25 lines later, is explicitly non-fatal:

```rust
// crates/ui/src/avm2_host.rs:175-185
let referenced = match referenced_host_methods_in_tags(&movie.tags) {
    Ok(referenced) => referenced,
    Err(error) => {
        log::warn!("Fallout 4 host-call inventory scan failed ({error}); ...");
        BTreeSet::new()
    }
};
```

with the reasoning "refusing to load a menu because its bytecode has a shape the scanner cannot walk would be strictly worse than the pre-fix behaviour" (`:172-174`). The hard-failure behaviour is currently *pinned* by `marker_without_a_lifecycle_class_is_rejected` (`:1746-1770`), which asserts `.unwrap_err()`.

## Impact

This is the same failure shape as #2963/#3149 one level up. There, an over-strict three-trait class predicate made four real shipped FO4 menus (DialogueMenu, MultiActivateMenu, SPECIALMenu, Terminal) fail to load entirely; the fix relaxed the predicate but left the *policy* — miss means refuse — untouched. The `NotPresent` state that exists precisely for "this movie has no host object" is unreachable from either branch. The FO4 base-game corpus (311 movies) passes today, so this is latent, not live: DLC, Creation Club and mod menus are the exposure.

## Related

#2963, #3149, and sibling finding UI-D3-2026-08-27-02.

## Suggested Fix

Degrade both branches to `ScaleformHostObjectState::NotPresent` with a `log::warn!` naming the movie, matching `:175-185`'s policy; keep the hard `Err` only for `patch_root_constructor` failures, where a *partial* rewrite really would hand Ruffle a corrupt SWF. Update `marker_without_a_lifecycle_class_is_rejected` to assert the degraded state rather than the error.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`referenced_host_methods_in_tags`, `patch_root_constructor`, the three `SwfPlayer` constructors)
- [ ] **TESTS**: A regression test pins this specific fix (update `marker_without_a_lifecycle_class_is_rejected`)
