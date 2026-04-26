# Issue #689 Investigation — KF importer NiSequenceStreamHelper path

**Date**: 2026-04-26
**Investigator**: Claude Opus 4.7

## Summary

The audit's premise — "every Oblivion KF parses to zero clips" — is **empirically wrong**. Across 47,933 scanned vanilla NIF files from three Bethesda games (Oblivion, FNV, Skyrim SE), there are **zero NiSequenceStreamHelper blocks**. Every animated NIF/KF in vanilla content uses NiControllerSequence, which is already handled by Path 2 of `import_kf`.

## Empirical Survey

Scanned every NIF + KF in the meshes BSA of three games using a throwaway example (`crates/nif/examples/scan_ssh.rs`, removed after run):

| Game | Archive | scanned | with NiSequenceStreamHelper | with NiControllerSequence |
|------|---------|---------|-----------------------------|---------------------------|
| Oblivion | `Oblivion - Meshes.bsa` | 9,874 | **0** | 2,244 |
| FNV | `Fallout - Meshes.bsa` | 19,197 | **0** | 5,171 |
| Skyrim SE | `Skyrim - Meshes0.bsa` | 18,862 | **0** | 1,051 |
| **Total** | | **47,933** | **0** | **8,466** |

(FO3 not on this dev box; Skyrim SE result is consistent with the 04-22 audit's framing of NiSequenceStreamHelper as "pre-Skyrim". FO4 / Starfield not surveyed but those use HKX/BHKX, not legacy KF chains.)

## What this means

NiSequenceStreamHelper is the Morrowind / NetImmerse-era animation root. Bethesda kept the block type parseable in the Gamebryo runtime through Skyrim for backwards-compat with **legacy mod content** (Morrowind ports, very-early-era plugins), but no shipped vanilla NIF references it. Our parser correctly accepts it (`controller.rs:1840-1859`) so files containing it don't hard-fail; the importer doesn't consume it because no real content we have exercises that path.

## Audit cross-reference

This is the same class of stale finding flagged in user memory `feedback_audit_findings.md`: "~5 of 30 audit findings in 2026-04 sweep were stale." The 04-17 audit (re-verified by 04-25 audit Dim 6 O6-N-01) inferred a problem from the existence of an unimplemented importer arm, without checking whether vanilla content actually exercises that arm.

The audit chain that produced #689:
- Audit prompt referenced "every Oblivion door idle / creature idle / NPC walk cycle" as broken.
- But Anvil Heinrich Oaken Halls already animates correctly per the README — because Oblivion vanilla uses NiControllerSequence, which Path 2 of `import_kf` handles.
- The "comment in controller.rs:1835-1838 says 'we don't currently consume this'" is technically true but addresses a non-existent need.

## Recommendation

**Don't implement a NiSequenceStreamHelper importer path right now.**

Instead:
1. Update the misleading comment at `crates/nif/src/blocks/controller.rs:1828-1838` to document that vanilla content doesn't use this block, parser-side support is for Morrowind compat / mod content, and an importer path can be added when Morrowind support lands.
2. Add a regression test pinning the empirical zero-count (so a future stale-audit doesn't re-discover this).
3. Close #689 as **stale audit premise** with a link to this investigation.
4. Keep the door open for actual Morrowind support — the importer arm would be 1-2 days of work *when* there's real content to test it against.

Filing this as the fix; the actual code change is small (comment update + a doc-test that asserts the empirical state).
