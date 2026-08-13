# REN-D13-04: taa.rs MAX_FRAMES_IN_FLIGHT assert weaker than ping-pong arithmetic requires

## Description
The local `MAX_FRAMES_IN_FLIGHT >= 2` assert is weaker than the `(f + 1) % MAX_FRAMES_IN_FLIGHT` history arithmetic requires — that expression selects the previous slot **only at exactly 2**. At 3 it resolves to two frames ago. The real gate is `sync.rs`'s `== 2` (#870), whose own comment enumerates the two remedies that would allow relaxing it, so relaxing it is a contemplated change. `volumetrics.rs` already uses the general form. `svgf.rs` has the same shape and the same weak reasoning.

## Location
`crates/renderer/src/vulkan/taa.rs` (module const-assert + `write_descriptor_sets`)

## Severity / Domain / Type
low / renderer,sync / bug

https://github.com/matiaszanolli/ByroRedux/issues/2771

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D13-04).
