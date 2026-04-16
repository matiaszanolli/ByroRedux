# Issue #317 — D1-02: Empty-TLAS path emits size=0 buffer barriers

- **Severity**: LOW | **Source**: AUDIT_RENDERER_2026-04-14.md | **URL**: https://github.com/matiaszanolli/ByroRedux/issues/317

`acceleration.rs:1022-1068`: when `copy_size == 0`, both barriers + the copy emit `size = 0`, which is driver-defined. Early-return when copy_size==0; empty TLAS build is still well-formed.
