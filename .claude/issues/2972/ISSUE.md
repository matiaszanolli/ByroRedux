# UI-D6-01: hiding the overlay stops Ruffle but not the compositor — render()'s None conflates "hidden" with "unchanged"

**Issue**: #2972
**Severity**: LOW
**Dimension**: Render & Device Lifecycle
**Labels**: `low,renderer,bug`
**Source report**: `docs/audits/AUDIT_UI_2026-08-16.md`
**Filed**: 2026-08-16 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_UI_2026-08-16.md` (Dimension 6 — Render & Device Lifecycle). Profile: both.

**Location**: `crates/ui/src/lib.rs`:118-136 · `byroredux/src/app_frame.rs`:255-274 · `crates/renderer/src/vulkan/context/draw.rs`:2977-2988

## Description

`UiManager::tick` is gated on `self.visible` and `UiManager::render` returns `None` when `!self.visible`. But since #2719 the engine reads `None` as "keep showing the texture you already have":

```rust
// byroredux/src/app_frame.rs:272
} else if self.ui_texture_handle.is_some() {
    ui_tex = self.ui_texture_handle;
}
```

and `draw.rs`:2977 emits the UI quad unconditionally whenever `ui_texture_handle` is `Some`.

So setting `visible = false` freezes the menu and **keeps compositing its last uploaded frame** on top of the world. The two distinct meanings — "nothing changed, reuse" and "do not draw" — share one `None`.

## Impact

**Unreachable today**, which is why it is LOW: `visible` is only ever set `true` (by `install_player`) or `false` (by `close`), and `close` has no caller — the dead-`close` half is already tracked as #2723.

It becomes a live, visible bug the moment either a menu-stack policy or a HUD-toggle sets `visible = false`, which is exactly the *Pending* work this mechanism has to carry.

## Suggested Fix

Have the frame driver gate the quad on `UiManager::visible` (plus a live player) rather than on `ui_texture_handle.is_some()`, **or** give `UiManager::render` a three-state return — *fresh pixels* / *unchanged* / *hidden* — so the compositor can tell the last two apart.

The three-state return is the more robust of the two: it makes the ambiguity unrepresentable rather than relying on the caller to check a second field.

## Related

- Existing #2723 (`close()` is dead; viewport pinned to setup-time extent)
- Existing #2722 (`render()` clears `dirty` on a failed capture)

**This finding is neither of those**: it is about the *meaning* of `None`, not about resize or capture failure. All three touch `render()`'s return contract, so they are worth resolving together.

## Completeness Checks
- [ ] **SIBLING**: Any other consumer of `UiManager::render`'s `None` audited for the same conflation
- [ ] **DROP**: If the UI quad's Vulkan resources change lifetime, the reverse-order teardown in `VulkanContext::drop` stays correct
- [ ] **CO-RESOLVE**: Checked against #2722 and #2723 — the three overlap on `render()`'s contract
- [ ] **TESTS**: A regression test sets `visible = false` and asserts the compositor stops emitting the quad

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2972 --json state` when live state is needed.*
