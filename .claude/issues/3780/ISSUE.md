# #3780 — AUD-2026-08-30-D5-01: the four ReverbBuilder parameters are unsourced bare literals — uniquely among the crate's tunables

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, audio, tech-debt, bug

---

**Audit**: `/audit-audio` — `docs/audits/AUDIT_AUDIO_2026-08-30.md` (Dimension 5 — Reverb Send & Routing), HEAD `64f64480`
**Finding ID**: `AUD-2026-08-30-D5-01`

- **Severity**: LOW
- **Status**: NEW

## Location

`crates/audio/src/lib.rs:463-469`

## Description

The global reverb effect is built as

```rust
ReverbBuilder::new()
    .feedback(0.85)
    .damping(0.6)
    .stereo_width(1.0)
    .mix(Mix::WET),
```

Four magic numbers, inline, with **no named constant, no rationale comment, and no cited source**. They are the only tunables in the crate with none of the three.

Every sibling constant carries at least a derivation:
- `SILENCE_DB` states the `log10` blow-up it clamps (`lib.rs:162-164`)
- `ABOVE_WATER_CUTOFF_HZ` carries a full derivation of kira's `g = tan(pi * clamp(f_c/f_s, 0.0001, 0.5))` and of why a "beyond-Nyquist" value would be numerically degenerate rather than transparent (`lib.rs:167-180`)
- `SUB_TRACK_CAPACITY` / `SEND_TRACK_CAPACITY` cite the ~400-emitter FO4 Diamond City Market figure and #842 (`lib.rs:296-307`)
- `DEFAULT_UNLOAD_FADE_MS` is pinned to `kira::Tween::default()` by a test (`tests.rs:20`)
- even the consumer-side `INTERIOR_REVERB_SEND_DB = -12.0` carries a one-line justification (`systems/audio.rs:59-61`)

Re-verified at HEAD: `crates/audio/src/lib.rs:463-469` unchanged.

## Why it matters

These four values decide how **every interior in every supported title** sounds; they are invisible to `cargo test` (no test asserts on them — the five `reverb_tests` cover only the send **level** gate and its transitions); and they are not greppable as a tunable.

The practical consequence is that the next person tuning interior acoustics — the per-cell acoustics work `set_reverb_send_db`'s own #847 note defers to — has no recorded baseline to move away from and no way to tell an authored choice from a copied example.

## Deliberately NOT proposed: replacement values

Nothing in the tree, in kira's docs, or in the Gamebryo 2.3 reference establishes what a Bethesda interior reverb should be, and inventing a plausible-sounding quadruple is exactly the failure the project's no-guessing rule exists to prevent. **This issue proposes no values.**

## Recommendation

The remediable half is the **hygiene** half: promote the four to named `const`s beside `UNDERWATER_CUTOFF_HZ` and record whatever provenance the original author had, or state explicitly that they were chosen by ear.

That converts an invisible magic number into an honest, greppable, revisitable one — without guessing a replacement.

## Related

- #847 (the per-cell acoustics work this baseline would be moved away from)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — any other bare literal passed to a kira builder in `crates/audio/src/`
- [ ] **TESTS**: A regression test pins this specific fix — once named, the constants can be asserted the way `DEFAULT_UNLOAD_FADE_MS` already is
