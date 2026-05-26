# REN-D19-2026-05-26-02: Dead Option<BloomPipeline> guard — None branch unreachable since #1081 made init fatal

## Severity: Low (dead-code / reader-of-code clarity)

**Locations**:
- `crates/renderer/src/vulkan/context/draw.rs:2840` — `if let Some(ref mut bloom) = self.bloom`
- `crates/renderer/src/vulkan/context/mod.rs:1953-1967` — init policy makes the `None` branch unreachable

## Problem

`BloomPipeline::new` failure logs a warn and sets `bloom = None` at `mod.rs:1953-1957`. Immediately after, `mod.rs:1958-1967` does a hard fail:

```rust
let bloom_views: Vec<vk::ImageView> = match bloom.as_ref() {
    Some(b) => b.output_views(),
    None => {
        return Err(anyhow::anyhow!(
            "Bloom pipeline failed to initialize — composite \
             requires the bloom output view for binding 7 (M58). \
             Check earlier 'bloom' WARN logs."
        ));
    }
};
```

So if bloom init fails, `VulkanContext::new` returns `Err` — the engine never reaches `draw_frame`. The `if let Some(ref mut bloom) = self.bloom { ... }` guard at `draw.rs:2840` therefore always succeeds at runtime. Same for the per-frame warn-and-skip on dispatch failure (`draw.rs:2850-2852`).

This is **closed-by-design via #1081** (no fallback binding for bloomTex when bloom is absent). But the dead `Option` shape was left behind as residue.

## Impact

**Misleading reader-of-code surface.** Someone tracing the "what if bloom is off?" thread spends time chasing graceful-degradation logic that doesn't actually exist. The dead `Option` also adds noise to grep results when investigating bloom lifecycle.

## Fix — pick one

### (a) Match the contract — make `bloom` non-optional

- Change `pub bloom: Option<BloomPipeline>` at `context/mod.rs:1127` → `pub bloom: BloomPipeline`
- Drop the `Some(b)` arm at `context/mod.rs:1945-1952`: `let bloom = BloomPipeline::new(...)?;`
- Unwrap the `if let Some` at `draw.rs:2840` and `resize.rs:478`
- Remove the warn-and-skip at `draw.rs:2850-2852`

Cleanest. **Caveat**: the resize-recreate path may temporarily set bloom to `None` during destroy-then-create; verify before committing.

### (b) Add a doc-only crumb

Leave the `Option` (resize-recreate might benefit from it as a temporary), and add a comment at `draw.rs:2840` cross-referencing #1081 + `context/mod.rs:1958` so readers know the `None` branch is unreachable at runtime.

Less invasive but doesn't actually remove the dead code.

**Recommendation**: (a) if the resize path is verified safe to use `mem::replace` patterns or take-and-rebuild on the non-`Option`; otherwise (b).

## Completeness Checks

- [ ] **TESTS**: No new tests; existing bloom dispatch + resize tests cover the surface.
- [ ] **SIBLING**: Check whether `volumetrics` or `caustic_splat` have similar dead-`Option` shapes — same pattern is plausible for any post-process that became mandatory.
- [ ] **DROP**: If (a) is chosen, verify the `BloomPipeline::Drop` impl still runs in the correct reverse-order teardown (it would now drop with the rest of `VulkanContext`'s fields rather than via `Option::take`).
- [ ] **RESIZE**: Verify the resize-recreate path at `resize.rs:478-510` is compatible with a non-`Option` field (likely requires a take-and-rebuild pattern).

## Related

- #1081 (closed) — "No fallback binding for bloomTex when bloom pipeline is absent" — the policy decision that made the `None` branch unreachable. This finding is the dead-code residue from that fix.

Audit: `docs/audits/AUDIT_RENDERER_2026-05-26_DIM19.md` (Finding L2)
