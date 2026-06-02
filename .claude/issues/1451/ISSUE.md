## SKIN-02: Stale MAX_TOTAL_BONES value in MAX_FIRST_SIGHT_UPLOADS_PER_FRAME doc comment

**Severity**: LOW (doc-rot)
**Domain**: renderer / GPU skinning
**Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:69`
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-02.md`

## Finding

Doc comment at `:69` cites stale `MAX_TOTAL_BONES (32768)` / ceiling `227`.
Live const is `MAX_TOTAL_BONES = 196608` (`:57`); correct ceiling `1366` is
stated at `:77` but `:69` was never updated after the `#1284` bump.

Code reads the live symbol — cosmetic only.

## Fix

Update `:69` from `Bumped to 227 (= \`MAX_TOTAL_BONES (32768) / ...`)` to
`Bumped to 1366 (= \`MAX_TOTAL_BONES (196608) / ...`)`, or delete the stale
sentence (`:77` already covers the full history).
