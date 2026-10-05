# #5360 — SPT-2026-10-05-D3-01: m-trees.sh's new attach check reads a real zero-billboard result as "byro-dbg never answered", so the gate's own target regression is blamed on the debug attach

- **Labels**: low,speedtree,tech-debt,game:fnv,game:fo3,game:oblivion,bug
- **Filed from**: `docs/audits/AUDIT_SPEEDTREE_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5360

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `docs/smoke-tests/m-trees.sh:126-131`, `:133`, `:147-151`; `tools/byro-dbg/src/display.rs:10-13`; `tools/byro-dbg/src/main.rs:57`
- **Status**: NEW (a defect in the #5142 fix `858dee21f`)
- **Description**: #5142 added this guard so that a dead `byro-dbg` session is no longer parsed as 0 billboards:
  ```bash
  if ! grep -qE '^\([0-9]+ entities\)' "$dbg_log"; then
      echo "… HARD FAIL — byro-dbg never answered 'entities Billboard' … the attach itself failed"
  ```
  `byro-dbg` never prints `(0 entities)`. For an empty `EntityList` it prints `(no entities)` (`display.rs:11-13`). In piped-stdin mode that text follows the unterminated `byro> ` prompt, so the line is `byro> (no entities)`. That line matches neither the `[0-9]+` alternative nor the `^` anchor. With N > 0, the `(N entities)` row follows the entity lines on its own line, so the anchor matches; this is why the live-verified run (1416 billboards) passed.
- **Evidence**:
  - When a cell loads but the `.spt` route spawns no `Billboard` entities, the attach-failure branch fires. That is the Phase-1.5 regression this gate exists to catch, and the failure is still blamed on the attach.
  - The diagnosis written for this case is now unreachable for a zero count. That message is `zero indicates the .spt extension switch isn't routing` at `:150`, and it can only fire for 0 < n < floor.
  - `m41-equip.sh:175` uses an unanchored `awk '/^\(.*entities\)/'`, which also does not match `byro> (no entities)`. It is not in this audit's scope, but it has the same shape.
- **Impact**: The pass/fail verdict is still correct, because both branches HARD FAIL. The diagnosis is inverted, though. Before #5142, a broken attach blamed the healthy `.spt` route. Now a broken `.spt` route blames the attach, and the triager is pointed at the debug server rather than `synth_child.rs` / `parse_and_import_spt`. This is a manual gate only, with no CI exposure.
- **Related**: #5142 (CLOSED), SPT-2026-09-29-D3-01, `scripts/check-byro-dbg-harness-contracts.sh` (it checks the opt-in and screenshot shapes, not the response parsing).
- **Suggested Fix**: Treat `(no entities)` as a valid answer with a count of 0. For example, make the attach check `grep -qE '\((no|[0-9]+) entities\)'` (unanchored, which tolerates the `byro> ` prefix) and parse `no` as 0, so the existing floor branch reports the `.spt` routing diagnosis.

Report ID in `AUDIT_SPEEDTREE_2026-10-05.md`: **SPT-D3-01** (filed under a dated ID because the bare ID collides with closed #998).

_Source: `AUDIT_SPEEDTREE_2026-10-05.md` (SPT-D3-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
