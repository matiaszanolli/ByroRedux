# #3850: TD9-2026-09-05-02: At least 101 of 182 `#[ignore]`d real-data tests report a green `ok` when their data is absent — the Rust half of the tree has no skip signal, while the shell half already does

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD9-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `medium,test-gap,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3850 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD9-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.


- **Severity**: MEDIUM
- **Dimension**: Test Hygiene (Dim 9) — `test-gap`
- **Location**: tree-wide; densest clusters at `/mnt/data/src/gamebyro-redux/crates/plugin/tests/parse_real_esm.rs` (19 tests, helper `data_dir`), `/mnt/data/src/gamebyro-redux/crates/plugin/src/esm/cell/tests/integration.rs` (12), `/mnt/data/src/gamebyro-redux/crates/bsa/src/archive/tests.rs` (10), `/mnt/data/src/gamebyro-redux/crates/bsa/tests/ba2_real.rs` (7), `/mnt/data/src/gamebyro-redux/byroredux/src/npc_spawn/tests.rs` (6), `/mnt/data/src/gamebyro-redux/crates/plugin/src/esm/records/tests.rs` (6)
- **Status**: NEW (the surviving half of #3084's premise; sibling of #3003, which fixed exactly this defect on the *shell* gates)
- **Description**: The universal idiom for a data-gated Rust test in this repo is `#[ignore = "needs <GAME> game data on disk"]` **plus** an in-body `eprintln!("… skipping …"); return;`. The `#[ignore]` handles the default lane correctly. But the `--ignored` lane — the *only* lane these tests ever execute in — has no skip result: libtest reports `test … ok`, and without `--nocapture` it swallows the `eprintln!` for passing tests. So on any machine that lacks one title's data (i.e. every machine, for at least some titles), an operator running `cargo test -p byroredux-plugin -- --ignored` reads N passes and learns nothing about which of them touched a byte of real data.

  This is precisely the defect `docs/smoke-tests/README.md` names for the shell gates and forbids there: *"an explicit `SKIP` with exit code `77`, never a pass"*, with `.github/workflows/playable-smoke.yml` turning a 77 into `::error::… skipped because $GAME data is unavailable`. The Rust corpus tests have no equivalent, and **no strict/require mode exists anywhere** — `grep -rnoE 'BYRO(REDUX)?_[A-Z0-9_]*(REQUIRE|STRICT|MUST)[A-Z0-9_]*'` over the tree returns nothing.

  A second-order defect in the same helper compounds it: `data_dir` (`crates/plugin/tests/parse_real_esm.rs`) treats an explicitly-set-but-wrong env var as advisory — it `eprintln!`s "falling back to default" and then reads the **hardcoded `/mnt/data/SteamLibrary/...` path anyway**. An operator who points `BYROREDUX_FNV_DATA` at a modded or DLC-stripped install silently gets results from a different install than the one they named.
- **Evidence**: Programmatic sweep over all 7 310 `#[test]` fns: 182 carry `#[ignore]`; **101** of those contain both a skip-ish diagnostic (`skip` / `not available` / `missing`) and a bare `return;` in the body. 101 is a **floor** — the sweep does not catch `let Ok(..) = .. else { return }` forms with no diagnostic word. Representative shape, `crates/plugin/tests/parse_real_esm.rs`:
  ```rust
  #[ignore = "needs FNV game data on disk"]
  fn fnv_karma_good_global_decodes_float_payload_before_narrowing() {
      let Some(dir) = data_dir("BYROREDUX_FNV_DATA", FNV_FALLBACK) else {
          eprintln!("[FNV/GLOB] skipping: game data unavailable");
          return;                 // ← libtest records `ok`
      };
  ```
  Contrast `docs/smoke-tests/README.md:8`: *"an explicit `SKIP` with exit code `77`, never a pass."*
- **Impact**: Not a coverage gap — a **trust gap**: a green `--ignored` run is not evidence. This repo has already been burned three times by mis-read test signals in this exact area (#3440 and #3456 were both wrong `#[ignore]` baselines published inside an audit report; #3348 was a red `--ignored` lane nobody noticed). The auto-memory note *NIF Corpus Baseline Tests* records the live consequence: "FO76 currently silently RED on NiPSysBlock" — a corpus baseline whose status had to be tracked in a memory file because the test run itself does not say. Worst case, a real-data guard is dropped or broken and the `--ignored` lane stays green for months.
- **Related**: #3084 (the `#[ignore]` half, fixed); #3003 (identical defect on the shell gates, fixed with exit 77 — the precedent); #3348 (red `--ignored` lane on `byroredux-bsa`); #3440 / #3456 / #3749 (the wrong-baseline lineage); memory note *NIF Corpus Baseline Tests*.
- **Suggested Fix**: Introduce one strict switch — e.g. `BYROREDUX_REQUIRE_GAME_DATA=1` — read by the shared resolvers (`data_dir` in `parse_real_esm.rs`, `game_data_dir` in `crates/nif/tests/common/`, and their siblings) so a missing corpus `panic!`s instead of returning. Set it in whatever lane is meant to be authoritative, exactly as `playable-smoke.yml` promotes exit 77 to an error. Separately, make `data_dir` treat an explicitly-set env var as binding: if it names a non-directory, fail rather than silently substituting the hardcoded Steam path.
- **Effort**: **Medium** (one helper each in ~4 resolver sites, then a mechanical sweep of the ~101 call sites to route through them; no test logic changes).

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
