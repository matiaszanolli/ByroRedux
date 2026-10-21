=================== #5764
PERF-2026-10-13-VM-02: interpreter deferral costs ~0.8 µs a delivery, and a value-called function or array-holding frame cannot suspend
STATE: OPEN  LABELS: 
**Follow-up to #5763** (the frame budget). Numbers: `docs/audits/BENCH_vm-cost_after-5763.txt`; repro `cargo test -p byroredux-scripting --release --lib -- --ignored --nocapture vm_cost`.

## What #5763 left
1. **Deferral is not free.** A delivery the frame budget cannot start is queued on `FragmentExecutionQueue`: ~0.8 µs each (the `Delivery`, an `Effect::Interpret`, a `ScriptInstanceData` clone for the saved continuation, the `OnInit` flag's interned key and variable write). A 5 000-reference `OnCellLoad` burst still costs 6.9 ms in the frame it is raised (14.9 ms before the budget); 1 000 references 1.7 ms. Real cells measured 100–500 scripted references. A compact deferred-delivery record (owner, script, event, args — the VMAD resolved through `ReferenceScriptIndex` at run time) would cut it, but it has to be saved like the continuation queue, so it is a format change.
2. **Unsplittable runs.** A statement inside a function called for its value cannot suspend (a suspended frame cannot hand a value back), and neither can a run whose frames hold an array or struct local (`VmStored` is scalar-only — the same limit `Utility.Wait` has). Such a run finishes in the frame it started, up to the 100 000-statement cap (13.6 ms), and is charged to the allowance so the work behind it defers. Fix options: store arrays in `VmFrame`, or let a value-called function's loop yield by suspending the whole chain back to the call expression.
3. `reference_script_event_system` still walks every attached script each frame to look for markers: 2.8 ns each (14 µs at 5 000, 56 µs at 20 000). A marker-driven walk (entities with an event component) would drop it to the event count.

None of these is reachable in vanilla content today at a cost above ~2 ms; filed so the limits are tracked, not forgotten.
