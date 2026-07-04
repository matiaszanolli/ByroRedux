# #1823 — FO4-D2-01: Regression of #1651 — BGSM/BGEM blend-factor 0↔1 swap (ALREADY FIXED, CLOSED)

**Status**: CLOSED 2026-07-02, fixed in commit `27334481` (prior session, before
this fix-issue invocation). Verified against current code: `gl_to_gamebryo_blend`
was renamed to `bgsm_blend_to_gamebryo` and is now an identity function
(`byroredux/src/asset_provider/material.rs:511`), matching the closing
comment's description exactly. No action needed — nothing further to do here.

---

# #1832 — RT-2: TES-family character rig never grounds -> infinite freefall (Oblivion, Skyrim)

**Severity**: MEDIUM · **Domain**: physics/binary (`byroredux-physics` +
`byroredux` character controller)
**Location**: `byroredux/src/systems/character.rs` (M28.5 grounding); ground
probe result from `crates/physics/src/world.rs` (`grounded` field on the move
result, ~line 411/635); written into
`crates/physics/src/components.rs::CharacterController.is_grounded` (~line 107)

Runtime audit (`docs/audits/AUDIT_RUNTIME_2026-07-02.md`, RT-2) telemetry: the
character rig grounds in every Fallout cell (`grounded=true` — FNV, FO3, FO4)
but never grounds in either TES cell (`grounded=false` — Oblivion, Skyrim),
body Y descending unbounded at v=-2000. Body count alone isn't the cause: FO4
has 2081 rapier bodies yet grounds fine; Skyrim has "only" 1575 and never
grounds.

Impact: infinite freefall in TES-family cells. Benign in Oblivion (156
bodies, cheap enough to be silent) but catastrophic in Skyrim (1575 bodies,
`systems_ms=31.97` — the RT-1/#1698 perf collapse, since a continuously
falling body sweeps the whole scene every physics substep).

Related: #1698 (perf half of the same root cause — fixing grounding here
should also close it).

Suggested fix: investigate why the ground probe fails for TES cells —
compare collision-mesh registration in the Oblivion/Skyrim cell-load path
against the Fallout path; verify the ground raycast axis after the Z-up→Y-up
conversion (`crates/nif/src/import/coord.rs`). Likely candidates: floor/
collision-mesh not registered with the Rapier solver for TES cell loads, or a
ground-probe axis mismatch specific to the TES import route.

Completeness checks called out in the issue:
- SIBLING: same grounding path checked across all TES-family cell-load sites (Oblivion interior/exterior, Skyrim interior/exterior)
- TESTS: a regression test pins `is_grounded=true` for a TES cell with valid floor collision within N frames of spawn
