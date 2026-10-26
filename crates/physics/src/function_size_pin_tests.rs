//! Size pin for the buoyancy driver. (#6143, regression of #5096.)
//!
//! `apply_buoyancy_with_scratch` is one straight-line pass over every body in
//! water and was not split by #6143; it is pinned at its size at the time so
//! growth is a deliberate edit — a new addition belongs in a helper in
//! `water.rs`, not in a raised pin.

use byroredux_core::source_scan::function_line_count;

#[test]
fn buoyancy_driver_stays_within_its_size_pin() {
    let measured = function_line_count(include_str!("water.rs"), "fn apply_buoyancy_with_scratch(");
    assert!(
        measured <= PIN,
        "`apply_buoyancy_with_scratch` carries {measured} lines (pin {PIN}) — move the new work \
         into a helper rather than raising the pin (#6143)"
    );
}

const PIN: usize = 710;
