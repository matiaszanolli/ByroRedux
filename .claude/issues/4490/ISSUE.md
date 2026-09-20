# EXT-D7-2026-09-19-03: no harness guards the stale-binary/stale-SPIR-V trap before judging shader claims

- **ID**: EXT-D7-2026-09-19-03
- **Labels**: medium,tech-debt,shaders,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4490

**Severity**: MEDIUM · **Dimension**: Acceptance harness · **Tier Violated**: no-fabrication (a capture attributed to shader X may be shader X-1) · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-03)

**Location**: `docs/smoke-tests/m-exteriors.sh:89-92`; `docs/smoke-tests/w1-water-traversal.sh:89-92`; `scripts/renderer-eval-groundcover.sh:80`; trap documented at `docs/engine/skyal.md` §4

**Description**
skyal.md §4 records the measured trap: "A recompiled `.spv` does not reliably trigger a `cargo` rebuild. … captures silently ran a stale binary and produced a ~6.9/255 mean 'regression' that did not exist." None of the four harnesses encode the documented fix (`touch crates/renderer/src/lib.rs` after `glslangValidator`) and none recompile shaders. Worse, m-exteriors and w1 skip `cargo build` entirely when `target/release/{byroredux,byro-dbg}` already exists — so even plain Rust-side source edits (`shader_constants_data.rs`) are not picked up. A sky/water/ground-cover smoke can pass against stale SPIR-V and stale code.

**Impact**
A shader "fix" validated by a smoke run may be validated against the old shader; a regression can be "confirmed absent" the same way. This is the exact phantom-regression class skyal.md already hit once.

**Related**: EXT-D7-2026-09-19-01; existing mitigations (CI `shader-artifacts` job catches committed `.spv` drift only) are not harness-side

**Suggested Fix**
In each script, replace the `-x` existence check with `cargo build --release -p byroredux -p byro-dbg` (cheap when fresh), and after any shader edit perform (or instruct) `touch crates/renderer/src/lib.rs` before the build, per skyal.md.

## Completeness Checks
- [ ] **SIBLING**: `crates/renderer/build.rs` could emit `rerun-if-changed` for `shaders/**` as a structural fix
- [ ] **TESTS**: A stale-artifact drill: edit a shader constant, run a smoke, assert the change is visible
