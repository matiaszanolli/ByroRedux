# N1-05: NiExtraData subclass parsers read name unconditionally (latent fragility)

## Finding: N1-05 (LOW)

**Source**: `docs/audits/AUDIT_NIF_2026-04-15.md` / Dimension 1
**Games Affected**: Latent — any subclass parsed on a pre-10.0.1.0 NIF (affected subclasses are Bethesda-only today, so unreachable in practice).
**Location**: `crates/nif/src/blocks/extra_data.rs:167, 218, 274, 309, 344, 380, 468, 509`

## Description

nif.xml gates `NiExtraData.Name` on `since="10.0.1.0"`. `ExtraData::parse` wrapper has explicit `parse_legacy` fallback, but specialised subclass parsers (`BsBound::parse`, `BSFurnitureMarker::parse`, `BSBehaviorGraphExtraData::parse`, `BSInvMarker::parse`, `BSDecalPlacementVectorExtraData::parse`, NiVectorExtraData variants, `BSPackedAdditionalGeometryData` channel name, `BSShaderTextureSet` name) call `stream.read_string()` directly with no version gate.

Any future caller or fuzzed input that dispatches these with a <10.0.1.0 stream will interpret the first 4 bytes as a string length and misalign.

## Suggested Fix

Extract a shared `read_extra_data_name(stream)` helper in `base.rs` that returns `None` for `version < 0x0A000100`. Replace the direct `read_string()` calls.


## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_NIF_2026-04-15.md`._
