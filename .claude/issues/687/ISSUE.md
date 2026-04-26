# Issue #687: O5-2: 80+ alloc-cap rejects in particle.rs silently truncate whole subtrees — drift-induced data loss

**Severity**: HIGH
**File**: `crates/nif/src/blocks/particle.rs` (NiPSysBoxEmitter / NiPSysGrowFadeModifier / NiPSysSpawnModifier)
**Dimension**: Real-Data Validation

84 of the 384 truncated files in the 2026-04-25 sweep trip on bogus allocation requests (1.7 GB / 2.1 GB / etc) — every one is a u32 harvested from a misaligned stream offset. The `check_alloc` gate at `crates/nif/src/stream.rs:219` correctly bounces them (no abort), BUT the `truncated: true` return path drops every block AFTER the failing one (median ~30 blocks lost per file).

**Concrete examples**:
- `meshes\\oblivion\\gate\\obgatemini01.nif` — 594 blocks dropped
- `meshes\\dungeons\\ayleidruins\\interior\\traps\\artrapchannelspikes01.nif` — 233 blocks dropped

These are downstream symptoms of upstream parser drift (a previous block under-consumed and the next u32 is interpreted as an alloc count). The 04-17 H-3 finding called this out as "victims, not perpetrators." Files that previously OOM-aborted are now silent data loss.

**Worst offenders** (~90+ instances combined):
- `NiPSysBoxEmitter`
- `NiPSysGrowFadeModifier`
- `NiPSysSpawnModifier`

**Fix**: Root-cause fix is **debug-mode per-parser consumed-byte cross-check against `parsed_size_cache`**. Add an end-of-block-parse assertion that the consumed byte count matches `block_size` (where block_size is known); the FIRST mismatch is the perpetrator. Bisect via `crates/nif/examples/trace_block.rs`.

This is the highest-leverage parse-rate recovery action — closing one drift source typically rescues a cluster of downstream blocks across many files.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
