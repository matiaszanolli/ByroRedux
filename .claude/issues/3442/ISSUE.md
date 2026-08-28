# #3442 — SAFE-2026-08-27b-04: #2771's source-scan pin cannot see draw.rs's `(f + 1) % MAX_FRAMES_IN_FLIGHT` — the fence wait every synchronous GPU destroy depends on

- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-27b.md`
- **Severity**: LOW
- **Labels**: `low,safety,renderer,vulkan,sync,bug`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3442

---

From `docs/audits/AUDIT_SAFETY_2026-08-27b.md` (Dimension 5 — Vulkan spec / sync + Dimension 3 — drop ordering).

- **Severity**: LOW (latent; correct at today's `MAX_FRAMES_IN_FLIGHT == 2`, and a bump is `const_assert`-gated)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1626`; guard at `crates/renderer/src/shader_constants.rs:504-529`; contract at `crates/renderer/src/vulkan/sync.rs:8-49`
- **Status**: NEW — residual of #2771 (CLOSED, fixed in `f8eee12a`)

## Description

`f8eee12a` replaced `(f + 1) % N` with the general `(f + N - 1) % N` previous-slot form in `taa.rs`, `svgf.rs` and `restir.rs`, and added `temporal_history_indexing_uses_the_general_previous_slot_form` to keep it that way. The commit message states the pins "cover every file in their class". They do not — a repo-wide sweep finds one production site left:

```rust
// crates/renderer/src/vulkan/context/draw.rs:1624-1637
let prev = (frame + 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT;
self.device.wait_for_fences(
    &[self.frame_sync.in_flight[frame], self.frame_sync.in_flight[prev]],
    true, u64::MAX,
).context("wait_for_fences")?;
```

Two independent reasons the pin misses it: `context/draw.rs` is not in the test's four-file list (`taa.rs`, `svgf.rs`, `restir.rs`, `volumetrics.rs`), **and** the needle is the literal string `"+ 1) % MAX_FRAMES_IN_FLIGHT"`, which cannot match this site's fully-qualified `% super::super::sync::MAX_FRAMES_IN_FLIGHT` spelling. Adding the file to the list would not fix it.

This is also the site with the largest blast radius of the family, because it is not only a temporal-history read. The both-fences wait is what makes "the GPU is idle with respect to every prior submission" true at this point, and three separate synchronous-destroy arguments cite it as their safety premise — `pending_skin_unload_victims` and `pending_morph_unload_victims` (`crates/renderer/src/vulkan/context/skinned_blas_refit.rs`: *"released NOW (post-fence-wait, so no in-flight command buffer still references the output buffer)"*) and the deferred-destroy tick. At `N == 3` the pattern covers 2 of 3 slots and leaves the immediately-previous frame unwaited, at which point those destroys become use-after-free rather than merely aliasing history.

`sync.rs`'s `const _: () = assert!(MAX_FRAMES_IN_FLIGHT == 2, …)` is a real gate and is why this is LOW rather than higher — a bump cannot happen silently. But `sync.rs` enumerates the two remedies that would let it be relaxed, so relaxing it is a contemplated change, which is the exact argument #2771 was accepted on.

## Evidence

`grep -rn "+ 1) % MAX_FRAMES_IN_FLIGHT\|+ 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT" crates/renderer/src byroredux/src` returns the `shader_constants.rs` needle strings, `draw.rs:1626` (above), and `draw.rs:3953` (`self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT`, which is a *next*-slot advance and correct as written).

Sibling doc-rot found in the same read: `crates/renderer/src/vulkan/sync.rs` cites the double-fence wait as living at *`context/draw.rs:108-120`*. That range now holds `camera_frame_deltas`' doc comment; the wait is at `:1624-1638`. The line reference is the one thing a reader follows to check the load-bearing claim.

## Impact

None at `MAX_FRAMES_IN_FLIGHT == 2`. Under a future sync-tier raise, the guard added specifically to prevent this class would pass while the highest-consequence instance of it silently regressed.

## Related

#2771, #870 (`sync.rs`'s `== 2` contract), #282 (the double-fence wait), #1003 / #643 / #2494 (the synchronous-destroy sites that cite it).

## Suggested Fix

Either wait on **all** `MAX_FRAMES_IN_FLIGHT` fences here (`&self.frame_sync.in_flight[..]`), which makes the site N-agnostic and is remedy (b) from `sync.rs` anyway, or rename `prev` to `other_slot` with an explicit "correct only at N == 2, gated by sync.rs" note. Either way, widen the pin's needle to a regex over `\+ 1\) % (?:[\w:]+::)?MAX_FRAMES_IN_FLIGHT` and add `context/draw.rs` to its file list, and refresh `sync.rs`'s stale line reference.

No render-pass / barrier / pipeline-state change is proposed here, per the standing no-speculative-Vulkan-fixes rule.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (`taa.rs`, `svgf.rs`, `restir.rs`, `volumetrics.rs` — the pin's existing list — plus any other qualified `MAX_FRAMES_IN_FLIGHT` spelling)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
