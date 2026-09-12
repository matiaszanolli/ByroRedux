# NIF-D3-2026-09-11-02: nif-parser.md's dispatch-arm/type-name counts are internally inconsistent and stale against the fresh 255/312 count

URL: https://github.com/matiaszanolli/ByroRedux/issues/4162
Labels: documentation, nif-parser, low, nif, doc-rot

---

**Severity**: LOW
**Dimension**: 3 — Block Dispatch Coverage
**Location**: `docs/engine/nif-parser.md:305-306,658`
**Status**: NEW

**Description**: `docs/engine/nif-parser.md`'s dispatch-arm/type-name counts are internally inconsistent across sections and stale against the fresh 255/312 count measured this session (`crates/nif/src/blocks/mod.rs:312-1365`, top-level arms only). One section says "~248/~315", another says "254/310" per this audit's cross-reference, and a third count ("251/315") appears in the 2026-07-25 report. The doc also undercounts the nested `type_name_static` re-derivation match blocks as "two small nested match blocks" — the actual count is four.

**Evidence** (`nif-parser.md:305-306`):
```
carries ~248 top-level match arms covering ~315 distinct type-name literals
```
Fresh count this session: 255 top-level arms / 312 distinct type-name literals; 4 nested `type_name_static` re-derivation matches, not 2.

**Impact**: Documentation drift only — no parser behavior affected. A reader trusting the doc's counts for a coverage sanity check gets a wrong baseline.

**Suggested Fix**: Refresh both counts to 255/312 and correct "two small nested match blocks" to four; note the FO76 coverage row (98.18%, "pending #3461") is also stale — `#3461` is confirmed closed at 100.0000% coverage — and refresh in the same pass.

## Completeness Checks
- [ ] none — documentation-only fix, `bug` → `documentation` + `doc-rot`

