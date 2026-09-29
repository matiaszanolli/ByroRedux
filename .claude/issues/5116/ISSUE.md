# TD9-2026-09-29-02: #2835's FSR bench-report self-test has never run in CI because the shader job's container has no python3

**Labels**: low,renderer,shaders,tech-debt,test-gap,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 9 · **Status**: NEW · **Effort**: trivial · **Kind**: test-gap
- **Location**: `.github/workflows/ci.yml:98-120`. The job runs in `container: ubuntu:26.04` and its apt
  list is ca-certificates, git, glslang-tools and ripgrep. The failing step is "FSR bench report reads
  both TSV schemas".
- **Evidence**:
  - The HEAD job log shows `python3: not found` → exit 127.
  - The step also failed in the sampled runs from 09-02, 09-09, 09-15 and 09-25.
  - The step was added by `4de5e78ee` (08-14); the container has been in place since `ca7a4e0ea` (07-25).
  - Locally: `ok — fsr_bench_report self-test passed (7 schemas)`.
  - The shader recompile/compare step passes. The job is red only because of this step.
- **Impact**:
  - #2835's guard has had no CI coverage for 6 weeks.
  - Shader parity has been permanently red for a non-shader reason, so a real SPIR-V drift would land
    on an already-red check.
- **Suggested Fix**: add `python3` to the apt list, or move the step into its own job.

**Validated at HEAD 9fcfdc3fc**: `.github/workflows/ci.yml` shader-artifacts job (`container: ubuntu:26.04`) installs ca-certificates, git, glslang-tools, ripgrep only, then runs `python3 scripts/fsr_bench_report.py --self-test`; HEAD run 36609043348 "Shader source/artifact parity" failed with `python3: not found`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
