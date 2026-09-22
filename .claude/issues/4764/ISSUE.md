# COORD-03: coordinate-system.md's --rotation-mode fallback claim is false — the code still clamps, contradicting the doc's own #4126 citation

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4764
**Severity**: LOW
**Dimension**: 1 — Coordinate-system correctness (documentation)
**Location**: `docs/engine/coordinate-system.md:182-186` vs. `byroredux/src/boot/mod.rs:294-303`
**Labels**: low, documentation, doc-rot, legacy-compat
**Source**: docs/audits/AUDIT_LEGACY_COMPAT_2026-09-22.md (COORD-03)

## Description

`docs/engine/coordinate-system.md:182-186` was rewritten in commit `d0dac91b1`
(2026-09-13, "Improved documentation for coordinate system handling and
rotation modes") to claim that the `--rotation-mode` CLI flag's out-of-range
safety fallback already works — but that claim is false, and cites the still-open
`#4126` as if it were resolved.

The doc now reads:

> `--rotation-mode N` (default `1`, wired in `byroredux/src/boot/mod.rs`);
> any value outside `0..=3` is passed through unclamped to
> `euler_zup_to_quat_yup_mode`'s own `_ =>` arm, which falls back to the safe
> shipping formula (mode 1) rather than silently producing garbage placement
> (`#4126`)

`#4126` is the open issue tracking the fact that this is *not* what the code
does. The code still pre-clamps out-of-range values with `.min(3)` before
`euler_zup_to_quat_yup_mode` is ever called, so its `_ =>` fallback arm can
never fire — an out-of-range mode silently becomes mode 3 (CCW +
`Rx·Ry·Rz`, one of the two explicitly-wrong diagnostic conventions), not the
documented safe fallback to mode 1.

This is worse than ordinary doc-rot: the paragraph doesn't just describe
stale behavior, it actively asserts the open bug (`#4126`) is fixed,
pointing at `#4126` as supporting evidence for a claim `#4126` itself
contradicts.

## Evidence

Doc (`docs/engine/coordinate-system.md:182-186`, current HEAD `c3f298a24`):

```
`--rotation-mode N` (default `1`, wired in `byroredux/src/boot/mod.rs`);
any value outside `0..=3` is passed through unclamped to
`euler_zup_to_quat_yup_mode`'s own `_ =>` arm, which falls back to the safe
shipping formula (mode 1) rather than silently producing garbage placement
(`#4126`):
```

Code (`byroredux/src/boot/mod.rs:299-303`, current HEAD `c3f298a24`):

```rust
if let Some(idx) = args.iter().position(|a| a == "--rotation-mode") {
    if let Some(mode) = args.get(idx + 1).and_then(|v| v.parse::<u8>().ok()) {
        crate::cell_loader::set_refr_rotation_mode_diag(mode.min(3));
        log::info!("--rotation-mode {} active", mode.min(3));
    }
}
```

`mode.min(3)` clamps before `set_refr_rotation_mode_diag` is even called —
`euler_zup_to_quat_yup_mode`'s `_ =>` fallback arm (downstream, in
`byroredux/src/cell_loader/euler.rs`) never sees an out-of-range value, so it
can never run.

Live-verified at time of filing: `#4126` (`COORD-01: --rotation-mode CLI
dispatcher defeats its own out-of-range safety fallback`) is **OPEN**.

## Impact

Low direct impact — same narrow blast radius as `#4126` itself (this is an
opt-in diagnostic CLI flag, never on the default game-loading path, so no
player-visible placement is affected).

But it compounds `#4126`: a contributor or auditor reading the authoritative
reference doc now sees the bug described as already fixed. That risks `#4126`
being dismissed as stale/already-resolved without checking the code, or a
"fix" landing that only touches the doc (marking `#4126` closed) rather than
the actual `.min(3)` clamp.

## Related

- `#4126` — COORD-01, the underlying open code bug this doc paragraph
  misrepresents as fixed.

## Suggested Fix

Either:
1. Fix the code to match the doc: drop the `.min(3)` clamp in
   `byroredux/src/boot/mod.rs` (per `#4126`'s own suggested fix) so an
   out-of-range value reaches `euler_zup_to_quat_yup_mode`'s `_ =>` arm and
   gets the documented safe fallback to mode 1 — this closes both `#4126`
   and this issue at once; or
2. If `#4126` is going to stay open a while longer, revert this paragraph to
   the previously-accurate "clamped to `0..=3`" wording until the real code
   fix lands, so the doc doesn't actively mislead in the meantime.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
