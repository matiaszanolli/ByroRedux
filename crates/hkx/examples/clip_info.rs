//! Print a spline-compressed animation packfile's duration, track count and
//! annotation tracks.
//!
//! ```text
//! cargo run -p byroredux-hkx --example clip_info -- <file.hkx>
//! ```

fn main() {
    let path = std::env::args().nth(1).expect("a packfile");
    let bytes = std::fs::read(&path).expect("read");
    match byroredux_hkx::decode_spline_animation(&bytes) {
        Ok(animation) => {
            println!("{animation:#?}");
            ().to_owned()
        },
        Err(error) => println!("not a spline animation: {error}"),
    }
}
