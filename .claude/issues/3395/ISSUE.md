# Issue #3395 — SF-2026-08-27-D3-01: ROADMAP cites #2359 as the live tracker for CDB Phase 2, but #2359 is CLOSED and Phase 2 is unimplemented

Filed: 2026-08-27 by `/audit-publish` from `docs/audits/AUDIT_STARFIELD_2026-08-27.md`

Labels: `low,documentation,doc-rot,game:starfield,legacy-compat`

> Immutable snapshot of the issue as filed (TD10-001 / #1156).
> GitHub is authoritative for current state: `gh issue view 3395 --json state`.

---

Found by `/audit-starfield` — [`docs/audits/AUDIT_STARFIELD_2026-08-27.md`](docs/audits/AUDIT_STARFIELD_2026-08-27.md), Dimension 3 (CDB material database correctness).

- **Severity**: LOW (doc-rot / tracking)
- **Location**: `byroredux/src/asset_provider/material.rs:1106-1125` (the `PresenceOnly` return that is the actual state), `byroredux/src/asset_provider/tests/starfield_mat.rs:148-189` (the invariant test #2359 shipped), and the forward-blocker row in `ROADMAP.md`
- **Status**: NEW — doc-rot only. **The underlying CDB Phase 2 gap is deliberately not re-filed**; this issue is about the tracker, not the feature.

## Description

#2359 was closed **COMPLETED** on 2026-08-19 by `323f0556` ("track the CDB Phase 2 deferral and pin its invariant with a test"). Its deliverable was the *tracking note + invariant test*, not the Phase 2 feature itself.

The forward-blocker chain in `ROADMAP.md` still names #2359 as the live tracker for "CDB → `ImportedMaterial` per-field extraction". The consequence is that the single largest remaining Starfield material gap now has **no open issue tracking it**: a reader following the ROADMAP lands on a CLOSED/COMPLETED issue and reasonably concludes the work shipped.

## Evidence

The code state is unambiguous and self-documenting. `merge_external_material`'s Starfield arm sets exactly one routing flag and returns:

```rust
// byroredux/src/asset_provider/material.rs:1073-1125
if starfield_named_material && provider.has_starfield_cdb() {
    material.is_pbr = true;
    ...
    return MergeOutcome::PresenceOnly;
}
```

and the shipped invariant test pins that this is *still* the state:

```rust
// byroredux/src/asset_provider/tests/starfield_mat.rs:177-188
assert_eq!(outcome, MergeOutcome::PresenceOnly,
    "#2359: the .mat arm resolves the sidecar but must not claim Merged until it actually forwards CDB-authored data");
assert_eq!(mesh.material.textures, MaterialTextureSet::default(),
    "#2359: every MaterialTextureSet role must stay at its default — Phase 1 forwards zero authored texture data from the CDB");
```

Corroborating: production never calls `ComponentDatabaseFile::parse` at all — `discover_starfield_cdbs` (`material.rs:211`) calls only `ComponentDatabaseFile::probe_header`, and `register_starfield_cdb_probe` (`material.rs:631-633`) discards the `CdbHeaderInfo` and increments a `usize` counter. There is no code path from CDB contents to `ImportedMaterial` anywhere in the tree.

Issue state verified directly:

```
$ gh issue view 2359 --json closedAt,stateReason
closedAt=2026-08-19T01:32:22Z reason=COMPLETED
```

## Impact

Documentation/tracking only. No runtime behaviour change. The blast radius is process: the gap is real, correctly test-pinned, and completely unowned by any open issue — so it can silently drop off the milestone plan.

For context on what remains unimplemented (from the ROADMAP row): `crates/sfmaterial` parses the Component Database end-to-end (97 classes / 1,438,780 instances, re-verified this audit), but nothing walks the tree for per-field data. Every Starfield surface therefore renders on NIF-derived, keyword-classified PBR values under the Disney BSDF lobe. The observable signal Phase 2 has shipped is the `.mat` arm returning `MergeOutcome::Merged` once a real CDB lookup supplies data.

## Suggested Fix

Open a fresh CDB Phase 2 issue (or reopen #2359) and repoint the ROADMAP forward-blocker row at it. Zero code change.

## Related

#2359 (CLOSED/COMPLETED), #1289 (Phase 2 origin), #3230 (the CDB gate making the BGSM/BGEM resolver unreachable), #2709 (`MergeOutcome::PresenceOnly` exists precisely to name this state).

## Completeness Checks
- [ ] **SIBLING**: check whether other ROADMAP forward-blocker rows cite closed issues as live trackers
- [ ] **TESTS**: the `#2359` invariant test keeps its explanatory message pointing at whichever issue is authoritative after this fix
