# #5278: UI-D7-2026-10-05-02: #4723 and #4725 left a doc splice, a stale "prefix guard" doc, a skip check that can never fire, and a test that asserts nothing

Labels: low,ui,documentation,doc-rot,test-gap
Filed from: docs/audits/AUDIT_UI_2026-10-05.md

**Source**: `docs/audits/AUDIT_UI_2026-10-05.md` (UI-D7-2026-10-05-02) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: MenuXml & HUD Drivers
- **Profile**: both
- **Location**:
  - `byroredux/src/hud.rs:462-478`
  - `byroredux/src/scaleform_hud.rs:445-459` and `:62`
  - `byroredux/src/scaleform_hud.rs:591-606` (`the_push_table_skips_reserved_engine_callbacks`)
- **Status**: NEW. The splice is the same class as open #5029.
- **Description**:
  - **Doc splice in `hud.rs`.** `menu_owned_overlay_skip` was inserted between `launch_hud`'s doc comment and
    `launch_hud`. The doc "Launch the HUD when `--hud` is present … registers the transparent overlay textures" now
    heads the helper's doc, and `launch_hud` has none. "textures" is also stale, since #4892 left one texture.
  - **Stale push doc.** The `push` doc still says FO4's lifecycle hooks are "skipped by the prefix guard".
  - **The skip at `:457-459` can never fire.** The `match` at `:460-472` acts only on
    `UpdateStats|updateStats|UpdateCompass|updateCompass`, and its `_ => continue` arm already skips every reserved
    name.
  - **The test asserts nothing.** Its pin pushes into a `UiManager` with no player, where invoke is a no-op, and then
    asserts `RESERVED_ENGINE_CALLBACKS.contains(&RESERVED_ENGINE_CALLBACKS[0])`. Deleting the skip, or the whole push
    body, leaves it green. The skill names this test as the guard for the exact-membership rule.
  - **Spliced enum line.** `scaleform_hud.rs:62` reads `pub(crate) enum ScaleformGame {    /// AVM1 …`, flagged by
    `rustfmt --check`.
- **Impact**: documentation and test quality only. Behaviour is correct today, but the guard would not catch a
  regression.
- **Related**: #4723, #4725 (closed), #5029 (open)
- **Suggested Fix**:
  - Move the launch doc back above `launch_hud` and update both docs.
  - Either drop the redundant skip, or make the test observe it (for example, by counting invoke attempts against a
    loaded player).
  - Fix the spliced enum line.

## Completeness Checks
- [ ] **SIBLING**: Same drift checked in sibling docs / comments / skill files
- [ ] **TESTS**: Where a number is restated, a test or measured source pins it
