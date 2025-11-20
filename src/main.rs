use anyhow::{anyhow, Context, Result};
use clap::Parser;
use colored::Colorize;
use env_logger::Builder;
use log::{error, info, warn, Level, LevelFilter};
use mpatch::{apply_patches_to_dir, parse_auto, Patch};
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, time::SystemTime, time::UNIX_EPOCH};

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
    let all_patches = parse_auto(&content)?;

    // --- Setup Logging and Reporting ---
    // This sets up the logger and, if needed, creates a report file.
    // The `_finalizer` is a "drop guard". When it goes out of scope at the end of
    // this function (no matter how it exits), its `drop` method is called,
    // which guarantees the report file is correctly finalized.
    let report_arc = setup_logging_and_reporting(&args, &content, &all_patches)?;
    let (report_file_arc, original_contents) = if let Some((arc, contents)) = report_arc {
        (Some(arc), Some(contents))
    } else {
        (None, None)
    };

    // This closure will be called at the end of the function to finalize the report.
    // This is done manually instead of with a Drop guard to allow access to `batch_result`.
    let finalize_report = |batch_result: Option<&mpatch::BatchResult>| {
        if let (Some(arc), Some(contents)) = (&report_file_arc, &original_contents) {
            write_report_footer(arc, &args, &all_patches, batch_result, contents);
        }
    };
    // --- Core Patching Logic ---
    if all_patches.is_empty() {
        info!("No valid patches found or processed in the input file.");
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

    // Use the new high-level batch application function.
    let batch_result = apply_patches_to_dir(&all_patches, &args.target_dir, options);
    let num_ops = batch_result.results.len();

    // Iterate through the results to provide detailed CLI feedback.
    for (i, ((path, result), patch)) in batch_result.results.iter().zip(&all_patches).enumerate() {
        info!(""); // Vertical spacing
        info!(">>> Operation {}/{}", i + 1, num_ops);
        match result {
            Ok(patch_result) => {
                if let Some(diff) = &patch_result.diff {
                    println!("----- Proposed Changes for {} -----", path.display());
                    print!("{}", diff);
                    println!("------------------------------------");
                }
                if patch_result.report.all_applied_cleanly() {
                    success_count += 1;
                } else {
                    fail_count += 1;
                    error!("--- FAILED to apply patch for: {}", path.display());
                    log_failed_hunks(&patch_result.report, patch);
                }
            }
            Err(e) => {
                // A "hard" error occurred (e.g., I/O error, path traversal).
                // This is fatal, so we stop and return the error.
                // Since `e` is a reference from `.iter()`, we create a new error from its display representation.
                return Err(anyhow!("{}", e)).with_context(|| {
                    format!(
                        "A fatal error occurred while applying patch for: {}",
                        path.display()
                    )
                });
            }
        }
    }

    // --- Final Summary ---
    info!("\n--- Summary ---");
    finalize_report(Some(&batch_result));
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

    finalize_report(None); // Finalize report on success if not already done
    Ok(())
}

// --- Helper Structs and Functions ---

/// A tuple containing the shared handle to the report file and a map of original file contents.
type ReportData = (Arc<Mutex<File>>, HashMap<PathBuf, String>);

/// Logs the reasons why hunks failed to apply.
fn log_failed_hunks(apply_result: &mpatch::ApplyResult, patch: &Patch) {
    for failure in apply_result.failures() {
        warn!("  - Hunk {} failed: {}", failure.hunk_index, failure.reason);
        // hunk_index is 1-based, so we need to subtract 1 for indexing.
        if let Some(hunk) = patch.hunks.get(failure.hunk_index - 1) {
            warn!("    Failed Hunk Content:");
            for line in &hunk.lines {
                warn!("      {}", line);
            }
        }
    }
}

/// Defines the command-line arguments for the application.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Apply diff hunks from a file to a target directory based on context, ignoring line numbers.",
    long_about = "Uses fuzzy matching if exact context fails. Automatically detects and parses Unified Diffs, Markdown code blocks, and Conflict Markers."
)]
struct Args {
    /// Path to the input file containing the patch (Markdown, Unified Diff, or Conflict Markers).
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

/// Sets up the global logger, creating a report file if verbosity is >= 4.
fn setup_logging_and_reporting(
    args: &Args,
    patch_content: &str,
    patches: &[Patch],
) -> Result<Option<ReportData>> {
    let mut builder = Builder::new();
    let report_data = if args.verbose >= 4 {
        // --- Create and Write Report Header ---
        let (file_arc, original_contents) = create_report_file(args, patch_content, patches)?;
        // --- Configure Logger to Tee to the Report File ---
        builder
            .filter_level(LevelFilter::Trace) // Max verbosity for the report
            .target(env_logger::Target::Pipe(Box::new(TeeWriter {
                file: file_arc.clone(),
            })));
        Some((file_arc, original_contents))
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

    Ok(report_data)
}

/// Creates the report file, writes the header, and returns a shared pointer to it.
fn create_report_file(args: &Args, patch_content: &str, patches: &[Patch]) -> Result<ReportData> {
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
    let mut original_contents = HashMap::new();
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
                original_contents.insert(patch.file_path.clone(), file_content);
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
    Ok((Arc::new(Mutex::new(file)), original_contents))
}

/// Writes the final sections of the debug report, including the discrepancy check.
fn write_report_footer(
    file_arc: &Arc<Mutex<File>>,
    args: &Args,
    all_patches: &[Patch],
    batch_result: Option<&mpatch::BatchResult>,
    original_contents: &HashMap<PathBuf, String>,
) {
    // Use a static bool to ensure this only runs once.
    use std::sync::atomic::{AtomicBool, Ordering};
    static IS_FINALIZED: AtomicBool = AtomicBool::new(false);
    if IS_FINALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    log::logger().flush();
    let mut file = file_arc.lock().unwrap();

    // Use `let _ = ...` to ignore potential write errors during cleanup.
    let _ = writeln!(file, "````"); // Close the log block

    // --- Final Target Files Section ---
    let _ = writeln!(file, "\n## Final Target File(s)\n");
    let _ = writeln!(file, "> This section shows the state of the target files *after* the patch operation was attempted.\n");

    if args.dry_run {
        let _ = writeln!(
            file,
            "*Final file state is the same as the original state because `--dry-run` was active.*"
        );
    } else {
        for patch in all_patches {
            let target_file_path = args.target_dir.join(&patch.file_path);
            let _ = writeln!(file, "### File: `{}`\n", patch.file_path.display());
            match fs::read_to_string(&target_file_path) {
                Ok(file_content) => {
                    if file_content.is_empty() {
                        let _ = writeln!(file, "*File is empty.*");
                    } else {
                        let lang = target_file_path
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("");
                        let _ = writeln!(file, "````{}", lang);
                        let _ = writeln!(file, "{}", file_content);
                        let _ = writeln!(file, "````");
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    let _ = writeln!(file, "*File does not exist.*");
                }
                Err(e) => _ = writeln!(file, "*Error reading file: {}*", e),
            }
        }
    }

    // --- Discrepancy Check Section ---
    let _ = writeln!(file, "\n## Discrepancy Check\n");
    let _ = writeln!(file, "> This section verifies that applying the patch and then creating a new diff from the result reproduces the original input patch. This is a key integrity check.\n");

    if args.dry_run {
        let _ = writeln!(
            file,
            "*Discrepancy check was skipped because `--dry-run` was active.*"
        );
        return;
    }

    let Some(batch_result) = batch_result else {
        let _ = writeln!(
            file,
            "*Discrepancy check was skipped as patch application did not complete.*"
        );
        return;
    };

    for (original_patch, (path, result)) in all_patches.iter().zip(batch_result.results.iter()) {
        let _ = writeln!(file, "### File: `{}`", path.display());
        match result {
            Ok(_) => {
                let Some(old_content) = original_contents.get(path) else {
                    let _ = writeln!(file, "\n- **Result:** <span style='color:orange;'>SKIPPED</span> (Could not read original file content for comparison).");
                    continue;
                };

                let new_content_res = fs::read_to_string(args.target_dir.join(path));
                let Ok(new_content) = new_content_res else {
                    let _ = writeln!(file, "\n- **Result:** <span style='color:orange;'>SKIPPED</span> (Could not read new file content after patching).");
                    continue;
                };

                // Re-create a patch from the before/after state.
                let recreated_patch =
                    Patch::from_texts(path, old_content, &new_content, 3).unwrap();

                if compare_patches(original_patch, &recreated_patch) {
                    let _ = writeln!(file, "\n- **Result:** <span style='color:green;'>SUCCESS</span>\n- **Details:** The regenerated patch is identical to the input patch.");
                } else {
                    let _ = writeln!(file, "\n- **Result:** <span style='color:red;'>FAILURE</span>\n- **Details:** The regenerated patch does not match the input patch. This may indicate an issue with how a fuzzy match was applied.");
                    let _ = writeln!(file, "\nClick to see original vs. regenerated patch\n");
                    let _ = writeln!(file, "**Original Input Patch:**");
                    let _ = writeln!(
                        file,
                        "```diff\n{}```",
                        format_patch_for_report(original_patch)
                    );
                    let _ = writeln!(file, "\n**Regenerated Patch (from file changes):**");
                    let _ = writeln!(
                        file,
                        "```diff\n{}```",
                        format_patch_for_report(&recreated_patch)
                    );
                }
            }
            Err(e) => {
                let _ = writeln!(file, "\n- **Result:** <span style='color:orange;'>SKIPPED</span> (Patch application failed with a hard error: {}).", e);
            }
        }
    }
}

/// Formats a [`Patch`] struct back into a human-readable diff string for reporting.
fn format_patch_for_report(patch: &Patch) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "--- a/{}", patch.file_path.display());
    let _ = writeln!(output, "+++ b/{}", patch.file_path.display());
    for hunk in &patch.hunks {
        let old_len = hunk.lines.iter().filter(|l| !l.starts_with('+')).count();
        let new_len = hunk.lines.iter().filter(|l| !l.starts_with('-')).count();
        let old_start = hunk.old_start_line.unwrap_or(1);
        let new_start = hunk.new_start_line.unwrap_or(1);
        let _ = writeln!(
            output,
            "@@ -{},{} +{},{} @@",
            old_start, old_len, new_start, new_len
        );
        for line in &hunk.lines {
            let _ = writeln!(output, "{}", line);
        }
    }
    if !patch.ends_with_newline {
        let _ = write!(output, "\\ No newline at end of file");
    }
    output
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

/// Compares two patches for semantic equivalence, focusing on the actual line changes.
/// It ignores `old_start_line` and `new_start_line` because these can differ legitimately
/// after a fuzzy match finds a new location.
fn compare_patches(original: &Patch, recreated: &Patch) -> bool {
    if original.hunks.len() != recreated.hunks.len() {
        return false;
    }
    for (h1, h2) in original.hunks.iter().zip(recreated.hunks.iter()) {
        // Compare the actual changes, ignoring context lines.
        if h1.added_lines() != h2.added_lines() {
            return false;
        }
        if h1.removed_lines() != h2.removed_lines() {
            return false;
        }
    }
    // Also check newline status, which is part of the patch's semantics.
    original.ends_with_newline == recreated.ends_with_newline
}
