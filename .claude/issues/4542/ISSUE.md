# REN-D12-2026-09-20-02: tex.dump has zero test coverage (arg split, menu-path resolution, decode-failure paths) and no debug-cli.md row while tex.missing/tex.loaded are documented

- **ID**: REN-D12-2026-09-20-02
- **Labels**: low,renderer,test-gap,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Debug/Telemetry
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D12-2026-09-20-02)

**Location**: `byroredux/src/commands/assets.rs` (`tex.dump`, ae572745f); `docs/engine/debug-cli.md`

**Description**
The archive-texture→PNG debug command landed with no tests and no doc row. Decode itself is properly bounded (menuxml decoder caps 8192², truncation-checked — distinct from REN-D5-2026-09-20-01), but the command surface (quoted-arg split, menus/menus80/menus50 candidate resolution, failure paths) is untested.

**Evidence**
Audit D12, 2026-09-20: `cargo test … tex_dump split_quoted` → 0 tests exist.

**Impact**
The first tex.dump regression ships silently; users can't discover the command.

**Suggested Fix**
Add the command tests + the debug-cli.md row.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
