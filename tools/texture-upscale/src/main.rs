fn main() {
    if let Err(error) = byro_texture_upscale::run_cli_from(std::env::args_os()) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
