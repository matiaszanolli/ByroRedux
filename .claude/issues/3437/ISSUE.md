# #3437 — SAFE-2026-08-27b-02: pre-10.1.0.106 NiControllerSequence defaults cycle_type to 0 (CYCLE_LOOP) where nif.xml specifies CYCLE_CLAMP (=2), with a -inf duration in the same branch

- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-27b.md`
- **Severity**: MEDIUM
- **Labels**: `medium,safety,nif-parser,nif,animation,game:oblivion,bug`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3437

---

From `docs/audits/AUDIT_SAFETY_2026-08-27b.md` (Dimension 8 — animation / NIF parse mismatch).

- **Severity**: MEDIUM
- **Location**: `crates/nif/src/blocks/controller/sequence.rs:310-332`; mapping at `crates/nif/src/anim/types.rs:35-42`; spec at `/mnt/data/src/reference/nifxml/nif.xml:1024-1026`, `:4218`, `:82-83`
- **Status**: NEW

## Description

For `stream.version() < V10_1_0_106` the `NiControllerSequence` fields are absent and the parser substitutes literals. Its own comment states what those should be:

> Defaults are nif.xml's own (`weight` 1.0, `frequency` 1.0, `cycle_type` **CYCLE_CLAMP = 0**, `start_time` FLT_MAX, `stop_time` FLT_MIN)

`CYCLE_CLAMP` is **not** `0`. nif.xml's `CycleType` enum is `CYCLE_LOOP = 0` / `CYCLE_REVERSE = 1` / `CYCLE_CLAMP = 2` (`nif.xml:1024-1026`), and the block's stated default *is* `CYCLE_CLAMP` (`nif.xml:4218`). The engine's own `CycleType::from_u32` agrees with nif.xml (`0 => Self::Loop`), so the substituted `0` is decoded as **Loop**. Every `NiControllerSequence` in the `10.0.1.0 ≤ v < 10.1.0.106` window therefore plays looping where the format says clamp — the comment asserting they are the same value is what makes it look correct.

The same `else` branch has a second, coupled property. `start_time` defaults to `f32::MAX` and `stop_time` to `f32::MIN`; both match nif.xml (`#FLT_MAX#` = `3.402823466e+38`, `#FLT_MIN#` = **`-3.402823466e+38`**). But `import_sequence` then computes `duration = stop_time - start_time` = `f32::MIN - f32::MAX`, which **overflows to `-inf`** (verified by execution). Today the wrong `cycle_type` masks it: the `Loop` arm gates on `if clip.duration > 0.0`, which is false, so nothing wraps and `local_time` stays finite. Correcting `cycle_type` to `2` **alone** routes these clips into the `Clamp` arm, `(local_time + delta).min(-inf) = -inf` on the first tick, and every such clip freezes at key 0. The two must be fixed together.

## Evidence

```rust
// crates/nif/src/blocks/controller/sequence.rs:328-332
let cycle_type = if has_ctlr_seq_fields {
    stream.read_u32_le()?
} else {
    0                       // ← decoded as CYCLE_LOOP, not CYCLE_CLAMP
};
```
```rust
// crates/nif/src/anim/types.rs:35-42 — agrees with nif.xml, not with the comment
pub fn from_u32(v: u32) -> Self {
    match v {
        0 => Self::Loop,
        1 => Self::Reverse,
        2 => Self::Clamp,
        _ => Self::Clamp,
    }
}
```
```xml
<!-- nif.xml:1024-1026 -->
<option value="0" name="CYCLE_LOOP">Loop</option>
<option value="1" name="CYCLE_REVERSE">Reverse</option>
<option value="2" name="CYCLE_CLAMP">Clamp</option>
<!-- nif.xml:4218 -->
<field name="Cycle Type" type="CycleType" default="CYCLE_CLAMP" since="10.1.0.106" />
<!-- nif.xml:82-83 -->
<default token="#FLT_MAX#" string="3.402823466e+38" />
<default token="#FLT_MIN#" string="-3.402823466e+38" />
```

The version window is live rather than theoretical: `NiControllerSequence` is "Root node in Gamebryo .kf files (version 10.0.1.0 and up)" (`nif.xml:4215`), and `crates/nif/src/version.rs` carries a deliberate *"old Oblivion" (v10.0.x)* layout predicate family (#1337). **Honest limit**: the audit did not census how many `NiControllerSequence` blocks in the supported titles actually land below `10.1.0.106`, so the blast radius is code-provable but not measured.

## Impact

Clips in the pre-`10.1.0.106` window play with the wrong cycle semantics (loop instead of clamp) — a visible animation defect on old-Oblivion content, and one that a naive one-line "fix the constant" turns into frozen poses because of the `-inf` duration in the same branch.

## Related

SAFE-2026-08-27b-01 / #3432 (the `duration` half), #1337 (the v10.0.x layout family), #687 (the last envelope-field misalignment in this parser), #2345 (the gate that introduced these defaults).

## Suggested Fix

Substitute `2` (`CYCLE_CLAMP`) and correct the comment to name nif.xml's actual numbering. In the same change, gate `duration` in `import_sequence` — `let duration = seq.stop_time - seq.start_time; let duration = if duration.is_finite() && duration > 0.0 { duration } else { 0.0 };` — so the corrected `Clamp` arm sees a sane envelope. Add a unit test that parses a `< 10.1.0.106` sequence and asserts `CycleType::Clamp` **and** a finite duration, so the two cannot be separated again.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other `has_ctlr_seq_fields` default substitutions in the same `else` chain)
- [ ] **CANONICAL-BOUNDARY**: version-specific defaulting stays in the parser, never re-derived downstream. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
