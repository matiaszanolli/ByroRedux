# #2369 — EX-14/15: Stream ground cover, persistent refs, parent worlds, FO4 spatial data

Multi-criterion plan issue (EX-14 + EX-15), not a scoped bug. Same shape
as #2372 (EX-16) and part of the #2377 epic-of-epics.

## GitHub-state note (important, read before trusting `state`)
Accidentally auto-closed **twice** this session by unrelated commits whose
messages happened to contain a closing keyword immediately before `#2369`
— once in a commit subject (2026-08-26), once in a commit body quoting
that exact phrase as a past-tense example while explaining the first
incident (2026-08-31). Reopened both times. Memory note filed
(`feedback_multi_issue_commit_close.md`) — no closing keyword will be
placed near this issue's number in any commit message from here on unless
the commit genuinely closes it.

## Acceptance criteria — verified status (2026-08-31)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | GRAS/REGN placement + full SpeedTree replace billboard-only coverage | **NOT DONE** | GRAS still routes through `parse_minimal_esm_record` (`dispatch_misc_stub.rs`), every field discarded. SpeedTree import still always emits one placeholder billboard quad (`crates/spt/src/import/mod.rs`). `docs/engine/exal-trees.md` is PROPOSED, zero Phase-2.1 geometry-tail code landed. |
| 2 | Density deterministic/spatially stable/streamed/unloaded/deadline-budgeted | **NOT DONE** | No ground-cover density/placement code exists. `exterior-readiness-plan.md` item A.1-A.7 all unchecked — blocked on a bench-hold measurement step, and the density field lives in GLSL (untestable by `cargo test`). |
| 3 | Persistent/temp ref ownership correct across parent worlds + boundary crossings | **PARTIAL** | `resolve_persistent_cell`/`persistent_cell_identity_unchanged`/`persistent_root_survives_crossing` (`cell_loader/exterior.rs`) land the *identity-skip* optimization (avoid redundant despawn/respawn when the persistent CELL is unchanged). No code distinguishes "temporary ref" ownership across a boundary at all. Full live-state snapshot/restore is #3299 (open, now unblocked). |
| 4 | FO4 precombine/previs/occlusion: render/collision/fallback/mod-invalidation | **PARTIAL** | Render + fallback + mod-invalidation: done (`PrecombinedSpawnJob` + BSCRC32 blob routing + `absorbed_refs_or_empty`, measured against real installed FO4 — 94 re-baked + 416 correctly-invalidated overrides). Collision: not implemented (Havok `BhkSystemBinary` blob undecoded, no decoder exists). Previs/occlusion: not implemented (only the `.uvd` outer header is cracked, zero parser/consumer). |
| 5 | No double geometry (absorbed refs vs. precombined meshes) | **DONE** | `absorbed_refs_or_empty` (`cell_loader/precombined.rs`), shared interior/exterior, unchanged since landing, no open follow-up. |
| 6 | Soak telemetry proves clean unload | **PARTIAL** | Real infra exists (`OwnershipTracker`, `precombine_mesh_rows` Exact-reclaim class, `m-exteriors.sh soak` mode) and covers precombine meshes with a measured 5-cycle FNV/FO4 pass. No ground-cover class (nothing to track yet), no parent-worldspace-crossing-specific class, no previs/occlusion class — coverage is honest about what exists, not complete for this issue's full scope. |

**1 of 6 fully done** (criterion 5). Two (4, 6) are real-but-partial with
genuine shipped substance. Three (1, 2, 3) have substantial unstarted or
half-started work, two of which (1, 2) are blocked on measurement/research
steps this project's conventions require before implementation.

## Duplicate check (done BEFORE proposing any split, unlike last time)
Searched for existing open issues on: ground cover / GRAS / SpeedTree,
persistent ref / parent world, "EX-14"/"EX-15" in title or body. Only
hits are #2369 itself and #2377 (the parent epic) — **no pre-existing
sub-issues to duplicate**, unlike #2372's split (which collided with
#3299/#3301).

## Decision
Asking the user how to proceed (matching #2372's precedent: split into
scoped sub-issues per real-gap criterion, vs. leave open with a corrected
status comment, vs. attempt one narrow tractable slice now).
