title:	SF-D2-NEW-02: CLAUDE.md "22 Starfield texture BA2s" claim stale; actual count is 30
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	documentation, low
comments:	0
assignees:	
projects:	
milestone:	
number:	1185
--
**Source**: [`docs/audits/AUDIT_STARFIELD_2026-05-18.md`](docs/audits/AUDIT_STARFIELD_2026-05-18.md)
**Dimension**: BA2 v2/v3 LZ4 Block Decompression
**Severity**: LOW (doc rot, not a parser bug)

## Observation

`CLAUDE.md:305`:

> decompression via lz4_flex::block. Verified against 22 Starfield texture archives (~128K DX10 textures) + 53 vanilla FO4 BA2s, zero failures.

`ls "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data" | grep -i 'Textures.*\.ba2'` returns **30**. The "22" predates a Bethesda content update (likely the 2024 "big update" or 2025 Shattered Space delta).

## Why bug

Audit prompts (e.g., "sweep across all 22 Starfield texture BA2s") are sized against the stale number and will under-count. Per the `feedback_audit_findings` memory, stale docs are the most common source of audit findings with bad premises.

## Fix

Re-stamp the Session 7 paragraph with the current archive count. Ideally this lands together with `SF-D2-NEW-01` (in-tree corpus sweep): the test discovers archives at runtime so the docs can cite "all archives matching `Starfield - *.ba2`" without committing to a brittle absolute count.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: check other CLAUDE.md / ROADMAP.md / docs/engine/archives.md passages for stale absolute counts (FO4 archive count, NIF parse-rate snapshots, etc.)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A (docs only) — but ideally couple this with the SF-D2-NEW-01 sweep test landing so the doc can cite the test as the source of truth

## Related

- #708 / Session 7 — the BA2 v3 LZ4 work being referenced
- SF-D2-NEW-01 (separate issue) — in-tree sweep that would replace the absolute-count claim
