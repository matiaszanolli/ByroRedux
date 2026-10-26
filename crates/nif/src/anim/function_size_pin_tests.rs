//! Size pins for the embedded-animation importer. (#6143, regression of
//! #5096.)
//!
//! `import_embedded_animations` was 433 lines at nesting 9 before the
//! per-controller dispatch moved into `import_embedded_controller`; the pins
//! hold both at their post-split size, so a new controller family goes in as
//! its own helper rather than a raised pin.

use byroredux_core::source_scan::function_line_count;

#[test]
fn embedded_animation_importer_stays_within_its_size_pins() {
    let src = include_str!("entry.rs");
    for (declaration, pin) in [
        ("fn import_embedded_animations(", PIN_DRIVER),
        ("fn import_embedded_controller(", PIN_CONTROLLER),
    ] {
        let measured = function_line_count(src, declaration);
        assert!(
            measured <= pin,
            "`{declaration}` carries {measured} lines (pin {pin}) — move the new work into a \
             helper rather than raising the pin (#6143)"
        );
    }
}

const PIN_DRIVER: usize = 145;
const PIN_CONTROLLER: usize = 305;
