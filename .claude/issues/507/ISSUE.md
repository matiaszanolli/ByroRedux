# Issue #507: Unconditional cmd_bind_pipeline at render-pass begin overridden

Severity: LOW
Location: draw.rs:720-721

## Problem
Unconditional opaque-pipeline bind right after begin_render_pass. Batch
loop's `last_pipeline_key` sentinel forces a rebind on the first batch,
so the initial bind is always discarded.

## Fix
Removed the bind + left a comment explaining why the sentinel + UI
rebind make it unnecessary.
