use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;
use env_logger::Builder;
use log::{error, info, Level, LevelFilter};
use mpatch::{apply_patch, parse_diffs};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

const DEFAULT_FUZZ_THRESHOLD: f32 = 0.7;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Apply diff hunks from a file to a target directory based on context, ignoring line numbers.",
    long_about = "Uses fuzzy matching if exact context fails. Parses unified diffs inside ```diff markdown blocks."
)]
struct Args {
    /// Path to the input file containing ```diff blocks.
    input_file: PathBuf,

    /// Path to the target directory to apply patches.
    target_dir: PathBuf,

    #[arg(short = 'n', long, help = "Show what would be done, but don't modify files.")]
    dry_run: bool,

    #[arg(
        short = 'f',
        long,
        default_value_t = DEFAULT_FUZZ_THRESHOLD,
        help = "Similarity threshold for fuzzy matching (0.0 to 1.0). Higher is stricter. 0 disables fuzzy matching."
    )]
    fuzz_factor: f32,

    /// Increase logging verbosity. Can be used multiple times (e.g., -v, -vv).
    #[arg(
        short,
        long,
        action = clap::ArgAction::Count,
        long_help = "Increase logging verbosity.\n-v for info, -vv for debug, -vvv for trace.\nBy default, only warnings and errors are shown."
    )]
    verbose: u8,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // --- Logger Initialization ---
    let log_level = match args.verbose {
        0 => LevelFilter::Warn,
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    Builder::new()
        .filter_level(log_level)
        .format(|buf, record| {
            match record.level() {
                Level::Error => writeln!(buf, "{} {}", "error:".red().bold(), record.args()),
                Level::Warn => writeln!(buf, "{} {}", "warning:".yellow().bold(), record.args()),
                Level::Info => writeln!(buf, "{}", record.args()),
                Level::Debug => writeln!(buf, "{} {}", "debug:".blue().bold(), record.args()),
                Level::Trace => writeln!(buf, "{} {}", "trace:".cyan().bold(), record.args()),
            }
        })
        .init();

    if !args.target_dir.is_dir() {
        anyhow::bail!("Target directory '{}' not found or is not a directory.", args.target_dir.display());
    }

    if !(0.0..=1.0).contains(&args.fuzz_factor) {
        anyhow::bail!("Fuzz factor must be between 0.0 and 1.0.");
    }

    let content = fs::read_to_string(&args.input_file)
        .with_context(|| format!("Failed to read input file '{}'", args.input_file.display()))?;

    let all_patches = parse_diffs(&content)?;

    if all_patches.is_empty() {
        println!("No valid diff blocks found or processed in the input file.");
        return Ok(());
    }

    println!(); // Vertical spacing
    info!("Found {} patch operation(s) to perform.", all_patches.len());
    if args.fuzz_factor > 0.0 {
        info!("Fuzzy matching enabled with threshold: {:.2}", args.fuzz_factor);
    } else {
        info!("Fuzzy matching disabled.");
    }

    let mut success_count = 0;
    let mut fail_count = 0;

    for (i, patch) in all_patches.iter().enumerate() {
        println!(); // Vertical spacing
        info!(">>> Operation {}/{}", i + 1, all_patches.len());
        match apply_patch(patch, &args.target_dir, args.dry_run, args.fuzz_factor) {
            Ok(true) => success_count += 1,
            Ok(false) => {
                fail_count += 1;
                error!("--- FAILED to apply patch for: {}", patch.file_path.display());
            }
            Err(e) => {
                // The `fail_count` is not incremented here because the program will
                // exit immediately with an error, and the summary will not be printed.
                error!("--- FAILED with hard error while applying patch for: {}", patch.file_path.display());
                // Propagate hard errors like IO or path traversal
                return Err(e.into());
            }
        }
    }

    println!("\n--- Summary ---");
    println!("Successful operations: {}", success_count);
    println!("Failed operations:     {}", fail_count);
    if fail_count > 0 {
        eprintln!("Review the log for errors. Some files may be in a partially patched state if a later hunk in the same patch failed.");
    }
    if args.dry_run {
        println!("DRY RUN completed. No files were modified.");
    }

    if fail_count > 0 {
        std::process::exit(1);
    }

    Ok(())
}
