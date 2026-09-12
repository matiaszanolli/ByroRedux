# TD7-001: `BLOOM_BYTES_PER_PIXEL_X1024`'s geometric-series literal is not derived from or tested against `BLOOM_MIP_COUNT`

Labels: medium,tech-debt,renderer,bug

**Description**: `pub const BLOOM_BYTES_PER_PIXEL_X1024: u32 = (341 + 340) * 4 * super::sync::MAX_FRAMES_IN_FLIGHT as u32;` — the `341`/`340` are a hand-computed geometric sum correct only for the current `BLOOM_MIP_COUNT = 5` (declared two lines below). Unlike its sibling `CAUSTIC_BYTES_PER_PIXEL` in `caustic.rs` (explicitly derived from live constants and pinned by a test per #2679), this constant bakes the mip-count-dependent result in as a bare literal with no test — grep confirms no test in `bloom.rs` references it. Feeds `acceleration/predicates.rs`'s VRAM reservation-floor computation; a future `BLOOM_MIP_COUNT` bump would silently mis-report VRAM headroom rather than failing a test.

**Evidence**:
`crates/renderer/src/vulkan/bloom.rs:91-92`.

**Impact**: A future change to `BLOOM_MIP_COUNT` would silently desync this VRAM-reservation constant from reality, mis-reporting headroom to `acceleration/predicates.rs`'s admission logic rather than failing a test.

**Related**: #2679 (the `caustic.rs` precedent this should mirror).

**Suggested Fix**: Either replace the literal with a `const fn` deriving the geometric sum from `BLOOM_MIP_COUNT`, or keep the literal but add a test recomputing the series from `BLOOM_MIP_COUNT` and asserting equality, mirroring `caustic.rs`'s #2679 precedent.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
