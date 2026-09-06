# #4024 — REN-2026-09-06-D23-02: ROADMAP's bench tracker still asserts the FSR harness is "byte-stable since `34074b93`" — three commits have touched the two harness files, one of them changing the reporter's arithmetic

**Labels**: low, renderer, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D23-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: FSR/Presentation (doc-rot, measurement integrity)
- **Location**: `ROADMAP.md`, the `R6a-stale-20` tracker entry — the phrase
  "**Harness still confirmed byte-stable**: no commit against
  `scripts/fsr-bench-matrix.sh` or `scripts/fsr_bench_report.py` since
  `34074b93`" in its **2026-09-01 (Session 77, HEAD `f9dd52b4`)** and
  **2026-09-03 (Session 79, HEAD `4d78dce6`)** fold paragraphs
- **Status**: NEW
- **Description**: The claim was true when first written (2026-08-19) and stayed
  true through the 2026-08-28 fold. It became false on 2026-08-28 and was then
  repeated twice. `git log 34074b93..HEAD` on the two files returns three
  commits, all ancestors of both `f9dd52b4` and `4d78dce6`:

  | Commit | Date | Files | Effect |
  |---|---|---|---|
  | `ff177576` | 2026-08-28 | `fsr-bench-matrix.sh` (+94/−2) | bench sanity gates (entity floor, state-hash rejection) |
  | `0e91fc5e` | 2026-08-28 | **both** (+130/−13) | adds the `gpu_inactive` TSV column and changes `fsr_bench_report.py`'s `render_sum` so brackets flagged inactive are **excluded** rather than summed as `0.000` |
  | `1293dfc0` | 2026-08-29 | `fsr-bench-matrix.sh` (+29) | adds the `gridcross` exterior scene definition (deliberately outside the default `SCENES`) |

  The 2026-09-01 paragraph names `0e91fc5e`'s own #2830 in its body ("Session 76
  changes an over-limit FSR render-extent from clamped to rejected") and then
  asserts the harness untouched since `34074b93` — the same commit did both.
- **Evidence**: The harness's own provenance stamp contradicts the claim
  directly. The two archived records:
  ```
  docs/audits/BENCH_stepped-camera_34074b93.tsv
    # harness=4de5e78e engine=34074b93 …          (23 columns, ends state_hash)
  docs/audits/BENCH_stepped-camera_2da754e7.tsv
    # harness=1293dfc0 engine=2da754e7 …          (24 columns, ends gpu_inactive)
  ```
  `git merge-base --is-ancestor` confirms all three harness commits precede
  `2da754e7`, `f9dd52b4` and `4d78dce6`.
- **Impact**: The tracker is the only place in the repo that records whether two
  bench records are comparable, and it currently licenses an apples-to-apples
  read of the 2026-08-14 and 2026-09-03 matrices that is not valid: the column
  set differs, the acceptance gates differ, and `render_sum` — the input to the
  "render rec." column — is computed differently. The practical damage is bounded
  because the **live** bench-of-record section (2026-09-03, `2da754e7`) does the
  right thing independently: it declines old-vs-new attribution outright ("The
  1059-commit gap is too large for an uncontrolled old-vs-new attribution").
  Hence LOW.
- **Related**: #2835 (the harness provenance stamp that makes this checkable),
  `0e91fc5e` (#2821, the `gpu_inactive` change), REN-2026-09-06-D23-03
- **Suggested Fix**: Replace the assertion in the last two fold paragraphs with
  the measured fact — three harness commits, what each changed, and that the two
  archived records therefore carry different `harness=` stamps and are not
  directly comparable. Going forward, derive the sentence from
  `git log <record>..HEAD -- scripts/fsr-bench-matrix.sh scripts/fsr_bench_report.py`
  at fold time rather than carrying it forward verbatim; the fold ritual copied
  this line through five updates unverified.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix
