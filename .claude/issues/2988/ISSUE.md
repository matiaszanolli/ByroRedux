# ESM-2026-08-16-D3-01: VMAD Object-property FormIDs are never load-order remapped, at all six call sites

**Issue**: #2988
**Severity**: HIGH
**Dimension**: 3 — FormID Remap, Load Order & ESL Space
**Labels**: `high,import-pipeline,bug`
**Source report**: `docs/audits/AUDIT_ESM_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ESM_2026-08-16.md` (Dimension 3 — FormID Remap, Load Order & ESL Space).

**Record / Sub-record**: `VMAD` (`QUST`, `SCEN`, `REFR`, `ACTI`, …)
**Location**: `crates/plugin/src/esm/records/script_instance.rs`:161-163 (`ScriptInstanceData::parse`), :489-505 (`property_value` type-1 arm)
**Call sites**: `crates/plugin/src/esm/cell/support.rs`:83 · `crates/plugin/src/esm/cell/walkers.rs`:684 · `crates/plugin/src/esm/records/common.rs`:289 · `crates/plugin/src/esm/records/misc/quest.rs`:574 · `crates/plugin/src/esm/records/misc/scene.rs`:170

**Status note**: NEW — a new instance of the class filed as ESM-D3-03, which cited `LTEX`/`SCOL`/`FLST` only. `script_instance.rs` was **explicitly excluded** from the 2026-08-13 Dim 2 sample (see that report's Cross-Audit Pointers).

## Description

`ScriptInstanceData::parse(data: &[u8])` has **no `FormIdRemap` parameter and no call site supplies one**, so a VMAD `Object` property's `form_id` stays in **plugin-local** space. Its consumers treat it as a **global** FormID.

## Evidence

The decoder reads the raw u32 and stores it verbatim:

```rust
// script_instance.rs:493-504
let (form_id, alias) = if object_format == 1 { … } else {
    let _unused = self.u16()?; let a = self.i16()?; let f = self.u32()?; (f, a)
};
PropertyValue::Object { form_id, alias }
```

Consumers:
- `crates/scripting/src/fragment.rs`:234-239 — `resolve_quest`'s `QuestRef::Property` arm maps `object_form_id(name)` straight into a `QuestFormId`
- `crates/scripting/src/translate/recognizers/quest_stage_gate.rs`:77-81 — the same value becomes `Condition::param_1` for `GetStageDone` (function 59), which `param1_is_form_id` remaps for **every other producer**

Contrast the correct shape landed in the same crate on 2026-08-16: `parse_omod_loose_item` (`items.rs`:517-524) takes `&Option<FormIdRemap>` and applies it, with a non-identity-remap test.

Re-verified 2026-08-17: `pub fn parse(data: &[u8]) -> Self` — no remap parameter; all six call sites pass a bare `&sub.data`.

## Impact

Under any non-identity load order — **every DLC, every multi-master stack, anything with an ESL** — a raw `0x01xxxxxx` VMAD property is compared against a remapped index.

It does not merely miss: `0x01` is a *valid* global slot, so the property can resolve to a **different plugin's record** and the fragment lowerer will advance the wrong quest with **no diagnostic**.

`parse_esm` passes `None`, so single-master loads are unaffected — which is why every in-crate test passes.

## Suggested Fix

Add a `remap: &Option<FormIdRemap>` parameter to `ScriptInstanceData::parse` (keeping `parse` for the no-remap tests as a thin wrapper), apply it in the `property_value` type-1 arm only, and thread the walker's existing `remap` through the five call sites — `cell/walkers.rs` and `misc/quest.rs` already hold one.

**Consider fixing the signature class once** rather than as five point fixes: #2906, ESM-D3-02, ESM-D3-03 and #2698 are all "a decoder whose signature cannot remap".

## Related

- #2906 (ESM-D3-01 — `XTEL`/`XESP`), ESM-D3-02, ESM-D3-03, #2698 (`XPRI`) — the same systematic omission
- Cross-reference `/audit-scripting` Dim 7 for the consumer half

## Completeness Checks
- [ ] **SIBLING**: All six call sites thread the remap, not just the two that already hold one
- [ ] **CLASS-FIX**: Considered fixing the decoder-signature class (#2906 / #2698 / D3-02 / D3-03) together rather than pointwise
- [ ] **NON-IDENTITY-TEST**: The regression test uses a non-identity `FormIdRemap` — an identity remap is exactly what hid this
- [ ] **CONSUMER**: `fragment.rs` and `quest_stage_gate.rs` verified to receive global-space ids after the fix
- [ ] **TESTS**: A regression test pins this specific fix

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2988 --json state` when live state is needed.*
