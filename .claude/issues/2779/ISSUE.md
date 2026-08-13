# REN-D14-NEW-04: caustic.rs storage_view/sampled_view are redundant byte-identical views

## Description
`storage_view` and `sampled_view` are built from the same closure with identical `ImageViewCreateInfo` — byte-identical handles — while the field doc claims a distinction the `610cb170` RGB-array refactor removed. Four redundant `VkImageView`s where two suffice, and every teardown path has to destroy both.

## Location
`crates/renderer/src/vulkan/caustic.rs` (`CausticSlot::sampled_view`, `create_slot`)

## Severity / Domain / Type
low / renderer,tech-debt / bug

https://github.com/matiaszanolli/ByroRedux/issues/2779

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D14-NEW-04).
