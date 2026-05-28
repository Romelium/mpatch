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
    // We re-bind `args` as mutable and allow `unused_mut`.
    // When the `clipboard` feature is disabled, `args` is never mutated,
    // which triggers an `unused_mut` warning. When enabled, it MUST be mutable.
    // This satisfies the compiler across all feature flag combinations.
    #[allow(unused_mut)]
    let mut args = args;

    // --- File Parsing & Clipboard ---
    #[cfg(feature = "clipboard")]
    let use_clipboard = args.clipboard;
    #[cfg(not(feature = "clipboard"))]
    let use_clipboard = false;

    let content = if use_clipboard {
        #[cfg(feature = "clipboard")]
        {
            let mut clipboard =
                arboard::Clipboard::new().context("Failed to initialize clipboard")?;
            let content = clipboard
                .get_text()
                .context("Failed to read text from clipboard")?;

            let target = match (&args.input_file, &args.target_dir) {
                (Some(dir), None) => dir.clone(),
                (None, None) => PathBuf::from("."),
                (Some(_), Some(dir)) => dir.clone(),
                _ => PathBuf::from("."),
            };
            args.target_dir = Some(target);
            args.input_file = None; // clear this so report generator marks it cleanly
            content
        }
        #[cfg(not(feature = "clipboard"))]
        {
            unreachable!()
        }
    } else {
        let input_file = args
            .input_file
            .as_ref()
            .expect("input_file is required unless --clipboard is used");
        let content = fs::read_to_string(input_file)
            .with_context(|| format!("Failed to read input file '{}'", input_file.display()))?;
        content
    };

    let actual_target_dir = args.target_dir.as_ref().unwrap().clone();

    // --- Argument Validation ---
    if !actual_target_dir.is_dir() {
        return Err(anyhow!(
            "Target directory '{}' not found or is not a directory.",
            actual_target_dir.display()
        ));
    }
    if !(0.0..=1.0).contains(&args.fuzz_factor) {
        return Err(anyhow!("Fuzz factor must be between 0.0 and 1.0."));
    }

    let mut all_patches = parse_auto(&content)?;

    if args.reverse {
        info!(
            "Reversing {} patch(es) before application...",
            all_patches.len()
        );
        all_patches = mpatch::invert_patches(&all_patches);
    }

    // --- Setup Logging and Reporting ---
    // This sets up the logger and, if needed, creates a report file.
    // The `_finalizer` is a "drop guard". When it goes out of scope at the end of
    // this function (no matter how it exits), its `drop` method is called,
    // which guarantees the report file is correctly finalized.
    let report_arc = setup_logging_and_reporting(&args, &content, &all_patches)?;
    let (report_file_arc, original_contents, anonymizer) =
        if let Some((arc, contents, anon)) = report_arc {
            (Some(arc), Some(contents), Some(anon))
        } else {
            (None, None, None)
        };

    // This closure will be called at the end of the function to finalize the report.
    // This is done manually instead of with a Drop guard to allow access to `batch_result`.
    let finalize_report = |batch_result: Option<&mpatch::BatchResult>| {
        if let (Some(arc), Some(contents), Some(anon)) =
            (&report_file_arc, &original_contents, &anonymizer)
        {
            write_report_footer(arc, &args, &all_patches, batch_result, contents, anon);
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
    let batch_result = apply_patches_to_dir(&all_patches, &actual_target_dir, options);
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
                finalize_report(Some(&batch_result));
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
    info!("Successful operations: {}", success_count);
    info!("Failed operations:     {}", fail_count);
    if args.dry_run {
        info!("DRY RUN completed. No files were modified.");
    }

    if fail_count > 0 {
        warn!("Review the log for errors. Some files may be in a partially patched state.");
        finalize_report(Some(&batch_result));

        // Return an error to set a non-zero exit code.
        return Err(anyhow!(
            "Completed with {} failed patch operations.",
            fail_count
        ));
    }

    finalize_report(Some(&batch_result));
    Ok(())
}

// --- Helper Structs and Functions ---

#[derive(Clone)]
struct Anonymizer {
    replacements: Vec<(String, String)>,
}

impl Anonymizer {
    fn new(args: &Args) -> Self {
        let mut replacements = Vec::new();

        if let Some(input) = &args.input_file {
            let canon = fs::canonicalize(input).unwrap_or_else(|_| input.clone());
            replacements.push((
                canon.to_string_lossy().into_owned(),
                "<INPUT_FILE>".to_string(),
            ));
            if let Some(parent) = canon.parent() {
                replacements.push((
                    parent.to_string_lossy().into_owned(),
                    "<INPUT_DIR>".to_string(),
                ));
            }
        }

        if let Some(target) = &args.target_dir {
            let canon = fs::canonicalize(target).unwrap_or_else(|_| target.clone());
            replacements.push((
                canon.to_string_lossy().into_owned(),
                "<TARGET_DIR>".to_string(),
            ));
        }

        if let Ok(cwd) = std::env::current_dir() {
            replacements.push((cwd.to_string_lossy().into_owned(), "<CWD>".to_string()));
        }

        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            replacements.push((home, "<HOME>".to_string()));
        }

        // Filter out very short strings (like "/", "\", "C:\") to prevent over-anonymization
        replacements.retain(|(s, _)| s.len() > 3);

        // Sort by length descending so we replace the longest, most specific paths first
        replacements.sort_by_key(|b| std::cmp::Reverse(b.0.len()));

        // Precompute and deduplicate slash formatting
        let mut processed = Vec::new();
        for (search, replace) in replacements {
            let search_forward = search.replace('\\', "/");
            if !processed.contains(&(search.clone(), replace.clone())) {
                processed.push((search.clone(), replace.clone()));
            }
            if search_forward != search
                && !processed.contains(&(search_forward.clone(), replace.clone()))
            {
                processed.push((search_forward, replace));
            }
        }

        processed.sort_by_key(|b| std::cmp::Reverse(b.0.len()));
        Self {
            replacements: processed,
        }
    }

    fn anonymize<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        if self.replacements.is_empty() {
            return std::borrow::Cow::Borrowed(text);
        }
        let mut result = std::borrow::Cow::Borrowed(text);
        for (search, replace) in &self.replacements {
            if result.contains(search) {
                result = std::borrow::Cow::Owned(result.replace(search, replace));
            }
        }
        result
    }
}

/// A tuple containing the shared handle to the report file, a map of original file contents, and the anonymizer.
type ReportData = (Arc<Mutex<File>>, HashMap<PathBuf, String>, Anonymizer);

/// Logs the reasons why hunks failed to apply.
fn log_failed_hunks(apply_result: &mpatch::ApplyResult, patch: &Patch) {
    if !log::log_enabled!(log::Level::Warn) {
        return;
    }
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
    long_about = "A high-resilience patching tool designed for LLM-generated code. It applies changes by searching for code context rather than relying on fragile line numbers. It automatically detects Unified Diffs and Markdown blocks. Note: Conflict Markers are supported but lack file path metadata."
)]
struct Args {
    /// Path to the input file containing the patch (Markdown, Unified Diff, or Conflict Markers).
    /// If --clipboard is used, the first positional argument becomes the target directory.
    #[cfg_attr(feature = "clipboard", arg(required_unless_present = "clipboard"))]
    #[cfg_attr(not(feature = "clipboard"), arg(required = true))]
    input_file: Option<PathBuf>,

    /// Path to the target directory to apply patches.
    #[cfg_attr(feature = "clipboard", arg(required_unless_present = "clipboard"))]
    #[cfg_attr(not(feature = "clipboard"), arg(required = true))]
    target_dir: Option<PathBuf>,

    /// Input from clipboard instead of a file.
    #[cfg(feature = "clipboard")]
    #[arg(short = 'c', long, help = "Input from clipboard instead of a file.")]
    clipboard: bool,
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
    /// Reverse the patch before applying (swaps additions and deletions).
    #[arg(short = 'R', long, help = "Reverse the patch before applying.")]
    reverse: bool,
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
    anonymizer: Anonymizer,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Write to standard error first.
        io::stderr().write_all(buf)?;
        // Then write to the locked file.
        if let Ok(s) = std::str::from_utf8(buf) {
            let anonymized = self.anonymizer.anonymize(s);
            self.file.lock().unwrap().write_all(anonymized.as_bytes())?;
        } else {
            self.file.lock().unwrap().write_all(buf)?;
        }
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
        let anonymizer = Anonymizer::new(args);
        // --- Create and Write Report Header ---
        let (file_arc, original_contents) =
            create_report_file(args, patch_content, patches, &anonymizer)?;
        // --- Configure Logger to Tee to the Report File ---
        builder
            .filter_level(LevelFilter::Trace) // Max verbosity for the report
            .target(env_logger::Target::Pipe(Box::new(TeeWriter {
                file: file_arc.clone(),
                anonymizer: anonymizer.clone(),
            })));
        Some((file_arc, original_contents, anonymizer))
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
#[allow(clippy::type_complexity)]
fn create_report_file(
    args: &Args,
    patch_content: &str,
    patches: &[Patch],
    anonymizer: &Anonymizer,
) -> Result<(Arc<Mutex<File>>, HashMap<PathBuf, String>)> {
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
    let cmd = anonymize_command_args(args);
    writeln!(file, "{}", anonymizer.anonymize(&cmd))?;
    writeln!(file, "```")?;

    // --- Write Input Patch File ---
    writeln!(file, "\n## Input Patch File\n")?;
    writeln!(file, "````markdown")?;
    writeln!(file, "{}", anonymizer.anonymize(patch_content))?;
    writeln!(file, "````")?;

    // --- Write Original Target Files ---
    writeln!(file, "\n## Original Target File(s)\n")?;
    let mut original_contents = HashMap::new();
    for patch in patches {
        let target_file_path = args.target_dir.as_ref().unwrap().join(&patch.file_path);
        writeln!(file, "### File: `{}`\n", patch.file_path.display())?;
        match fs::read_to_string(&target_file_path) {
            Ok(file_content) => {
                let lang = target_file_path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                writeln!(file, "````{}", lang)?;
                writeln!(file, "{}", anonymizer.anonymize(&file_content))?;
                writeln!(file, "````")?;
                original_contents.insert(patch.file_path.clone(), file_content);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                writeln!(file, "*File does not exist.*")?;
                original_contents.insert(patch.file_path.clone(), String::new());
            }
            Err(e) => {
                let msg = format!("*Error reading file: {}*", e);
                writeln!(file, "{}", anonymizer.anonymize(&msg))?;
            }
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
    anonymizer: &Anonymizer,
) {
    // Use a static bool to ensure this only runs once.
    use std::sync::atomic::{AtomicBool, Ordering};
    static IS_FINALIZED: AtomicBool = AtomicBool::new(false);
    if IS_FINALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    log::logger().flush();

    // Scope 1: Write initial headers and final file states
    {
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
                let target_file_path = args.target_dir.as_ref().unwrap().join(&patch.file_path);
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
                            let _ = writeln!(file, "{}", anonymizer.anonymize(&file_content));
                            let _ = writeln!(file, "````");
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        let _ = writeln!(file, "*File does not exist.*");
                    }
                    Err(e) => {
                        let msg = format!("*Error reading file: {}*", e);
                        let _ = writeln!(file, "{}", anonymizer.anonymize(&msg));
                    }
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
    } // Lock released here

    let Some(batch_result) = batch_result else {
        let mut file = file_arc.lock().unwrap();
        let _ = writeln!(
            file,
            "*Discrepancy check was skipped as patch application did not complete.*"
        );
        return;
    };

    for (original_patch, (path, result)) in all_patches.iter().zip(batch_result.results.iter()) {
        // Scope 2: Write file header
        {
            let mut file = file_arc.lock().unwrap();
            let _ = writeln!(file, "### File: `{}`", path.display());
        } // Lock released

        match result {
            Ok(_) => {
                let Some(old_content) = original_contents.get(path) else {
                    let mut file = file_arc.lock().unwrap();
                    let _ = writeln!(file, "\n- **Result:** <span style='color:orange;'>SKIPPED</span> (Could not read original file content for comparison).");
                    continue;
                };

                let new_content = match fs::read_to_string(
                    args.target_dir.as_ref().unwrap().join(path),
                ) {
                    Ok(content) => content,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
                    Err(_) => {
                        let mut file = file_arc.lock().unwrap();
                        let _ = writeln!(file, "\n- **Result:** <span style='color:orange;'>SKIPPED</span> (Could not read new file content after patching).");
                        continue;
                    }
                };

                // Re-create a patch from the before/after state.
                // NO LOCK HELD HERE - Prevents deadlock if from_texts logs anything
                let recreated_patch =
                    Patch::from_texts(path, old_content, &new_content, 3).unwrap();

                // Scope 3: Write result
                {
                    let mut file = file_arc.lock().unwrap();
                    if compare_patches(original_patch, &recreated_patch) {
                        let _ = writeln!(file, "\n- **Result:** <span style='color:green;'>SUCCESS</span>\n- **Details:** The regenerated patch is identical to the input patch (ignoring context lines).");
                    } else {
                        let original_norm = format_normalized_patch(original_patch);
                        let recreated_norm = format_normalized_patch(&recreated_patch);
                        let diff_text = similar::udiff::unified_diff(
                            similar::Algorithm::default(),
                            &original_norm,
                            &recreated_norm,
                            3,
                            Some(("Original Input Patch", "Regenerated Patch")),
                        );

                        let _ = writeln!(file, "\n- **Result:** <span style='color:red;'>FAILURE</span>\n- **Details:** The regenerated patch does not match the input patch. This may indicate an issue with how a fuzzy match was applied.");
                        let _ = writeln!(file, "\n**Diff (Original vs. Regenerated):**");
                        let _ = writeln!(
                            file,
                            "```diff\n{}```",
                            anonymizer.anonymize(&diff_text.to_string())
                        );
                        let _ = writeln!(file, "\n<details><summary>Click to see full original and regenerated patches</summary>\n");
                        let _ = writeln!(file, "**Original Input Patch:**");
                        let _ = writeln!(
                            file,
                            "```diff\n{}```",
                            anonymizer.anonymize(&original_patch.to_string())
                        );
                        let _ = writeln!(file, "\n**Regenerated Patch (from file changes):**");
                        let _ = writeln!(
                            file,
                            "```diff\n{}```",
                            anonymizer.anonymize(&recreated_patch.to_string())
                        );
                        let _ = writeln!(file, "</details>\n");
                    }
                }
            }
            Err(e) => {
                let mut file = file_arc.lock().unwrap();
                let msg = format!("\n- **Result:** <span style='color:orange;'>SKIPPED</span> (Patch application failed with a hard error: {}).", e);
                let _ = writeln!(file, "{}", anonymizer.anonymize(&msg));
            }
        }
    }
}

/// Formats a [`Patch`] struct into a normalized string for robust discrepancy checking.
/// It excludes context lines, ignores interleaving of +/- lines, removes self-replacements,
/// and sorts additions/deletions globally to ignore hunk ordering differences.
fn format_normalized_patch(patch: &Patch) -> String {
    let mut all_removed = Vec::new();
    let mut all_added = Vec::new();

    for hunk in &patch.hunks {
        all_removed.extend(hunk.removed_lines().into_iter().map(|s| s.to_string()));
        all_added.extend(hunk.added_lines().into_iter().map(|s| s.to_string()));
    }

    // Remove identical lines (self-replacements) using multiset subtraction
    let mut i = 0;
    while i < all_removed.len() {
        if let Some(j) = all_added.iter().position(|a| a == &all_removed[i]) {
            all_removed.remove(i);
            all_added.remove(j);
        } else {
            i += 1;
        }
    }

    // Sort to ignore ordering differences across hunks
    all_removed.sort();
    all_added.sort();

    let mut output = String::new();
    for line in all_removed {
        let _ = writeln!(output, "-{}", line);
    }
    for line in all_added {
        let _ = writeln!(output, "+{}", line);
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

    let canonical_input = args
        .input_file
        .as_ref()
        .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()));
    let canonical_target = args
        .target_dir
        .as_ref()
        .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()));

    // Iterate through the rest of the command-line arguments.
    for arg in args_iter {
        let arg_path = PathBuf::from(&arg);
        // Canonicalize paths to handle relative vs. absolute paths consistently.
        let canonical_arg = fs::canonicalize(&arg_path).unwrap_or(arg_path);

        // Check if the argument matches one of the sensitive paths.
        if canonical_input
            .as_ref()
            .is_some_and(|p| p == &canonical_arg)
        {
            anonymized_args.push("<INPUT_FILE>".to_string());
        } else if canonical_target
            .as_ref()
            .is_some_and(|p| p == &canonical_arg)
        {
            anonymized_args.push("<TARGET_DIR>".to_string());
        } else {
            // If it's not a sensitive path (e.g., a flag like `-v`), keep it as is.
            anonymized_args.push(arg);
        }
    }
    anonymized_args.join(" ")
}

/// Compares two patches for semantic equivalence, focusing on the actual line changes.
/// It ignores context lines, hunk headers, interleaving, self-replacements, and hunk order
/// by comparing their normalized string representations.
fn compare_patches(original: &Patch, recreated: &Patch) -> bool {
    format_normalized_patch(original) == format_normalized_patch(recreated)
}
