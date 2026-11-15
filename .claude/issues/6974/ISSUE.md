# #6974: TD3-2026-11-15-09: history-narrating comments (#6719 rule) concentrate in three files touched in the window

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6974
- **Labels**: low,renderer,esm-plugin,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: small (one file per commit, comment-only diffs). **Labels**: `tech-debt`, `doc-rot`. **Existing**: none (#6719 is the rule; the first pass was `skinned_blas_refit.rs`).
- **Method**: of 817 `.rs` files touched in 670f81917..HEAD, counted comment lines with "used to", "pre-#N", "previously", "formerly", "was replaced", "before this fix" and similar; the three below are the worst real ones (counts are matching comment lines at HEAD; `resize.rs` and `shader_contract_tests.rs` scored high on noise such as "the old surface" and were not counted).
- **Evidence**:
  - `crates/renderer/src/vulkan/context/frame_params.rs` (17): `:108-120` ("the VP-matrix limb used to BE the cut signal ... It was replaced by two signals"), `:184` ("already had before this fix"), `:219` ("the path it used before #2219"), `:398` ("comment previously claimed Halton(2)/Halton(3)...").
  - `crates/core/src/ecs/components/material.rs` (22): `:269-273` ("pre-#398 it was extracted but never reached the GPU", "the value used pre-#398"), `:364-368` ("this doc used to justify that shape by citing ... At the time there was no such field"), `:502-505` ("this doc used to teach that the default was `0`").
  - `crates/plugin/src/esm/records/index.rs` (12): `:4-5` ("Lifted out of the pre-#1118 monolithic `records/mod.rs`"), `:213` ("The pre-#5644 one-INFO-at-a-time fold"), `:334`, `:358` ("pre-#458 fell through").
  - Same pattern in `crates/nif/src/anim/keys.rs:190-203` (TD3-07).
- **Fix**: per file, rewrite each block to the rule the code follows today plus the issue number; check for tests that pin comment text first (the skill lists the known ones). Not a bulk sweep.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
