# #3719: NIF-2026-08-30-D2-02: three NifVersion constants have no call site, including a V20_2_0_7_SSE alias whose only reference is a test asserting it equals V20_2_0_7

**Labels**: bug, nif-parser, low, tech-debt, nif
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_NIF_2026-08-30.md` · **Severity**: LOW · **Dimension**: Version Gating
**Game affected**: none (dead code)

## Location
- `crates/nif/src/version.rs` — `V10_1_0_112` (`:109`), `V20_2_0_7_SSE` (`:150`), `V30_1_0_1` (`:160`); the self-referential assertion at `:848`

## Description
`V20_2_0_7_SSE` is defined as `Self(0x14020007)` — bit-identical to `V20_2_0_7`. Its only reference anywhere in the workspace is `assert_eq!(NifVersion::V20_2_0_7, NifVersion::V20_2_0_7_SSE)`, a test that can only fail if someone edits the constant it exists to alias.

Two names for one value invite a future gate written against the "SSE" spelling in the belief that it discriminates Skyrim SE from LE — **it does not**: the corpus census confirms SE and LE share version 20.2.0.7 and differ only by `bsver` 100 vs 83. `V10_1_0_112` and `V30_1_0_1` are likewise call-site-less.

## Evidence
Exhaustive workspace scan re-run 2026-08-30:
```
V20_2_0_7_SSE  -> version.rs:150 (definition), version.rs:848 (the tautological assert)
V10_1_0_112    -> version.rs:109 (definition only)
V30_1_0_1      -> version.rs:160 (definition only)
```
Zero references outside `version.rs` for all three; non-zero for every other `pub const V*`.

## Impact
No runtime effect. Same dead-constant class that #1511, #1840 and #1897 each had to remove once already; the aliasing case is actively misleading rather than merely unused.

## Related
#1511, #1840, #1897; cross-domain with `/audit-tech-debt`.

## Suggested Fix
Delete `V20_2_0_7_SSE` and its tautological assertion; keep `V10_1_0_112` / `V30_1_0_1` only if a documented near-term consumer exists.

## Completeness Checks
- [ ] **SIBLING**: re-scan every `pub const V*` for call sites in the same pass so a fourth dead constant is not left behind
- [ ] **TESTS**: removing the tautological assert must not reduce real coverage — confirm no other test depended on it
