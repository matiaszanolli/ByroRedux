# 3233: NIFAL-D7: morph-target index space desyncs between weight channel and vertex-delta array

**Severity**: MEDIUM (→ HIGH once a GPU morph-blend consumer reads `morph_delta_address`/`morph_weight_address` for real) · **Dimension**: NIFAL Animation/controllers · **Report**: `docs/audits/AUDIT_NIFAL_2026-08-23.md` (NIFAL-D7-2026-08-23-01)

## Description

Two independent extractors both derive a "morph target index" from the same `NiMorphData.morphs` array, and the `GpuInstance` doc comment added by `5f4dea46` (#3231) explicitly intends the two to share one stable slot number so a future GPU consumer can pair `AnimatedMorphWeights[idx]` with `ImportedMesh.morph_targets[idx]`'s deltas.

- `resolve_morph_target_index` (`crates/nif/src/anim/channel.rs:16-30`, unchanged, existing since #262) returns `data.morphs.iter().position(...)` — the target's **original, unfiltered position**. This flows through `extract_float_channel` → `FloatTarget::MorphWeight(idx)` → `convert_nif_clip` → `attach_animation_sinks` → the live, per-frame-driven `AnimatedMorphWeights` component.
- `extract_morph_targets` (`crates/nif/src/import/mesh/morph.rs:60-97`, new in `c1339301`) iterates the same array but `continue`s past any target whose `.vectors.len() != vertex_count` and `break`s at `MAX_MORPH_TARGETS_PER_MESH` — both correct, documented fail-soft behaviors for the delta array in isolation — but the resulting `Vec<ImportedMorphTarget>` compacts around the gap: the position of every target after a dropped one shifts down by one. Nothing records the original index.

## Evidence

```rust
// crates/nif/src/import/mesh/morph.rs (extract_morph_targets)
for morph in &data.morphs {
    if morph.vectors.len() != vertex_count {
        log::warn!(/* dropping this target */);
        continue;               // shifts every later index down by one
    }
    if targets.len() >= MAX_MORPH_TARGETS_PER_MESH {
        break;
    }
    targets.push(ImportedMorphTarget { name: morph.name.clone(), deltas: ... });
}
```
```rust
// crates/nif/src/anim/channel.rs (resolve_morph_target_index)
data.morphs.iter().position(|m| m.name.as_deref()
    .is_some_and(|n| n.eq_ignore_ascii_case(target_name)))
    .map(|i| i as u32)          // always the ORIGINAL, unfiltered position
```

Concretely: if `NiMorphData.morphs = [Blink(ok), JawOpen(mismatched), BrowUp(ok)]`, `extract_morph_targets` returns `[Blink, BrowUp]` (indices 0/1), while `resolve_morph_target_index` still reports `BrowUp`'s controller as index `2` (its original position). The existing `drops_target_with_mismatched_vertex_count` test only asserts the surviving target's *name*, not index alignment, so it cannot catch this.

## Impact

Currently **dormant** — `ImportedMesh.morph_targets` has zero consumers outside `crates/nif` today, and `GpuInstance.morph_delta_address`/`morph_weight_address` are hardcoded to `0` per `5f4dea46`'s own commit message ("still a placeholder-zero follow-up"). The bug is already baked into the two landed pieces and will silently misapply facial-morph weights (or blend the wrong slider entirely) the instant the announced GPU-consumer follow-up in #3231 wires the delta/weight buffers by this index — on exactly the malformed-vertex-count content the guard exists to make safe.

## Related

Should be folded into #3231's follow-up work rather than tracked as fully independent, since the fix belongs in the same slice — but filed here so it isn't lost/forgotten before that phase lands.

## Suggested Fix

Give `ImportedMorphTarget` an explicit `original_index: u32` field carried through from `data.morphs`'s position, and have the eventual weight/delta buffer builder join on that field rather than on `Vec` position — or, simpler, make `extract_morph_targets` emit a fixed-size, index-preserving `Vec<Option<ImportedMorphTarget>>` (`None` for dropped/truncated slots) so position IS the stable index by construction. Land before or alongside #3231's GPU-consumer phase.

## Completeness Checks
- [ ] **TESTS**: Extend `drops_target_with_mismatched_vertex_count` to assert index alignment with `resolve_morph_target_index`, not just surviving-target name
- [ ] **CANONICAL-BOUNDARY**: Confirm the fix keeps this a parse-time (`crates/nif`) concern, not pushed into a render-time reconciliation
