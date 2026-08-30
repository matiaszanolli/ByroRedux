# #3581 — REN-2026-08-30-D4-03: `FrameSync::images_in_flight`'s invariant doc cites four `draw.rs` line numbers that are all stale, and its deadlock rationale was inverted by #952

**Labels**: `low,renderer,sync,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3581 --json state`.

---

- **Severity**: LOW
- **Dimension**: Sync/Barriers
- **Location**: `crates/renderer/src/vulkan/sync.rs:93–113` (`FrameSync::images_in_flight` doc comment)
- **Status**: NEW — doc-in-code is wrong, code is right. Distinct from #3442 (which is about `#2771`'s source-scan pin over the `(f + 1) % MAX_FRAMES_IN_FLIGHT` expression, a different file and a different mechanism).
- **Description**: This comment is the only place in the tree that states the
  `images_in_flight` safety invariant and the "if `draw_frame` ever drops to a
  single-slot fence wait, this breaks silently" warning. Two problems:
  1. **All four line citations are dead.** It names
     `context/draw.rs:179-186` (the image-fence read),
     `context/draw.rs:144-156` (the both-slots wait), `draw.rs:180` (the
     aliasing guard) and `draw.rs:191` (the fence reset). Those lines now
     hold unrelated pure helpers — `draw.rs:145` is
     `uses_rigid_motion_history`, `draw.rs:159` is
     `skinned_vertex_address_for_draw`, `draw.rs:195` is
     `skin_slot_backs_mesh`. The real sites are `draw.rs:1624–1636`
     (`wait_for_fences` on `in_flight[frame]` + `in_flight[prev]`),
     `draw.rs:1745–1761` (the `image_fence != in_flight[frame]` guard and the
     `images_in_flight[img] = in_flight[frame]` store), and `draw.rs:3811`
     (`reset_fences`).
  2. **The stated reason for the aliasing guard no longer holds.** The doc
     says: "Reusing the slot's own fence would block on an UNSIGNALED handle
     (it's reset at `draw.rs:191`) and deadlock." `#952 / REN-D1-NEW-04` moved
     `reset_fences` out of that position to immediately before `queue_submit`
     — `draw.rs:1763` carries the comment recording the move, and the call
     itself is at `draw.rs:3811`. At the guard site the slot's own fence is
     therefore still SIGNALED (it was waited on at `draw.rs:1624`), so waiting
     on it would return immediately, not deadlock. The guard is still correct
     and worth keeping; its documented justification is simply no longer the
     true one.
- **Evidence**: `grep -n "draw.rs:179\|draw.rs:144\|draw.rs:180\|draw.rs:191"
  crates/renderer/src/vulkan/sync.rs` → lines 95, 97, 102, 105; compare
  against `sed -n '140,200p' crates/renderer/src/vulkan/context/draw.rs` and
  `grep -n "reset_fences" crates/renderer/src/vulkan/context/draw.rs`
  (`1763` comment, `3811` call).
- **Impact**: A maintainer following this comment to check the invariant
  before, say, narrowing the both-slots wait to one slot lands in the middle
  of `draw.rs`'s pure-function preamble and gets a rationale that contradicts
  the code. The invariant itself is intact and correctly upheld today.
- **Needs RenderDoc**: no
- **Suggested Fix**: Replace the four citations with symbol-anchored prose
  (`draw_frame`'s both-slots `wait_for_fences`; the `image_fence !=
  in_flight[frame]` guard; the pre-`queue_submit` `reset_fences`) rather than
  line numbers, and restate the guard's purpose post-#952. Consider a
  source-scan pin in the sibling style of the existing
  `dependency_chain_tests` in `egui_pass.rs`. No behavioural change.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D4-03

## Dedup cross-reference

Distinct from **#3442**, which is about `#2771`'s source-scan pin over `draw.rs`'s
`(f + 1) % MAX_FRAMES_IN_FLIGHT` expression — a different file and a different mechanism.


## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
