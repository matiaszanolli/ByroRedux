# #5063 — PERF-D7-2026-09-29-03: pre_parse_cell's doc still describes a serial coordinator extract, the exact regression shape #3659 / 67de801f8 removed

**Labels**: low, documentation, doc-rot, performance

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` — finding `PERF-D7-2026-09-29-03`

**Severity**: LOW

**Dimension**: Streaming & Cells

**Location**: `byroredux/src/streaming.rs:1700-1703`. It is contradicted by `:1870-1880` and by `pre_parse_one`.

**Status in report**: NEW. `7a5cfa7d5` fixed the body comment but not the item doc.

## Description / Impact

the doc says the coordinator "extracts bytes serially through the archive provider while `parse_nif_pipeline` parses". The code admits against `STREAM_PARSE_INPUT_BYTES` and extracts inside each pool task. A maintainer following the doc could restore the serial extract. Doc only.

## Suggested Fix

reword to "admits each input against the decoded-input budget; each pool task extracts and parses its own input".

Validated at HEAD 9fcfdc3fc: `pre_parse_cell`'s item doc (`byroredux/src/streaming.rs`) still says it "extracts bytes serially through the archive provider while `parse_nif_pipeline` parses"; the body admits against `STREAM_PARSE_INPUT_BYTES` and extracts inside each pool task.
