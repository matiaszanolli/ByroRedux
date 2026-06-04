# Investigation — #1356 D5-01 stale ROADMAP checkbox

**Domain:** documentation / tech-debt

## Finding
ROADMAP carried an unchecked `- [ ] BSBoneLODExtraData has no parser`. The parser
landed in commit `782b7238` (Fix #614, 2026-04-25) — confirmed real (not a stub)
at `crates/nif/src/blocks/extra_data.rs` (`"BSBoneLODExtraData"` arm). The 0/N R3
counts were zero *instances* in vanilla content, not parse failures.

Note: the issue cited ROADMAP.md:726; the line had drifted to **702** (trust the
symbol, not the line — per the audit-publish re-map rule).

## Fix
Flipped the item to `- [x]` with a strike-through + "closed via #614 (782b7238)"
note, matching the ROADMAP closed-item convention.

## Verification
Doc-only (markdown). No build impact; backticked path `extra_data.rs` exists.
