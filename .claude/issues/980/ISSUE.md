# Issue #980

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/980
**Title**: NIF-D5-NEW-04: bhkPoseArray + bhkRagdollTemplate missing — FO3/FNV death-pose system silently disabled
**Labels**: bug, nif-parser, medium
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (Dim 5)
**Severity**: MEDIUM
**Dimension**: Coverage
**Game Affected**: FO3, FNV (`.psa` files in `meshes\ragdollconstraint\`, `.rdt` death-pose templates)
**Location**: missing from `crates/nif/src/blocks/mod.rs` dispatch

## Description

`nif.xml` defines two FO3+ ragdoll-extension types under `versions="#FO3_AND_LATER#"`:

```
<niobject name="bhkPoseArray" inherit="NiObject"
          module="BSHavok" versions="#FO3_AND_LATER#">
    Found in Fallout 3 .psa files, extra ragdoll info for
    NPCs/creatures. (usually idleanims\deathposes.psa)

<niobject name="bhkRagdollTemplate" inherit="NiExtraData"
          module="BSHavok" versions="#FO3_AND_LATER#">
    Found in Fallout 3, more ragdoll info?
    (meshes\ragdollconstraint\*.rdt)
```

Both files (`.psa` / `.rdt`) are loaded when an NPC dies — `bhkPoseArray` supplies pre-canned death poses; `bhkRagdollTemplate` defines per-creature ragdoll constraint hierarchy. On FO3+ both fall through to `NiUnknown` (block_size recovery skips the body).

## Impact

NPCs ragdoll into the default skeletal-pose null instead of canned death poses. Not parse-fatal but content-incorrect — every dead raider / molerat in FO3/FNV falls in the same generic spine-collapse pattern instead of the authored "shot in the chest" / "stumble-and-drop" poses.

Could promote to HIGH if a confirmed parse-time cascade is observed on Oblivion content that's been retro-fitted with FO3-style ragdolls (modder ports — exists in modlists but not vanilla).

## Suggested Fix

Stub parser pair — `bhkRagdollTemplate` inherits NiExtraData (base parse exists; trailing payload is a u32 + byte-array we can skip via block_size); `bhkPoseArray` inherits NiObject directly (raw byte-array trailer).

```rust
// blocks/mod.rs near the other BSHavok extras
"bhkPoseArray" => Ok(Box::new(BhkPoseArray::parse(stream, block_size)?)),
"bhkRagdollTemplate" => Ok(Box::new(BhkRagdollTemplate::parse(stream, block_size)?)),

// blocks/collision.rs (or new blocks/ragdoll.rs)
impl BhkPoseArray {
    fn parse(stream: &mut NifStream, block_size: Option<u32>) -> io::Result<Self> {
        // Stub: skip body using block_size. Real parser is a follow-up
        // once we have a .psa sample to crack.
        if let Some(sz) = block_size { stream.skip(sz as usize)?; }
        Ok(Self {})
    }
}
```

Body parse can be expanded later once we have a `.psa` / `.rdt` sample to inspect — what matters today is preventing the silent drop and surfacing the type to the dispatch table so future content consumers can downcast it.

## Completeness Checks

- [ ] **TESTS**: Add a fixture test with synthetic `bhkPoseArray` / `bhkRagdollTemplate` blocks; assert dispatch routes through the new arms (not `NiUnknown`)
- [ ] **REAL_DATA**: Locate a vanilla FNV `.psa` (`meshes\actors\character\character assets\deathposes.psa` or similar) and verify the new stub parses without warn
- [ ] **NIFXML_DEEP**: Inspect nif.xml entries for `bhkPoseArray` and `bhkRagdollTemplate` — if a trailing-field schema exists, file a follow-up to expand the stub into a real parser
- [ ] **SIBLING**: Other `#FO3_AND_LATER#` BSHavok extras missing from dispatch? Cross-ref nif.xml `module="BSHavok"` entries

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → MEDIUM → NIF-D5-NEW-04.

