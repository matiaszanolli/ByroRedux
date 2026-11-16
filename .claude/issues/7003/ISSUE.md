# #7003: UI-D1-2026-11-15-01: `docs/engine/ui.md`'s `SwfPlayer` API block gives `invoke_callback` a stale signature and omits four fields

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/7003
- **Labels**: low,ui,documentation,doc-rot
- **Source**: docs/audits/AUDIT_UI_2026-11-15.md

From the UI audit `docs/audits/AUDIT_UI_2026-11-15.md` (HEAD 8473ae994).

- **Severity**: LOW
- **Dimension**: Profile & Bridge
- **Profile**: both
- **Existing**: none (earlier ui.md drift issues #2971, #3774, #5733, #6530 are closed and about other lines)
- **Location**: `docs/engine/ui.md:179` (and the struct listing at `docs/engine/ui.md:145-158`) vs `crates/ui/src/player.rs:729-733` and `:164-205`
- **Status**: NEW
- **Description**: the `SwfPlayer` block documents `pub fn invoke_callback(&mut self, name: &str, args: &[ScaleformValue]) -> bool;`. The real signature is `invoke_callback(&mut self, name: &str, arguments: impl IntoIterator<Item = ScaleformValue>) -> Option<ScaleformValue>`, and the `None` vs `Some(Null)` distinction (unknown or unregistered vs registered-returning-null, 86f5a1a0a / #5732) is the contract the Papyrus bridge and menu stack will rely on. The `UiManager` block 100 lines lower (`ui.md:297-301`) already has the correct form, so the file contradicts itself. The struct listing also omits `pacer`, `render_passes`, `resource_loads` and `resource_loads_capped`, and the method list omits `new_with_profile`.
- **Evidence**: `docs/engine/ui.md:179` ends `-> bool;` while the `UiManager` block at `:297-302` ends `-> Option<ScaleformValue>;`. `ui_doc_signature_pin` (`lib.rs:504-564`) pins only the `profile:` parameter of three constructors, which is why this survived.
- **Impact**: a reader writing the menu-stack or Papyrus bridge from the doc would treat an unregistered callback as `false` and lose the `None`/`Some(Null)` distinction. No runtime effect.
- **Suggested Fix**: correct the block (signature, the four fields, `new_with_profile`); extend `ui_doc_signature_pin` with a return-type check for `invoke_callback` (assert both the doc and `player.rs` say `Option<ScaleformValue>`).
- **Completeness Checks**: SIBLING the `UiManager` block is already right; TESTS extend the pin.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
