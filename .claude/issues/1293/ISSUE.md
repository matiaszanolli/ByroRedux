Follow-up to closed [#1291](https://github.com/matiaszanolli/ByroRedux/issues/1291).

## Issue

#1291 added \`XCLL_SIZES_STARFIELD = [28, 108]\` to stop the per-cell warn spam. The existing dispatch arm at \`crates/plugin/src/esm/cell/walkers.rs:392\` decodes through 92 bytes (Skyrim+ 6×RGBA ambient cube + specular + extended fog + light-fade range) correctly — confirmed via \`byro-dbg light.dump\` on a loaded Cydonia cell.

**But the 16 trailing bytes (108 - 92 = 16) are silently dropped.** They're SF-specific extension fields whose layout isn't decoded today. Without the layout, we don't know what authoring intent we're losing.

## Likely candidates

Per the prior 2026-05-28 audit (\`docs/audits/SF_FIRST_RENDER_2026-05-28.md\`):

- New HDR exposure params (Starfield uses physically-based units; pre-FO76 didn't surface them in XCLL)
- Additional fog curve points (denser fog falloff control)
- Post-FO76 directional ambient cube format change (maybe an extra alpha / luminance field per face)
- Volumetric-lighting cell-tint (Starfield's volumetric lighting M55 sibling)
- Per-cell skydome blend factor

## Suggested investigation

1. Cross-reference xEdit's \`wbDefinitionsSF1.pas\` for the XCLL record definition — this is the canonical reverse-engineered reference but isn't cloned locally. Either grab the xEdit Starfield branch or read the upstream online.
2. Compare 5-10 Cydonia cells' XCLL tail bytes (offsets 92-107) across known-different-lighting cells (e.g., \`citycydoniamainlevel\` vs \`citycydoniamainlevel02\` vs a Va'ruun temple). Patterns will surface field boundaries.
3. Extend \`crates/plugin/examples/sf_parse_check\` with an XCLL-tail-dump mode that prints the 16-byte tail as f32 / u32 / RGBA candidates for a chosen cell.
4. Once layout is known, extend \`CellLighting\` struct + decoder arm to consume the SF tail.
5. Renderer-side consumer wiring is a separate scope (mirrors #1289 / #1292 pattern: parser-landed-consumer-unwired).

## Severity / urgency

LOW — \`#1291\` already stopped the warn spam and the Skyrim+ 92-byte body decode is correct, so cells are USABLE today (lighting plausibly matches Bethesda's authoring through the documented fields). Decoding the SF-specific tail is a "fidelity polish" task, not a blocker for visible Cydonia rendering. The actual blocker for Cydonia visuals is [#1292](https://github.com/matiaszanolli/ByroRedux/issues/1292) (NIF geometry drop, 99.7% of REFRs).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: same XCLL extension may apply to FO76 future content updates — verify against latest FO76 ESMs if they author 108-byte XCLL (they shouldn't per current evidence, but the upstream Creation Engine may have backported the SF extension)
- [ ] **TESTS**: regression test that a known SF cell's authored XCLL tail values reach \`CellLighting\` (whatever new fields the layout reveals)

## References

- Parent issue (closed): [#1291](https://github.com/matiaszanolli/ByroRedux/issues/1291)
- First-render audit: [docs/audits/SF_FIRST_RENDER_2026-05-28.md](docs/audits/SF_FIRST_RENDER_2026-05-28.md)
- XCLL dispatch: [crates/plugin/src/esm/cell/walkers.rs:392-497](crates/plugin/src/esm/cell/walkers.rs#L392-L497)
- Canonical sizes: [crates/plugin/src/esm/cell/walkers.rs:21-25](crates/plugin/src/esm/cell/walkers.rs#L21-L25)
