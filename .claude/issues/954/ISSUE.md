# #954 — REN-D3-NEW-01: Bindless layout lacks VARIABLE_DESCRIPTOR_COUNT

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM3.md`
**Dimension**: Pipeline State
**Severity**: LOW
**Confidence**: HIGH
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/954

## Location

`crates/renderer/src/texture_registry.rs:206-215`

## Summary

Bindless texture binding uses fixed `descriptor_count = max_textures` with `PARTIALLY_BOUND | UPDATE_AFTER_BIND` flags. Never sets `VARIABLE_DESCRIPTOR_COUNT`. Not a correctness bug today (pool sizes and allocations match the fixed count), but a future "shrink bindless allocations below `max_textures` at low-RAM startup" path needs the flag added first.

## Fix (preferred)

Add `vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT` to the `binding_flags` array. One line, zero behaviour change.

## Tests

Optional — a unit test asserting the binding_flags includes `VARIABLE_DESCRIPTOR_COUNT` would pin the contract.
