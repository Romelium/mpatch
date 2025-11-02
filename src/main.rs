use anyhow::{anyhow, Context, Result};
use clap::Parser;
use colored::Colorize;
use env_logger::Builder;
use log::{error, info, warn, Level, LevelFilter};
use mpatch::{apply_patch};
use mpatch::{parse_diffs, Patch};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_FUZZ_THRESHOLD: f32 = 0.7;

// --- Main Application Entry Point ---

fn main() {
    // 1. Parse command-line arguments using `clap`.
    let args = Args::parse();

    // 2. Call the main logic function.
    //    All complex logic and error handling is inside `run`.
    if let Err(e) = run(args) {
        // 3. If `run` returns an error, it has already been logged by the time it gets here
        //    (unless the logger itself failed). We just need to print a user-facing
        //    message and set the exit code.
        //    Using {:?} ensures the full error chain from `anyhow` is printed.
        eprintln!("{} {:?}", "error:".red().bold(), e);
        std::process::exit(1);
    }
}

/// Contains the primary logic of the application.
fn run(args: Args) -> Result<()> {
    // --- Argument Validation ---
    if !args.target_dir.is_dir() {
        return Err(anyhow!(
            "Target directory '{}' not found or is not a directory.",
            args.target_dir.display()
        ));
    }
    if !(0.0..=1.0).contains(&args.fuzz_factor) {
        return Err(anyhow!("Fuzz factor must be between 0.0 and 1.0."));
    }

    // --- File Parsing ---
    // Read the input file and pass its content to the core parsing logic from the library.
    let content = fs::read_to_string(&args.input_file)
        .with_context(|| format!("Failed to read input file '{}'", args.input_file.display()))?;
    let all_patches = parse_diffs(&content)?;

    // --- Setup Logging and Reporting ---
    // This sets up the logger and, if needed, creates a report file.
    // The `_finalizer` is a "drop guard". When it goes out of scope at the end of
    // this function (no matter how it exits), its `drop` method is called,
    // which guarantees the report file is correctly finalized.
    let report_arc = setup_logging_and_reporting(&args, &content, &all_patches)?;
    let _finalizer = ReportFinalizer::new(report_arc);

    // --- Core Patching Logic ---
    if all_patches.is_empty() {
        info!("No valid diff blocks found or processed in the input file.");
        return Ok(());
    }

    let options = mpatch::ApplyOptions {
        dry_run: args.dry_run,
        fuzz_factor: args.fuzz_factor,
    };

    info!(""); // Vertical spacing for readability
    info!("Found {} patch operation(s) to perform.", all_patches.len());
    if options.fuzz_factor > 0.0 {
        info!(
            "Fuzzy matching enabled with threshold: {:.2}",
            options.fuzz_factor
        );
    } else {
        info!("Fuzzy matching disabled.");
    }

    let mut success_count = 0;
    let mut fail_count = 0;

    // Iterate through each parsed patch and apply it.
    for (i, patch) in all_patches.iter().enumerate() {
        info!(""); // Vertical spacing
        info!(">>> Operation {}/{}", i + 1, all_patches.len());
        match apply_patch(patch, &args.target_dir, options) {
            Ok(patch_result) => {
                if let Some(diff) = patch_result.diff {
                    println!(
                        "----- Proposed Changes for {} -----",
                        patch.file_path.display()
                    );
                    print!("{}", diff);
                    println!("------------------------------------");
                }
                if patch_result.report.all_applied_cleanly() {
                    success_count += 1;
                } else {
                    fail_count += 1;
                    error!(
                        "--- FAILED to apply patch for: {}",
                        patch.file_path.display()
                    );
                    log_failed_hunks(&patch_result.report);
                }
            }
            Err(e) => {
                // A "hard" error occurred (e.g., I/O error, path traversal).
                // This is fatal, so we stop and return the error.
                return Err(anyhow::Error::from(e)).with_context(|| {
                    format!(
                        "A fatal error occurred while applying patch for: {}",
                        patch.file_path.display()
                    )
                });
            }
        }
    }

    // --- Final Summary ---
    info!("\n--- Summary ---");
    info!("Successful operations: {}", success_count);
    info!("Failed operations:     {}", fail_count);
    if args.dry_run {
        info!("DRY RUN completed. No files were modified.");
    }

    if fail_count > 0 {
        warn!("Review the log for errors. Some files may be in a partially patched state.");
        // Return an error to set a non-zero exit code.
        return Err(anyhow!(
            "Completed with {} failed patch operations.",
            fail_count
        ));
    }

    Ok(())
}

// --- Helper Structs and Functions ---

/// Logs the reasons why hunks failed to apply.
fn log_failed_hunks(apply_result: &mpatch::ApplyResult) {
    for failure in apply_result.failures() {
        warn!("  - Hunk {} failed: {}", failure.hunk_index, failure.reason);
    }
}

/// Defines the command-line arguments for the application.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Apply diff hunks from a file to a target directory based on context, ignoring line numbers.",
    long_about = "Uses fuzzy matching if exact context fails. Parses unified diffs inside ```diff or ```patch markdown blocks."
)]
struct Args {
    /// Path to the input file containing ```diff blocks.
    input_file: PathBuf,
    /// Path to the target directory to apply patches.
    target_dir: PathBuf,
    /// If set, show what would be done, but don't modify any files.
    #[arg(
        short = 'n',
        long,
        help = "Show what would be done, but don't modify files."
    )]
    dry_run: bool,
    /// The similarity threshold for fuzzy matching (0.0 to 1.0).
    /// Higher is stricter. 0 disables fuzzy matching completely.
    #[arg(short = 'f', long, default_value_t = DEFAULT_FUZZ_THRESHOLD, help = "Similarity threshold for fuzzy matching (0.0 to 1.0). Higher is stricter. 0 disables fuzzy matching.")]
    fuzz_factor: f32,
    /// Increase logging verbosity. Can be used multiple times.
    /// -v for info, -vv for debug, -vvv for trace.
    /// -vvvv also generates a comprehensive debug report file.
    #[arg(short, long, action = clap::ArgAction::Count, long_help = "Increase logging verbosity.\n-v for info, -vv for debug, -vvv for trace.\n-vvvv to generate a comprehensive debug report file.")]
    verbose: u8,
}

/// A "Tee" writer that sends output to both stderr and a shared file.
/// This is used in debug report mode (`-vvvv`) to show logs on the console
/// while also writing them to the report file.
struct TeeWriter {
    file: Arc<Mutex<File>>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Write to standard error first.
        io::stderr().write_all(buf)?;
        // Then write to the locked file.
        self.file.lock().unwrap().write_all(buf)?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()?;
        self.file.lock().unwrap().flush()?;
        Ok(())
    }
}

/// A "Drop Guard" to ensure the report file is always finalized correctly.
/// When an instance of this struct goes out of scope, its `drop` method is
/// called, which writes the closing markdown fence for the log block.
/// This ensures the report is valid even if the program panics or exits early.
struct ReportFinalizer {
    file_arc: Option<Arc<Mutex<File>>>,
}
impl ReportFinalizer {
    fn new(file_arc: Option<Arc<Mutex<File>>>) -> Self {
        Self { file_arc }
    }
}
impl Drop for ReportFinalizer {
    fn drop(&mut self) {
        if let Some(arc) = &self.file_arc {
            log::logger().flush();
            let mut file = arc.lock().unwrap();
            // Use `let _ = ...` to ignore potential write errors during cleanup.
            let _ = writeln!(file, "````");
        }
    }
}

/// Sets up the global logger, creating a report file if verbosity is >= 4.
fn setup_logging_and_reporting(
    args: &Args,
    patch_content: &str,
    patches: &[Patch],
) -> Result<Option<Arc<Mutex<File>>>> {
    let mut builder = Builder::new();
    let report_arc = if args.verbose >= 4 {
        // --- Create and Write Report Header ---
        let arc = create_report_file(args, patch_content, patches)?;
        // --- Configure Logger to Tee to the Report File ---
        builder
            .filter_level(LevelFilter::Trace) // Max verbosity for the report
            .target(env_logger::Target::Pipe(Box::new(TeeWriter {
                file: arc.clone(),
            })));
        Some(arc)
    } else {
        // --- Configure Standard Logger ---
        let log_level = match args.verbose {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            _ => LevelFilter::Trace, // -vvv and higher
        };
        builder.filter_level(log_level);
        None
    };

    // Configure the log format with colors.
    builder
        .format(|buf, record| match record.level() {
            Level::Error => writeln!(buf, "{} {}", "error:".red().bold(), record.args()),
            Level::Warn => writeln!(buf, "{} {}", "warning:".yellow().bold(), record.args()),
            Level::Info => writeln!(buf, "{}", record.args()),
            Level::Debug => writeln!(buf, "{} {}", "debug:".blue().bold(), record.args()),
            Level::Trace => writeln!(buf, "{} {}", "trace:".cyan().bold(), record.args()),
        })
        .init();

    Ok(report_arc)
}

/// Creates the report file, writes the header, and returns a shared pointer to it.
fn create_report_file(
    args: &Args,
    patch_content: &str,
    patches: &[Patch],
) -> Result<Arc<Mutex<File>>> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let report_filename = format!("mpatch-debug-report-{}.md", timestamp);
    let mut file = File::create(&report_filename)
        .with_context(|| format!("Failed to create debug report file '{}'", report_filename))?;

    info!(
        "Debug report mode enabled. Generating comprehensive report to '{}'...",
        report_filename
    );

    // --- Write Metadata ---
    writeln!(file, "# Mpatch Debug Report\n")?;
    writeln!(file, "> **Note:** This report has been partially anonymized. Please review for any remaining sensitive information before sharing.\n")?;
    writeln!(
        file,
        "- **Mpatch Version:** `{}`",
        env!("CARGO_PKG_VERSION")
    )?;
    writeln!(file, "- **OS:** `{}`", std::env::consts::OS)?;
    writeln!(file, "- **Architecture:** `{}`", std::env::consts::ARCH)?;
    writeln!(file, "- **Timestamp (Unix):** `{}`", timestamp)?;

    // --- Write Anonymized Command ---
    writeln!(file, "\n## Command Line\n")?;
    writeln!(file, "```sh")?;
    writeln!(file, "{}", anonymize_command_args(args))?;
    writeln!(file, "```")?;

    // --- Write Input Patch File ---
    writeln!(file, "\n## Input Patch File\n")?;
    writeln!(file, "````markdown")?;
    writeln!(file, "{}", patch_content)?;
    writeln!(file, "````")?;

    // --- Write Original Target Files ---
    writeln!(file, "\n## Original Target File(s)\n")?;
    for patch in patches {
        let target_file_path = args.target_dir.join(&patch.file_path);
        writeln!(file, "### File: `{}`\n", patch.file_path.display())?;
        match fs::read_to_string(&target_file_path) {
            Ok(file_content) => {
                let lang = target_file_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                writeln!(file, "````{}", lang)?;
                writeln!(file, "{}", file_content)?;
                writeln!(file, "````")?;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                writeln!(file, "*File does not exist.*")?;
            }
            Err(e) => writeln!(file, "*Error reading file: {}*", e)?,
        }
    }

    // --- Prepare for Log ---
    writeln!(file, "\n## Full Trace Log\n")?;
    writeln!(file, "````log")?;

    // Return a thread-safe, reference-counted pointer to the file.
    Ok(Arc::new(Mutex::new(file)))
}

/// Replaces sensitive paths in command line arguments with placeholders.
/// This helps protect user privacy when sharing debug reports.
fn anonymize_command_args(args: &Args) -> String {
    let mut anonymized_args = Vec::new();
    let mut args_iter = std::env::args();
    // The first argument is always the program name.
    anonymized_args.push(args_iter.next().unwrap_or_else(|| "mpatch".to_string()));

    // Iterate through the rest of the command-line arguments.
    for arg in args_iter {
        let arg_path = PathBuf::from(&arg);
        // Canonicalize paths to handle relative vs. absolute paths consistently.
        let canonical_arg = fs::canonicalize(&arg_path).unwrap_or(arg_path);
        let canonical_input =
            fs::canonicalize(&args.input_file).unwrap_or_else(|_| args.input_file.clone());
        let canonical_target =
            fs::canonicalize(&args.target_dir).unwrap_or_else(|_| args.target_dir.clone());

        // Check if the argument matches one of the sensitive paths.
        if canonical_arg == canonical_input {
            anonymized_args.push("<INPUT_FILE>".to_string());
        } else if canonical_arg == canonical_target {
            anonymized_args.push("<TARGET_DIR>".to_string());
        } else {
            // If it's not a sensitive path (e.g., a flag like `-v`), keep it as is.
            anonymized_args.push(arg);
        }
    }
    anonymized_args.join(" ")
}
