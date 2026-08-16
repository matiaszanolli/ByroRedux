# UI-D3-01: four shipped vanilla Fallout 4 menus fail ABC injection outright, and a lifecycle-class scan miss is fatal instead of degrading to "no host object"

**Issue**: #2963
**Severity**: HIGH
**Dimension**: AVM2 Adapter Injection
**Labels**: `high,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 3 — AVM2 Adapter Injection). Profile: `Fallout4Avm2`.

**Location**: `crates/ui/src/avm2_host.rs`:56-99 · `crates/ui/src/player.rs`:206-207 · `crates/ui/src/avm2_host.rs`:1374-1379

## Description

`inject_host_object_adapter` decides a movie declares the Fallout 4 contract with a **byte scan** (`contains_bytes(abc, b"BGSCodeObj") && contains_bytes(abc, b"onCodeObjCreate")`), then looks for a single ABC *instance* carrying all **three** of `BGSCodeObj`, `onCodeObjCreate` and `onCodeObjDestruction` as trait names.

When the byte scan says yes and the three-trait scan says no, the function returns `Err("Fallout 4 BGSCodeObj lifecycle class was not found")`. Every caller propagates that with `?` (`SwfPlayer::new`, `new_with_profile`, `from_resource_provider`), so `UiManager::load_swf*` fails and **the menu is not loaded at all** — a strictly worse outcome than the `ScaleformHostObjectState::NotPresent` path that already exists for movies that do not declare the contract.

This is not hypothetical: the crate's own 311-movie corpus sweep carries the four menus that hit it in a hard-coded exclusion list so the test still passes.

## Evidence

```rust
// crates/ui/src/avm2_host.rs:1374 — the sweep's own admission
const KNOWN_MISSING_ON_DESTROY_TRAIT: &[&str] = &[
    "interface\\dialoguemenu.swf",
    "interface\\multiactivatemenu.swf",
    "interface\\specialmenu.swf",
    "interface\\terminalmenu.swf",
];
```

```rust
// crates/ui/src/avm2_host.rs:94 — the miss is an Err, not a state
let root_abc_index = root_abc_index
    .ok_or_else(|| "Fallout 4 BGSCodeObj lifecycle class was not found".to_string())?;
```

```rust
// crates/ui/src/player.rs:206 — and the Err aborts the whole player build
let (swf_data, host_object_state) = inject_host_object_adapter(&swf_data, catalog)
    .map_err(|error| anyhow!("Failed to prepare Scaleform host object: {error}"))?;
```

Re-running `all_installed_fallout4_swfs_round_trip_through_injection` against the installed archive on 2026-08-16: the four paths still land in `still_missing_on_destroy`. The sweep's own comment asks for "the follow-up issue this comment references" — which did not exist. **This issue is that follow-up.**

## Impact

`dialoguemenu.swf` and `terminalmenu.swf` are core gameplay menus, not decoration. Loading either through `UiManager::load_swf_from_resource_provider` returns `Err` and the engine has no menu at all — no partial menu, no host-object-free fallback, no diagnostic beyond one log line at the call site.

The same shape will hit mod- and DLC-authored menus far more often than vanilla ones, because the three-trait requirement encodes one particular authoring pattern.

## Suggested Fix

Relax the class predicate to require `BGSCodeObj` + `onCodeObjCreate` and treat `onCodeObjDestruction` as optional (skip only the destroy-callback registration when absent). Failing that, make **every** post-`declares_contract` failure return `Ok((original_bytes, ScaleformHostObjectState::NotPresent))` with a `log::warn!`, so a scan miss costs the host object rather than the menu.

Whichever direction is taken, `KNOWN_MISSING_ON_DESTROY_TRAIT` and its exclusion branch should shrink or disappear — the exclusion list is the symptom.

## Related

- UI-D4-01 (the other half of "the FO4 contract model is narrower than the shipped corpus")
- Supersedes the coverage concern in *SAFEUI-03/04*, both since addressed

## Completeness Checks
- [ ] **SIBLING**: The AVM1/`SkyrimAvm1` path checked for the same fatal-vs-degrade shape
- [ ] **DEGRADE**: A contract-scan miss costs the host object, never the whole menu
- [ ] **TESTS**: A regression test loads one of the four excluded menus end-to-end and asserts it yields a player (not an `Err`)
- [ ] **EXCLUSION-LIST**: `KNOWN_MISSING_ON_DESTROY_TRAIT` removed or justified as a narrower thing

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2963 --json state` when live state is needed.*
