use crate::{load_manifest, run_manifest, save_manifest, Manifest, RunOptions, SourceStack};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Archive-aware, reference-guided texture-set upscaling")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Discover conservative Bethesda texture sets and write an editable manifest.
    Discover {
        /// Loose directory, BSA, or BA2 source. Repeat in load order; later wins.
        #[arg(long = "source", required = true)]
        sources: Vec<PathBuf>,
        /// Manifest to create.
        #[arg(long)]
        manifest: PathBuf,
        /// Learned reference upscale factor.
        #[arg(long, default_value_t = 4)]
        scale: u32,
    },
    /// Run the learned reference upscale and guided companion-map pass.
    Run {
        /// Loose directory, BSA, or BA2 source. Repeat in load order; later wins.
        #[arg(long = "source", required = true)]
        sources: Vec<PathBuf>,
        /// Reviewed texture-set manifest.
        #[arg(long)]
        manifest: PathBuf,
        /// Output root. Assets are written as lossless PNG intermediates.
        #[arg(long)]
        output: PathBuf,
        /// Validate inputs, space, and commands without writing or invoking the model.
        #[arg(long)]
        dry_run: bool,
        /// Replace existing generated PNGs.
        #[arg(long)]
        overwrite: bool,
    },
}

/// Run the offline texture workbench from either its dedicated binary or the
/// `byroredux texture-upscale` multicall entry point.
pub fn run_cli_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args).unwrap_or_else(|error| error.exit());
    match cli.command {
        Commands::Discover {
            sources,
            manifest,
            scale,
        } => {
            let sources = SourceStack::open(&sources)?;
            let manifest_data = Manifest::discovered(scale, sources.list_files());
            save_manifest(&manifest, &manifest_data)?;
            println!(
                "wrote {} texture sets to {}",
                manifest_data.sets.len(),
                manifest.display()
            );
        }
        Commands::Run {
            sources,
            manifest,
            output,
            dry_run,
            overwrite,
        } => {
            let sources = SourceStack::open(&sources)?;
            let manifest_data = load_manifest(&manifest)?;
            let report = run_manifest(
                &sources,
                &manifest_data,
                RunOptions {
                    output_root: &output,
                    dry_run,
                    overwrite,
                },
            )
            .with_context(|| format!("run texture manifest {}", manifest.display()))?;
            if dry_run {
                println!("planned {} texture sets", manifest_data.sets.len());
            } else {
                println!(
                    "upscaled {} texture sets into {}",
                    report.sets.len(),
                    output.display()
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_cli_accepts_the_main_binary_subcommand_shape() {
        let cli = Cli::try_parse_from([
            "byroredux texture-upscale",
            "discover",
            "--source",
            "textures.bsa",
            "--manifest",
            "sets.toml",
        ])
        .unwrap();
        assert!(matches!(cli.command, Commands::Discover { scale: 4, .. }));
    }
}
