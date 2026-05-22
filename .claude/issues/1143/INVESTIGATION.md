# #1143 Investigation

## Resolution

**Defer-until-#928** (M-LIGHT v2 / volumetric flip). The audit's premise rests on speculation about LLVM DCE behaviour that the issue itself explicitly asks to verify under RenderDoc / Nsight before acting.

## Verification of the code paths

Both sites cited (now at lines ~1675 and ~2464 after various intervening edits):

### Site 1 — `draw.rs:1675` (CompositeParams depth_params)

```rust
depth_params: [
    if sky_params.is_exterior { 1.0 } else { 0.0 },
    0.85,
    if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED { 1.0 } else { 0.0 },
    0.0,
],
```

Pure expression: `if false { 1.0 } else { 0.0 }`. LLVM (and Rust's MIR optimiser before it) folds this to the literal `0.0` deterministically. No DCE risk. No dispatch is gated on this site.

### Site 2 — `draw.rs:2464` (volumetric inject + integrate dispatch)

```rust
if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED {
    if let Some(ref mut vol) = self.volumetrics {
        let vol_tlas = self.accel_manager.as_ref().and_then(|accel| accel.tlas_handle(frame));
        if let Some(tlas) = vol_tlas {
            vol.write_tlas(...);
            // ... actual dispatch ...
        }
    }
}
```

**The if-let chain is INSIDE the const-gated block, not enclosing it.** The audit's framing — "the enclosing if-let chain may not receive same DCE treatment if the compiler can't prove `self.volumetrics.is_some()` is the only check protecting state needed by the false branch" — appears to mis-describe the actual control-flow shape.

`if false { ... entire block including if-let ... }` is the canonical DCE pattern; Rust + LLVM remove the whole contents reliably. The `self.volumetrics`, `self.accel_manager`, `tlas_handle`, and `write_tlas` calls live inside the dead branch, not alongside it.

## Why the audit recommendation isn't acted on

1. **The audit explicitly asks for verification first**: "Currently un-measured. ... Recommendation: verify under RenderDoc / Nsight before fixing."
2. Per `feedback_speculative_vulkan_fixes.md`: shipping Vulkan changes when failure modes are invisible to `cargo test` requires RenderDoc validation or a revert path. The "fix" here (Cargo feature) is fix-against-hypothetical and would introduce a flag the CI matrix has to track for zero confirmed benefit.
3. The audit's recommendation (`#[cfg(feature = "volumetrics")]`) is structurally awkward because:
   - The feature would default `off`, making the volumetric pipeline invisible to type-checking on default builds (catches drift only when someone runs `cargo check --features volumetrics`).
   - Once #928 flips the const to `true`, the feature gate becomes redundant: the dispatch is live, so the DCE concern is moot.

## When the concern actually goes away

#928 (M-LIGHT v2 — multi-tap soft shadows + temporal stability) is the open tracker that, when it lands, flips `VOLUMETRIC_OUTPUT_CONSUMED = true` and removes the `* 0.0` in `composite.frag`. At that point:

- The dispatch becomes intentional, fully live, paying the documented ~1.84M ray-query traces / ~28 MB bandwidth per frame.
- The DCE concern is irrelevant — the code path is supposed to run.
- Any leftover dead-code in the `if false` branch (if any actually existed today) is wiped out by the flip.

## Closure

This issue's audit premise is the LOW-confidence speculative kind that the project's own `feedback_speculative_vulkan_fixes.md` flags as needing verification before action. The concrete control-flow inspection above shows no actual leak — `if false { ... if-let ... }` is the canonical DCE case Rust handles correctly. Closing without code change.

If a future profile (RenderDoc, Nsight, `perf stat`, `cargo asm`) shows the volumetric path is consuming CPU/GPU time at runtime today (with the const at `false`), reopen with the profile attached and the fix becomes targeted to whatever the profile actually shows.
