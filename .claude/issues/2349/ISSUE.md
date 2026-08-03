# REG-2026-08-03-01: post_passes.rs split (#2258) reintroduces undocumented unsafe blocks — regression of #2131 / #1904

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2349
**Severity**: MEDIUM
**Labels**: medium, renderer, safety, bug
**Source audit**: docs/audits/AUDIT_REGRESSION_2026-08-03.md (finding REG-2026-08-03-01)
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs:254,311,417,620,663,730,768,796,864`

## Description

#1904 (closed) flagged ~134 renderer `unsafe {}` blocks with no `SAFETY:` comment and recommended `#![deny(clippy::undocumented_unsafe_blocks)]` (now live at `crates/renderer/src/lib.rs:21`). #2131 (closed) was a first regression of that fix — 30 blocks went undocumented again — closed by commit `b0d331af`, which added per-block `// SAFETY:` comments.

Today's commit `7bb517b2` ("Fix #2258: split record_post_passes into one helper per GPU pass") split the 556-LOC `record_post_passes` into nine `record_<pass>_pass` helpers, moving the unsafe FFI calls verbatim into each helper — but instead of preceding each `unsafe {` with its own `// SAFETY:` line, the commit put a `# Safety` doc comment on the enclosing **safe** function. `clippy::undocumented_unsafe_blocks` does not accept a function-level doc comment as satisfying the lint for a block inside a safe fn (that convention only applies to `unsafe fn`); it requires a comment on the line(s) immediately preceding the `unsafe {` itself.

Only one of the ten new/moved blocks (`post_passes.rs:119`, the depth-history barrier helper) got an inline `// SAFETY:` comment — the other nine (in `record_svgf_pass`, `record_caustic_splat_pass`, the volumetrics helper, `record_taa_pass`, `record_ssao_pass`, `record_bloom_pass`, `record_composite_pass`, `record_upscale_pass`, `record_presentation_pass`) did not.

Third occurrence of this discipline regressing: #1904 → #2131 → this.

## Evidence (re-verified directly against current code, 2026-08-03)

```
$ cargo clippy -p byroredux-renderer --lib -- -D warnings 2>&1 | grep -c "unsafe block missing a safety comment"
9
```

Exact sites match the audit report:
```
post_passes.rs:254, 311, 417, 620, 663, 730, 768, 796, 864
```

## Impact

`cargo clippy -p byroredux-renderer --lib -- -D warnings` fails with 9 `undocumented_unsafe_blocks` errors (plus 5 unrelated pre-existing `doc_lazy_continuation` errors in `water.rs`). Any CI job gating on `cargo clippy --workspace -- -D warnings` is red. Not a runtime-safety issue, but the third regression of this discipline in three sibling refactors.

## Related

Regression of #2131 (itself a regression of #1904), both CLOSED (confirmed via `gh issue view`). Sibling: #2350 (same-day `redundant_closure` break of the same CI gate, different lint/site).

## Suggested Fix

Add a one-line `// SAFETY:` comment immediately before each of the nine `unsafe {` blocks in `post_passes.rs`. Consider a dedicated CI step for `cargo clippy -p byroredux-renderer -- -D clippy::undocumented_unsafe_blocks` given this is a 3rd occurrence.

## Completeness Checks
- [ ] **UNSAFE**: Each of the 9 unsafe blocks needs a `// SAFETY:` comment stating the upheld invariant
- [ ] **SIBLING**: Check if the same commit's split introduced the same gap elsewhere
- [ ] **TESTS**: Add/restore CI enforcement so this can't silently regress a 3rd time

## Validation performed before filing

- Path-validation gate (`.claude/commands/_audit-validate.sh`): PASS
- Re-ran `cargo clippy -p byroredux-renderer --lib -- -D warnings` directly: reproduced 9 errors at the exact reported lines — CONFIRMED
- Dedup: searched open + all-state issues (400-issue window) for "unsafe"/"safety"/"post_passes" keywords — no open duplicate found; #1904 and #2131 both confirmed CLOSED via `gh issue view`
