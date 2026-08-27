# FNV-2026-08-26-D5-02

**Issue**: #3327
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 5 — NIF Parser Regression Guard
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `crates/nif/src/blocks/mod.rs:787-792` (dispatch) ·
`crates/nif/src/anim/entry.rs:404-421` (consumption)

**Premise verified**: `BSMaterialEmittanceMultController`, `BSRefractionStrengthController` and
`BSFrustumFOVController` dispatch to a **bare** `NiSingleInterpController`, erasing their RTTI:

```rust
// crates/nif/src/blocks/mod.rs:786-792
"BSMaterialEmittanceMultController"
| "BSRefractionStrengthController"
| "BSFrustumFOVController" => {
    Ok(Box::new(NiSingleInterpController::parse(stream)?))
}
```

`anim/entry.rs` dispatches embedded controllers by `block_type_name()`, and its comment states
outright that these three are the only blocks still reaching the `"NiSingleInterpController"` arm,
where the only thing attempted is `extract_transform_channel_at` — which "self-selects to `None`"
because none of them drives a transform. There is no float-channel arm for them, and
`anim::types::FloatTarget` (`crates/nif/src/anim/types.rs:89-112`) has no emissive-multiplier or
refraction-strength variant.

**Evidence**: FNV corpus counts from the header RTTI census —
`BSMaterialEmittanceMultController` 471, `BSRefractionStrengthController` 87,
`BSFrustumFOVController` 56 (614 total, matching the baseline's erased row exactly). Per
nif.xml:6780 / 6802 / 7019 all three inherit `NiFloatInterpController` → the abstract
`NiSingleInterpController` (nif.xml:3646) with no fields of their own, so the *byte* parse is
correct — only the name and the downstream routing are lost.

**Impact**: FNV-visible content gap, not a parse regression. 471 meshes author an animated
emissive multiplier (the pulsing/flickering neon and glow-panel work the Strip and interior
signage is built on) and 87 author animated refraction strength (Stealth Boy / heat-haze
effects); both animate flat. The parse rate stays 100% and nothing logs, so the loss is invisible.
It is also the residual half of the RTTI restoration #2562/#2563/#3175 performed for the four
transform-family controllers — the same fix shape was simply not extended to these three.

**Fix sketch**: give the three a `type_name`-carrying newtype (mirroring
`NiPreSplitDataController` / `BsNiAlphaPropertyTestRefController`), add
`FloatTarget::EmissiveMultiple` / `RefractionStrength`, and route them through
`extract_float_channel_at` in `anim/entry.rs`. Removes the abstract baseline row as a side effect.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
