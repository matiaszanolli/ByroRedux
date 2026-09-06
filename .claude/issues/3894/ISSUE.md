# #3894: TD9-2026-09-05-04: Five `recon`-gated `crates/spt` example binaries have no compile gate in any lane

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD9-2026-09-05-04) via `/audit-publish`, 2026-09-05. Labels: `low,speedtree,test-gap,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3894 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD9-2026-09-05-04), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.


- **Severity**: LOW
- **Dimension**: Test Hygiene (Dim 9) — `test-gap`
- **Location**: `/mnt/data/src/gamebyro-redux/crates/spt/Cargo.toml` (five `[[example]]` targets with `required-features = ["recon"]`), sources under `/mnt/data/src/gamebyro-redux/crates/spt/examples/`
- **Status**: NEW
- **Description**: `byroredux-spt` declares `default = []` and `recon = []`, and gates `spt_recon`, `spt_dissect`, `spt_tagmap`, `spt_transitions`, `spt_walk` behind `required-features = ["recon"]`. Cargo skips a target whose required features are off, and no CI job passes `--features recon` — so these five reverse-engineering harnesses are never type-checked. They are the tooling that produced the SpeedTree tag dictionary (`6f83b1c3`), i.e. the artifacts a future `.spt` investigation would reach for first.

  I verified they are **not currently broken**: `cargo check -p byroredux-spt --features recon --examples` finishes clean. This is therefore a hardening finding, not a live breakage — but nothing prevents the next `crates/spt` or `crates/bsa` API change from silently rotting them.

  The `recon` feature comment anticipates "the future format-discovery integration tests"; none exist yet, so no *test* is currently dark behind a never-enabled feature. That triage bullet is otherwise clean this cycle (see Verified Clean).
- **Evidence**: `crates/spt/Cargo.toml` lines declaring the five `[[example]]` blocks; `grep -rn 'features' .github/workflows/ci.yml` shows only `--features dhat-heap` (a dedicated job), never `recon`; `cargo check -p byroredux-spt --features recon --examples` → `Finished dev profile ... in 5.64s`.
- **Impact**: Silent bit-rot of dev tooling for an already-thinly-owned crate (`/audit-speedtree` owns the parser; the examples are effectively unowned). Discovered only when someone needs them, which is exactly when they are least welcome to fix.
- **Related**: `/audit-speedtree`; the `dhat-heap` job in `ci.yml` is the in-repo precedent for gating a non-default feature.
- **Suggested Fix**: Add `cargo check -p byroredux-spt --features recon --examples` to the existing `ci.yml` clippy/check job — one line, seconds of wall time, no new job.
- **Effort**: **Trivial**.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
