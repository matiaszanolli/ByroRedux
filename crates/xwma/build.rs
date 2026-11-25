//! Builds the one FFmpeg decoder the engine links: `wmav2`, statically (xWMA PRD, #7413).
//!
//! The release is not in the repository. This script downloads the pinned tarball, checks its
//! SHA-256, extracts and patches it, configures and builds it out of tree, keeps the two static archives and
//! removes everything else. The wrapper's C shim is compiled against those same headers before
//! cleanup. `third_party/ffmpeg/PATCHES.md` records the release, configure line and licence audit.
//!
//! Build requirements beyond a Rust toolchain: a C compiler, GNU Make, a POSIX shell, `curl`, `patch`, and a
//! `tar` that reads `.xz`. Offline builds: set `XWMA_FFMPEG_TARBALL` to a local copy of the tarball
//! (its digest is still checked). `XWMA_FFMPEG_CACHE` moves the tarball cache. `XWMA_FFMPEG_SOURCE`
//! names an unpacked tree with the streaming patch or an equivalent change: always
//! rebuilt, and never deleted (the LGPL relink path in `PATCHES.md`).

use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const VERSION: &str = "9.1";
const URL: &str = "https://ffmpeg.org/releases/ffmpeg-9.1.tar.xz";
const SHA256: &str = "1a4d394ae63d190a526cd052eb4415c156b5c5e2df37efab41a7de0b8fde2433";

// Keep this in sync with V7 and third_party/ffmpeg/PATCHES.md.
const CONFIGURE_FLAGS: &[&str] = &[
    "--disable-everything",
    "--enable-decoder=wmav2",
    "--disable-asm",
    "--disable-faan",
    "--enable-static",
    "--disable-shared",
    "--disable-programs",
    "--disable-doc",
    "--disable-autodetect",
    "--disable-iconv",
    "--disable-network",
    "--disable-avdevice",
    "--disable-avformat",
    "--disable-avfilter",
    "--disable-swscale",
    "--disable-swresample",
];

const LIBRARIES: [&str; 2] = ["avcodec", "avutil"];
const SHIM: &str = "byro_xwma_shim";
const STREAM_PATCH: &str = "../../third_party/ffmpeg/wmav2-stream-frames.patch";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let shim_source = Path::new("src/wmav2.c");
    println!("cargo:rerun-if-changed={}", shim_source.display());
    println!("cargo:rerun-if-changed={STREAM_PATCH}");
    println!("cargo:rerun-if-env-changed=XWMA_FFMPEG_TARBALL");
    println!("cargo:rerun-if-env-changed=XWMA_FFMPEG_CACHE");
    println!("cargo:rerun-if-env-changed=XWMA_FFMPEG_SOURCE");
    let own_source = env::var_os("XWMA_FFMPEG_SOURCE").map(PathBuf::from);
    if let Some(source) = &own_source {
        println!("cargo:rerun-if-changed={}", source.display());
    }

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let lib = out.join("lib");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let msvc = env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|value| value == "msvc");

    // What the archives were built from; a matching stamp means they are current.
    let shim_digest = Sha256::digest(fs::read(shim_source).expect("read the wmav2 shim"));
    let patch_digest =
        Sha256::digest(fs::read(STREAM_PATCH).expect("read the wmav2 streaming patch"));
    let stamp = format!(
        "ffmpeg {VERSION} {SHA256} msvc={msvc} shim={shim_digest:x} patch={patch_digest:x} {}",
        CONFIGURE_FLAGS.join(" ")
    );
    let stamp_file = lib.join("stamp");
    let archives_present = LIBRARIES
        .iter()
        .chain([&SHIM])
        .all(|name| lib.join(format!("lib{name}.a")).is_file());
    let current = own_source.is_none()
        && archives_present
        && fs::read_to_string(&stamp_file).is_ok_and(|s| s == stamp);

    if !current {
        let work = out.join("ffmpeg-work");
        remove_dir(&work);
        fs::create_dir_all(&work).expect("create FFmpeg work directory");
        let source = match &own_source {
            Some(source) => {
                assert!(
                    source.join("configure").is_file(),
                    "XWMA_FFMPEG_SOURCE {} has no configure script",
                    source.display()
                );
                source.clone()
            }
            None => {
                extract(&fetch_tarball(&out), &work);
                let source = work.join(format!("ffmpeg-{VERSION}"));
                apply_stream_patch(&source);
                source
            }
        };
        build(&work, &source, msvc);
        cc::Build::new()
            .cargo_metadata(false)
            .include(work.join("build"))
            .include(&source)
            .file(shim_source)
            .out_dir(&work)
            .compile(SHIM);

        remove_dir(&lib);
        fs::create_dir_all(&lib).expect("create library directory");
        for name in LIBRARIES {
            let file = format!("lib{name}.a");
            fs::copy(
                work.join("build").join(format!("lib{name}")).join(&file),
                lib.join(&file),
            )
            .unwrap_or_else(|err| panic!("keep {file}: {err}"));
        }
        let shim_archive = format!("lib{SHIM}.a");
        fs::copy(work.join(&shim_archive), lib.join(&shim_archive)).expect("keep the shim archive");
        fs::write(&stamp_file, &stamp).expect("write the build stamp");
        // The extracted source and every object file go; only the archives and the tarball stay.
        remove_dir(&work);
    }

    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-lib=static={SHIM}");
    for name in LIBRARIES {
        println!("cargo:rustc-link-lib=static={name}");
    }
    if target_os == "windows" {
        println!("cargo:rustc-link-lib=bcrypt");
        println!("cargo:rustc-link-lib=user32");
    } else {
        println!("cargo:rustc-link-lib=m");
        println!("cargo:rustc-link-lib=pthread");
    }
}

fn apply_stream_patch(source: &Path) {
    use std::process::Stdio;

    let patch = fs::File::open(STREAM_PATCH).expect("open the wmav2 streaming patch");
    run(
        Command::new("patch")
            .current_dir(source)
            .args(["--batch", "-p1"])
            .stdin(Stdio::from(patch)),
        "apply the wmav2 streaming patch",
    );
}

/// The verified tarball: the one `XWMA_FFMPEG_TARBALL` names, else the cached download (fetched
/// when absent). Its SHA-256 is checked every time; a bad cached copy is removed and refetched once.
fn fetch_tarball(out: &Path) -> PathBuf {
    if let Some(path) = env::var_os("XWMA_FFMPEG_TARBALL") {
        let path = PathBuf::from(path);
        check_digest(&path).unwrap_or_else(|err| panic!("XWMA_FFMPEG_TARBALL: {err}"));
        return path;
    }
    // The default cache sits beside the profile's `build` directory (target/<profile>/), found by
    // name because Cargo's layout under it has changed between versions; `cargo clean` removes it.
    let cache = env::var_os("XWMA_FFMPEG_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let build_dir = out
                .ancestors()
                .find(|dir| dir.file_name().is_some_and(|name| name == "build"))
                .expect("OUT_DIR is not under a `build` directory");
            build_dir
                .parent()
                .expect("a `build` directory has a parent")
                .join("xwma-ffmpeg-cache")
        });
    fs::create_dir_all(&cache).expect("create the FFmpeg tarball cache");
    let tarball = cache.join(format!("ffmpeg-{VERSION}.tar.xz"));

    if tarball.is_file() {
        match check_digest(&tarball) {
            Ok(()) => return tarball,
            Err(err) => {
                println!("cargo:warning=discarding the cached FFmpeg tarball: {err}");
                let _ = fs::remove_file(&tarball);
            }
        }
    }
    let part = cache.join(format!("ffmpeg-{VERSION}.tar.xz.part"));
    let _ = fs::remove_file(&part);
    run(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location"])
            .args(["--proto", "=https", "--tlsv1.2", "--output"])
            .arg(&part)
            .arg(URL),
        &format!("download {URL} (set XWMA_FFMPEG_TARBALL to build offline)"),
    );
    check_digest(&part).unwrap_or_else(|err| {
        let _ = fs::remove_file(&part);
        panic!("downloaded {URL}: {err}")
    });
    fs::rename(&part, &tarball).expect("move the verified tarball into the cache");
    tarball
}

fn check_digest(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|err| format!("cannot read {}: {err}", path.display()))?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual == SHA256 {
        Ok(())
    } else {
        Err(format!(
            "SHA-256 mismatch for {}: expected {SHA256}, got {actual}",
            path.display()
        ))
    }
}

fn extract(tarball: &Path, work: &Path) {
    let mut tar = Command::new("tar");
    // GNU tar on Windows reads `C:` as a remote host without this.
    #[cfg(windows)]
    tar.arg("--force-local");
    tar.arg("-xJf").arg(tarball).arg("-C").arg(work);
    run(
        &mut tar,
        "extract the FFmpeg tarball (needs a tar that reads .xz)",
    );
}

/// Configure and make `source` out of tree in `work/build`.
fn build(work: &Path, source: &Path, msvc: bool) {
    let build = work.join("build");
    fs::create_dir_all(&build).expect("create FFmpeg build directory");

    let script = source.join("configure");
    // MSYS sh accepts drive-letter paths with forward slashes.
    #[cfg(windows)]
    let script = script
        .to_str()
        .expect("UTF-8 source path")
        .replace('\\', "/");
    let mut configure = Command::new("sh");
    configure
        .current_dir(&build)
        .env("TMPDIR", ".")
        .arg(script)
        .args(CONFIGURE_FLAGS);
    if msvc {
        configure.arg("--toolchain=msvc");
    }
    run(&mut configure, "configure FFmpeg");

    let mut make = Command::new("make");
    make.current_dir(&build);
    // GNU Make can share Cargo's Unix jobserver instead of starting its own pool.
    #[cfg(unix)]
    if let Some(flags) = env::var_os("CARGO_MAKEFLAGS") {
        make.env("MAKEFLAGS", flags);
    }
    run(&mut make, "build FFmpeg");
}

fn remove_dir(path: &Path) {
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => panic!("cannot remove {}: {err}", path.display()),
    }
}

fn run(command: &mut Command, action: &str) {
    let status = command
        .status()
        .unwrap_or_else(|err| panic!("could not {action} with {command:?}: {err}"));
    assert!(status.success(), "could not {action}: {status}");
}
