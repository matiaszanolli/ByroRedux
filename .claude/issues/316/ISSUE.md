# Issue #316 — D2-02: BLAS compaction phase 6 leaks on alloc failure

- **Severity**: LOW | **Source**: AUDIT_RENDERER_2026-04-14.md | **URL**: https://github.com/matiaszanolli/ByroRedux/issues/316

`acceleration.rs:609-641`: mid-loop `?` propagates without destroying earlier-iteration `compact_accels` or `prepared` originals. Wrap phase 6 in a closure with on-error cleanup mirroring lines 582-594/665-682.
