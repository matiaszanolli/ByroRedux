# TD8-004: Dx10Chunk::start_mip now read — #[allow(dead_code)] redundant; end_mip set-never-read

_Filed 2026-06-26 as #1761 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1761` for live state)._

**Severity**: LOW · **Dimension**: 8 — Dead Code
**Location**: `crates/bsa/src/ba2.rs:144-151`
**Status**: NEW · **Audit**: TD8-004

## Description
The `#[allow(dead_code)]` block at ba2.rs:144-151 reserves both `Dx10Chunk::start_mip` and `end_mip` for M40 streaming (#1049). But `start_mip` is now a **live read** — the monotonic-order validation at ba2.rs:621/626 uses it (`chunks.windows(2).all(|w| w[0].start_mip <= w[1].start_mip)`). So its `#[allow(dead_code)]` is now redundant. `end_mip` is written at ba2.rs:600 and never read back.

## Evidence
`grep -n 'end_mip' ba2.rs` → only def (151) + construction (600), 0 reads. `start_mip` → also read at 621, 626, 630, 635.

## Suggested Fix
Lowest-risk: remove the now-redundant `#[allow(dead_code)]` on `start_mip` (the actionable bit). Keep `end_mip` + its attribute as the documented #1049 M40 reserve, OR delete `end_mip` if M40 will reconstruct chunk bounds on demand.

## Completeness Checks
- [ ] **TESTS**: `cargo build -p byroredux-bsa` clean with `start_mip`'s attribute removed (no dead-code warning ⇒ confirms it's read)
