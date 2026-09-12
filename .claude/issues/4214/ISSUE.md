# TD6-003: IMGS (image-space/HDR tonemap) records parsed but have zero downstream consumer

Labels: medium,tech-debt,esm-plugin,renderer,bug

**Description**: `parse_imgs` intentionally only captures `EDID` + raw `DNAM` bytes ("deferred to M48" per its own comment). `grep -rn image_spaces crates byroredux` shows the map is populated on every ESM parse and never read again — no cell-loader binding, no render-pass consumer. The entire per-region/per-cell HDR/tint/tonemap feature currently has zero rendering effect despite full indexing. The "deferred to M48" framing is stale — M48 shipped (Scaleform UI route) and didn't touch this. Closed issue #624 ("IMGS dispatch") title suggests this dispatch was meant to land there but the render-side consumer was not part of what actually shipped.

**Evidence**:
`crates/plugin/src/esm/records/misc/world.rs:1358` (`ImgsRecord`), `:1372` (`parse_imgs`); stored via `dispatch_misc_gameplay_a.rs:118` into `EsmIndex.image_spaces` (`index.rs:185`).

**Impact**: Every `--esm` load populates the map at parse cost with zero visual effect; visually invisible until compared against a reference (no crash, no wrong output relative to what ships today — the gap is a missing feature, not a regression in rendered output).

**Related**: Regression of #624 (closed; its own title names "IMGS dispatch" as delivered, but no render-side consumer exists in current code).

**Suggested Fix**: Land the DNAM decode + a render-side consumer, or correct the stale "deferred to M48" comment and link a fresh tracking issue. Full implementation is large (DNAM + IMAD modifier-graph parser + render consumer); realistically gated on a future cinematics/post-process milestone.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
