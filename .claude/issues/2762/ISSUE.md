# REN-D11-2026-08-12-06: gbuffer.rs module header says "Five" auxiliary targets, table lists seven

## Description
Opens "Five auxiliary render targets" while the table two lines below correctly lists **seven** (the FSR reactive + transparency attachments, `5c56e311` / `5c7acfe2`). NEW — merged with stale-run `REN-D11-02`, file once.

## Location
`crates/renderer/src/vulkan/gbuffer.rs` (module header)

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2762

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D11-2026-08-12-06).
