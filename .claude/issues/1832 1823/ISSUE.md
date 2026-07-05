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

---

## Update 2026-07-05 — partial fix landed (commit `ae083d69`), issue stays OPEN

Root-caused and fixed one real, confirmed contributor via live A/B testing
(Skyrim SE WhiterunBanneredMare + WhiterunJorrvaskr vs. FNV
GSProspectorSaloonInterior for contrast).

**Confirmed root cause #1**: Skyrim architecture (walls/floor/roof — large
TriMesh shapes, e.g. 256×10×256 floor tiles) ships a Havok
`bhkRigidBody.motionType` raw value in the SPHERE/BOX_INERTIA family (2-5,
decodes to `MotionType::Dynamic` per the existing enum mapping) paired
with `mass=0`. `crates/nif/src/import/collision.rs::extract_from_classic`
mapped this literally to a genuine Rapier `Dynamic` body, spawned asleep
(the exterior-freeze perf fix in `crates/physics/src/sync.rs`), which
free-falls the instant the player's KCC wakes it by standing on it.
Havok's own runtime treats zero-mass "dynamic" bodies as immovable world
geometry (F=ma undefined at m=0); Rapier doesn't replicate that special
case. Fixed by reclassifying `Dynamic`-per-enum + `mass<=0` as `Static`.
Confirmed this doesn't affect real movable clutter (non-zero-mass bodies
are untouched — 101 of 240 census'd bodies in Bannered Mare keep real
masses 0.2-84 and correctly stay Dynamic). Two regression tests added.

Havok scale (`havok_scale_for`) was ruled out as the TES-vs-Fallout
differentiator earlier in this same investigation — Oblivion shares
×7.0 with FO3/FNV (which ground fine); Skyrim shares ×69.99125 with FO4
(which also grounds fine, via a completely different path — the NP-collision
stub + render-geometry trimesh fallback, since FO4 never reaches the
classic-bhk path this fix touches for most content).

**Live-verified impact**: static collider count jumped 19→416 (Bannered
Mare) and 105→431 (Jorrvaskr) — confirms the fix is doing real work, not
just passing a synthetic unit test.

**Still open — NOT fixed**: even after this fix, the character still
free-falls completely at the door-based spawn point in both cells tested.
This looks like a **separate, second issue**:
- Bannered Mare's spawn door (first `DoorTeleport` found in load order)
  resolves its XTEL destination to the *exterior* Whiterun worldspace
  (Tamriel-worldspace coordinates ~25669,-7632) — not loaded in an
  interior-only `--cell` invocation, so the threshold may genuinely have
  no interior floor on our side of the loaded content. This may be a test-
  harness artifact rather than a real in-game bug (a live game always has
  the adjoining worldspace loaded).
- Jorrvaskr's spawn door is a cleaner interior-only test (no exterior
  confound) and *still* free-falls after the fix — this rules out "just
  an exterior-boundary test artifact" as the full explanation.
- A pre-existing code comment in `crates/physics/src/world.rs::move_character`
  documents, specifically for Bannered Mare: "floor planks have ~1-2 BU
  vertex-gaps where adjacent collision triangles meet" — a KCC
  tunneling/seam issue independent of collision-authoring classification.
  `kcc_offset_bu` was already bumped 0.5→4 BU to mitigate; may not be
  sufficient once the character reaches terminal velocity (-2000 U/s) if
  the physics substep travels far enough per tick to jump clean through a
  thin floor before any query registers contact.

**Next step**: fresh investigation specifically into why the door-adjacent
spawn column has no ground contact even with correct Static classification
now confirmed nearby — likely a spawn-point/inward-nudge distance problem
or a substep/CCD tunneling problem, not a collision-import problem. Not yet
scoped as its own tracked issue.
