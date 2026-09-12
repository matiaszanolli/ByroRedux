# PERF-D6-2026-09-11-02: read_pod_vec_from_cursor lacks the #[must_use] its NifStream twin carries

URL: https://github.com/matiaszanolli/ByroRedux/issues/4165
Labels: bug, nif-parser, low, tech-debt, nif

---

**Severity**: LOW
**Dimension**: 6 — Allocation Hygiene
**Location**: `crates/nif/src/header.rs:426-449`
**Status**: NEW

**Description**: `read_pod_vec_from_cursor` (header parser) lacks the `#[must_use]` attribute its `NifStream` twin (`read_pod_vec`, `crates/nif/src/stream.rs:445`) carries. No live bug — both current call sites (`header.rs:272,296`) correctly bind the result — but the asymmetry means a future caller that forgets to bind the result gets no compiler warning, unlike the stream-side twin.

**Evidence**: `stream.rs:445` carries `#[must_use = "read_pod_vec returns a populated Vec; bind it or call stream.skip() to advance the cursor without reading"]`; the equivalent attribute is absent from `header.rs:426`'s `read_pod_vec_from_cursor`.

**Impact**: None today — both call sites already bind the result. Latent hygiene gap only.

**Suggested Fix**: Add the matching `#[must_use]` attribute to `read_pod_vec_from_cursor`, mirroring its `NifStream` twin.

## Completeness Checks
- [ ] none — trivial attribute addition, no behavioral test applies

