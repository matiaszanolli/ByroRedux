# TD9-001: NIF heap-allocation regression test never runs in CI (dhat-heap feature dormant)

_Filed 2026-06-26 as #1763 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1763` for live state)._

**Severity**: LOW · **Dimension**: 9 — Test Hygiene
**Location**: `crates/nif/tests/heap_allocation_bounds.rs:30` (`#![cfg(feature = "dhat-heap")]`); `crates/nif/Cargo.toml:28`; `.github/workflows/ci.yml:31,59`
**Status**: NEW · **Audit**: TD9-001

## Description
The entire NIF heap-budget regression file is module-gated on the opt-in `dhat-heap` feature. The file's own header states: *"This test promotes the verification from audit-cadence to CI-cadence … CI should run this alongside the default test job."* But `ci.yml` runs only `cargo test --workspace` with **default** features — and `dhat-heap` is not a default feature — so neither `parse_skyrim_se_single_node_stays_within_heap_budget` nor `parse_skyrim_se_geometry_particle_stays_within_heap_budget` ever execute in CI.

## Impact
The two tests pin 4 allocation-hygiene fixes (#832/#833/#831/#408). A future block-parser change that re-introduces an `or_insert(name.to_string())`-class allocation (#832) or drops a `read_pod_vec` (#833) will NOT fail CI. The regression test's stated CI-cadence purpose is unmet; it only fires on a manual `cargo test -p byroredux-nif --features dhat-heap`.

## Suggested Fix
Add a dedicated CI step: `cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds` (its own job — dhat installs a `#[global_allocator]` and must not share a process with the rest of the suite). Alternatively, fix the file comment if CI execution is intentionally out of scope (doc and workflow currently disagree).

## Completeness Checks
- [ ] **TESTS**: the new CI job runs the two heap-budget tests and they pass
