# REN-D7-2026-08-12-02: ui.vert slot-0 comment argues pre-#807 premise, contradicts gpu_types.rs corrected contract

- **Severity**: LOW
- **Dimension**: 7
- **Labels**: low,renderer,documentation

## Description
The UI slot-0 comment argues from the pre-#807 premise ("`materials[0]` is the FIRST scene material"), contradicting `gpu_types.rs`'s corrected contract — and slot-0 semantics is what the over-cap fallback rests on. Cites `scene_buffer.rs:172-176`; `scene_buffer` has been a **directory** since Session 34/35. `material.rs:281-294` is likewise rotted (`as_bytes` is near 594).

## Location
`crates/renderer/shaders/ui.vert`, `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`, `crates/renderer/src/vulkan/scene_buffer/descriptors.rs`

---
Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D7-2026-08-12-02).

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2797
