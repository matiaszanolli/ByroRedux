# #3146 — ESM-2026-08-20-D2-01 / LC-D6-02: `decode_data`'s 144-220 tail is unreachable on every vanilla record and assigns offsets that contradict `decode_data_fo3nv` for the same fields

**Finding**: ESM-2026-08-20-D2-01 / LC-D6-02
**Labels**: bug, import-pipeline, low, legacy-compat
**Filed**: 2026-08-20 · `/audit-publish` · HEAD `bb0b92f2`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/3146

---

- **Severity**: LOW
- **Dimension**: ESM Dim 2 — sub-record byte accounting · LEGACY_COMPAT Dim 6 — per-game translation-survey gaps (Pattern C)
- **Record / Sub-record**: `WATR` / `DATA`
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:363-366` (the `len >= 186` early delegation) and `:396-433` (the tail reads and the `color_base` ternary that follow it)
- **Status**: NEW

> **Merged finding.** This is `ESM-2026-08-20-D2-01` and `LC-D6-02` from `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` — the same dead-code block, found independently by two dimensions. Neither is filed separately.

## Description

`decode_data` opens with an unconditional early delegation:

```rust
fn decode_data(data: &[u8]) -> WaterParams {
    if data.len() >= 186 {
        return decode_data_fo3nv(data);
    }
    …
```

after which roughly forty lines of the function are structurally unreachable on any real payload:

- `read_f32_at(data, 144 / 148 / 172 / 176 / 180 / 184 / 188 / 192)` each require `len >= offset + 4`, i.e. `len >= 148..196`
- `depth_weights` from `[208, 212, 216, 220]` requires `len >= 224`
- `effect_controls` from `[152, 156, 196, 204]` requires `len >= 208`

Every one of those is `None` for any `len < 186`. The whole block is reachable only for a `DATA` payload of length **148–185** — a window no supported game emits.

The consequence is not just dead lines. The `let color_base = if data.len() >= 186 { 40 } else { 36 };` ternary at `:433` **can only ever evaluate to `36`**, yet it reads as a live per-game branch and carries a `#1778` rationale comment explaining the 186-byte case that can never be taken here. It is also *inconsistent* dead code: it maps 152/156 to `effect_controls[0..2]` and 196/204 to `effect_controls[2..4]`, and 144/148 to the underwater fog pair — assignments that for a sub-186-byte record cannot be the same fields `decode_data_fo3nv` reads at those offsets in the long layout.

## Evidence

Sub-record length census over the installed masters (`DATA` on `WATR`, independent GRUP walks):

```
Oblivion.esm    DATA  2×1   42×2   62×1   86×2   102×17    (→ decode_data_oblivion, never reaches here)
Fallout3.esm    DATA  2×42  186×11
FalloutNV.esm   DATA  2×70  186×8
Skyrim.esm      DATA  2×34
Fallout4.esm    DATA  0×42
SeventySix.esm  DATA  0×47
Starfield.esm   DATA  0×15
```

**No length falls in `[148, 185]`.** Non-Oblivion `DATA` is only ever 2 or 186 bytes.

## Impact

**None at runtime — it is dead.** This is filed at LOW as a maintenance hazard, not a bug.

The cost is that it reads as a live fallback path during audit and maintenance. This file now carries five sibling offset maps for what byte evidence shows is one wire structure (see #3104–#3110), and a decoder that appears to have a sixth, contradictory map for the same fields is exactly how that divergence accumulated. The stale `#1778` comment on an unreachable ternary will be cited as a live per-game branch by the next reader.

## Related

- #3104, #3105, #3106, #3107, #3108, #3110 — the WATR offset-map arbitration. This dead map is the seventh, and deleting it removes one source of the disagreement.
- The 186-byte `DATA` path this delegates to (`decode_data_fo3nv`) is live and correct-ish; only the fall-through tail is dead.

## Suggested Fix

Delete the unreachable tail from `decode_data` (everything from the `read_f32_at(data, 144)` block through the `effect_controls` loop), collapse `color_base` to the constant `36`, drop the stale `#1778` comment, and rename the function to say what it actually is — the short compatibility shape for damage-only stubs and synthetic fixtures.

Net effect: the file has exactly one offset map per real layout.

---
*Filed from `docs/audits/AUDIT_ESM_2026-08-20.md` (D2-01) and `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-20.md` (LC-D6-02), merged. Verified against HEAD `bb0b92f2` before filing.*

## Completeness Checks
- [ ] **SIBLING**: the other `decode_*` arms in `water.rs` checked for the same early-return-then-dead-tail shape
- [ ] **CANONICAL-BOUNDARY**: deleting the block must not change what any live path produces — the 186-byte and 2-byte cases are the only reachable inputs
- [ ] **TESTS**: any existing test that reaches the dead tail is a synthetic fixture in the `[148, 185]` window and should be deleted with it, not preserved
