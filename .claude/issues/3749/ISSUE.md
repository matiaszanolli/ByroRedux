# #3749 — TD9-2026-08-30-01: 80 % of `#[ignore]`s carry no machine-readable reason, and that has already produced two wrong audit baselines (#3440, #3456)

**Labels**: bug, low, tech-debt, test-gap

---

- **Severity**: LOW
- **Dimension**: 9 — Test Hygiene
- **Location**: workspace-wide — the 136 bare `#[ignore]` sites under `crates/` and `byroredux/`
- **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-30.md` (`TD9-2026-08-30-01`), HEAD `64f64480`

## Description

Of **169** `#[ignore]` sites (re-verified at HEAD: 169 total, 33 reason-form), **33 (20 %)**
use the documented reason form `#[ignore = "…"]`; **136 (80 %)** are bare `#[ignore]` with
the gate condition stated only in an adjacent `///` doc comment.

**All 169 were triaged and every one is legitimately gated** — on-disk game corpora, a
Vulkan device, an audio device, a release build, or a one-shot calibration bench. There is
**no `#[ignore]` in this codebase hiding a broken test.** The 14 distinct reason strings in
use are all of the form *"requires FNV BSA — opt in with `--ignored`"* / *"requires an
RT-capable Vulkan device and a display/Xvfb"* / *"needs Skyrim SE game data on disk;
~1 GB resident"*.

**The finding is the inconsistency, not any individual test.**

## Impact — this has already cost the audit suite twice

The reason string is the only form a tool can read; the doc comment is not.

- **#3440** (OPEN, `TD4-2026-08-27-03`) records that `AUDIT_TECH_DEBT_2026-08-24.md`
  published an `#[ignore]` baseline of **171 where the real figure was 121**.
- **#3456** had to widen this dimension's own discovery regex after the bare-`]` pattern
  silently dropped every reason-form test — a **19 % undercount** at the time.

Both are symptoms of the same thing: the population is not uniformly self-describing, so
every count of it is a judgement call.

## Suggested Fix

Convert the 136 bare sites to `#[ignore = "<existing doc-comment reason>"]` — purely
mechanical, the reason text already exists one line above in nearly every case. Then this
dimension's triage becomes
`grep -oE 'ignore = "[^"]*"' | sort | uniq -c` instead of reading 169 doc comments, and a
future `#[ignore]` with no reason becomes a reviewable anomaly. Effort: small.

> ⚠️ Do **not** run the suite with `--ignored` / `--include-ignored` while working this —
> `cargo test -p byroredux-plugin -- --ignored` has OOM-killed sessions on this machine.
> The conversion is a text edit; it needs `cargo check`, not a corpus run.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — all 169 sites, not just the ones this dimension's regex currently matches
- [ ] **TESTS**: A regression test pins this specific fix — a CI check that every `#[ignore]` carries a reason string makes the convention self-enforcing
