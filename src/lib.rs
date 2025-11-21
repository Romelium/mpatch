//! A smart, context-aware patch tool that applies diffs using fuzzy matching.
//!
//! `mpatch` is designed to apply unified diffs to a codebase, but with a key
//! difference from the standard `patch` command: it doesn't rely on strict line
//! numbers. Instead, it finds the correct location to apply changes by searching
//! for the surrounding context lines.
//!
//! This makes it highly resilient to patches that are "out of date" because of
//! preceding changes, which is a common scenario when working with AI-generated
//! diffs, code from pull requests, or snippets from documentation.
//!
//! ## Getting Started
//!
//! The simplest way to use `mpatch` is the one-shot [`patch_content_str()`] function.
//! It's perfect for the common workflow of taking a diff string (e.g., from an
//! LLM in a markdown file) and applying it to some existing content in memory.
//!
//! ````rust
//! use mpatch::{patch_content_str, ApplyOptions};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Define the original content and the diff.
//! let original_content = "fn main() {\n    println!(\"Hello, world!\");\n}\n";
//! let diff_content = r#"
//! A markdown file with a diff block.
//! ```diff
//! --- a/src/main.rs
//! +++ b/src/main.rs
//! @@ -1,3 +1,3 @@
//!  fn main() {
//! -    println!("Hello, world!");
//! +    println!("Hello, mpatch!");
//!  }
//! ```
//! "#;
//!
//! // 2. Call the one-shot function to parse and apply the patch.
//! let options = ApplyOptions::new();
//! let new_content = patch_content_str(diff_content, Some(original_content), &options)?;
//!
//! // 3. Verify the new content.
//! let expected_content = "fn main() {\n    println!(\"Hello, mpatch!\");\n}\n";
//! assert_eq!(new_content, expected_content);
//!
//! # Ok(())
//! # }
//! ````
//!
//! ## Applying Patches to Files
//!
//! For CLI tools or scripts that need to modify files on disk, the workflow involves
//! parsing and then using [`apply_patches_to_dir()`]. This example shows the end-to-end
//! process in a temporary directory.
//!
//! ````rust
//! use mpatch::{parse_auto, apply_patches_to_dir, ApplyOptions};
//! use std::fs;
//! use tempfile::tempdir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Set up a temporary directory and a file to be patched.
//! let dir = tempdir()?;
//! let file_path = dir.path().join("src/main.rs");
//! fs::create_dir_all(file_path.parent().unwrap())?;
//! fs::write(&file_path, "fn main() {\n    println!(\"Hello, world!\");\n}\n")?;
//!
//! // 2. Define the diff content, as if it came from a markdown file.
//! let diff_content = r#"
//! Some introductory text.
//!
//! ```diff
//! --- a/src/main.rs
//! +++ b/src/main.rs
//! @@ -1,3 +1,3 @@
//!  fn main() {
//! -    println!("Hello, world!");
//! +    println!("Hello, mpatch!");
//!  }
//! ```
//!
//! Some concluding text.
//! "#;
//!
//! // 3. Parse the diff content to get patches.
//! let patches = parse_auto(diff_content)?;
//!
//! // 4. Apply the patches to the directory.
//! let options = ApplyOptions::new();
//! let result = apply_patches_to_dir(&patches, dir.path(), options);
//!
//! // The batch operation should succeed.
//! assert!(result.all_succeeded());
//!
//! // 5. Verify the file was changed correctly.
//! let new_content = fs::read_to_string(&file_path)?;
//! let expected_content = "fn main() {\n    println!(\"Hello, mpatch!\");\n}\n";
//! assert_eq!(new_content, expected_content);
//! # Ok(())
//! # }
//! ````
//!
//! ## Key Concepts
//!
//! ### The Patching Workflow
//!
//! Using the `mpatch` library typically involves a two-step process: parsing and applying.
//!
//! #### 1. Parsing
//!
//! First, you convert diff text into a structured `Vec<Patch>`. `mpatch` provides
//! several functions for this, depending on your input format:
//!
//! - [`parse_auto()`]: The recommended entry point. It automatically detects the format
//!   (Markdown, Unified Diff, or Conflict Markers) and parses the content accordingly.
//! - [`parse_single_patch()`]: A convenient wrapper around `parse_auto()` that ensures
//!   the input contains exactly one patch, returning a `Result<Patch, _>`.
//! - [`parse_diffs()`]: Scans a string for markdown code blocks containing diffs.
//! - [`parse_patches()`]: A lower-level parser that processes a raw unified diff string
//!   directly, without needing markdown fences.
//! - [`parse_conflict_markers()`]: Parses a string containing conflict markers
//!   (`<<<<`, `====`, `>>>>`) into patches.
//! - [`parse_patches_from_lines()`]: The lowest-level parser. It operates on an iterator
//!   of lines, which is useful for streaming or avoiding large string allocations.
//!
//! You can also use [`detect_patch()`] to identify the format (Markdown, Unified, or Conflict)
//! without parsing the full content.
//!
//! #### 2. Applying
//!
//! Once you have a `Patch`, you can apply it using one of the `apply` functions:
//!
//! - [`apply_patches_to_dir()`]: Applies a list of patches to a directory. This is
//!   ideal for processing multi-file diffs.
//! - [`apply_patch_to_file()`]: The most convenient function for applying a single
//!   patch to a file. It handles reading the original file and writing the new content
//!   back to disk.
//! - [`apply_patch_to_content()`]: A pure function for in-memory operations. It takes
//!   the original content as a string and returns the new content.
//!
//! Each of these also has a "strict" `try_` variant (e.g., [`try_apply_patch_to_file()`])
//! that treats partial applications as an error, simplifying the common apply-or-fail
//! workflow.
//!
//! ### Core Data Structures
//!
//! - [`Patch`]: Represents all the changes for a single file. It contains the
//!   target file path and a list of hunks.
//! - [`Hunk`]: Represents a single block of changes within a patch, corresponding
//!   to a block of changes (like a `@@ ... @@` section in a unified diff).
//!   For **Conflict Markers**, the "before" block is treated as deletions and the
//!   "after" block as additions.
//!
//! ### Context-Driven Matching
//!
//! The core philosophy of `mpatch` is to ignore strict line numbers. Instead, it
//! searches for the *context* of a hunk—the lines that are unchanged or being
//! deleted.
//!
//! - **Primary Search:** It first looks for an exact, character-for-character match
//!   of the hunk's context.
//! - **Ambiguity Resolution:** If the same context appears in multiple places,
//!   `mpatch` uses the line numbers (e.g., from the `@@ ... @@` header) as a *hint* to
//!   find the most likely location.
//!   Note that patches derived from **Conflict Markers** typically lack line numbers,
//!   so ambiguity cannot be resolved this way.
//! - **Fuzzy Matching:** If no exact match is found, it uses a similarity algorithm
//!   to find the *best* fuzzy match, making it resilient to minor changes in the
//!   surrounding code.
//!
//! ## Advanced Usage
//!
//! ### Configuring `ApplyOptions`
//!
//! The behavior of the `apply` functions is controlled by the [`ApplyOptions`] struct.
//! `mpatch` provides several convenient ways to construct it:
//!
//! ````rust
//! use mpatch::ApplyOptions;
//!
//! // For default behavior (fuzzy matching enabled, not a dry run)
//! let default_options = ApplyOptions::new();
//!
//! // For common presets
//! let dry_run_options = ApplyOptions::dry_run();
//! let exact_options = ApplyOptions::exact();
//!
//! // For custom configurations using the new fluent methods
//! let custom_fluent = ApplyOptions::new()
//!     .with_dry_run(true)
//!     .with_fuzz_factor(0.9);
//!
//! // For complex configurations using the builder pattern
//! let custom_builder = ApplyOptions::builder()
//!     .dry_run(true)
//!     .fuzz_factor(0.9)
//!     .build();
//!
//! assert_eq!(custom_fluent.dry_run, custom_builder.dry_run);
//! assert_eq!(custom_fluent.fuzz_factor, custom_builder.fuzz_factor);
//! ````
//!
//! ### In-Memory Operations and Error Handling
//!
//! This example demonstrates how to use [`apply_patch_to_content()`] for in-memory
//! operations and how to programmatically handle cases where a patch only
//! partially applies.
//!
//! ````rust
//! use mpatch::{parse_single_patch, apply_patch_to_content, HunkApplyError};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Define original content and a patch where the second hunk will fail.
//! let original_content = "line 1\nline 2\nline 3\n\nline 5\nline 6\nline 7\n";
//! let diff_content = r#"
//! ```diff
//! --- a/partial.txt
//! +++ b/partial.txt
//! @@ -1,3 +1,3 @@
//!  line 1
//! -line 2
//! +line two
//!  line 3
//! @@ -5,3 +5,3 @@
//!  line 5
//! -line WRONG CONTEXT
//! +line six
//!  line 7
//! ```
//! "#;
//!
//! // 2. Parse the diff.
//! let patch = parse_single_patch(diff_content)?;
//!
//! // 3. Apply the patch to the content in memory.
//! let options = mpatch::ApplyOptions::exact();
//! let result = apply_patch_to_content(&patch, Some(original_content), &options);
//!
//! // 4. Verify that the patch did not apply cleanly.
//! assert!(!result.report.all_applied_cleanly());
//!
//! // 5. Inspect the specific failures.
//! let failures = result.report.failures();
//! assert_eq!(failures.len(), 1);
//! assert_eq!(failures[0].hunk_index, 2); // Hunk indices are 1-based.
//! assert!(matches!(failures[0].reason, HunkApplyError::ContextNotFound));
//!
//! // 6. Verify that the content was still partially modified by the successful first hunk.
//! let expected_content = "line 1\nline two\nline 3\n\nline 5\nline 6\nline 7\n";
//! assert_eq!(result.new_content, expected_content);
//! # Ok(())
//! # }
//! ````
//!
//! ### Strict Apply-or-Fail Workflow with `try_` functions
//!
//! The previous example showed how to manually check `result.report.all_applied_cleanly()`
//! to detect partial failures. For workflows where any failed hunk should be treated as a
//! hard error, `mpatch` provides "strict" variants of the apply functions.
//!
//! - [`try_apply_patch_to_file()`]
//! - [`try_apply_patch_to_content()`]
//! - [`try_apply_patch_to_lines()`]
//!
//! These functions return a `Result` where a partial application is mapped to a
//! `Err(StrictApplyError::PartialApply { .. })`. This simplifies the common
//! apply-or-fail pattern.
//!
//! ````rust
//! use mpatch::{parse_single_patch, try_apply_patch_to_content, ApplyOptions, StrictApplyError};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let original_content = "line 1\nline 2\n";
//! let failing_diff = r#"
//! ```diff
//! --- a/file.txt
//! +++ b/file.txt
//! @@ -1,2 +1,2 @@
//!  line 1
//! -WRONG CONTEXT
//! +line two
//! ```
//! "#;
//! let patch = parse_single_patch(failing_diff)?;
//! let options = ApplyOptions::exact();
//!
//! // Using the try_ variant simplifies error handling.
//! let result = try_apply_patch_to_content(&patch, Some(original_content), &options);
//!
//! assert!(matches!(result, Err(StrictApplyError::PartialApply { .. })));
//! # Ok(())
//! # }
//! ````
//!
//! ### Step-by-Step Application with `HunkApplier`
//!
//! For maximum control, you can use the [`HunkApplier`] iterator to apply hunks
//! one at a time and inspect the state between each step.
//!
//! ````rust
//! use mpatch::{parse_single_patch, HunkApplier, HunkApplyStatus, ApplyOptions};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Define original content and a patch.
//! let original_lines = vec!["line 1", "line 2", "line 3"];
//! let diff_content = r#"
//! ```diff
//! --- a/file.txt
//! +++ b/file.txt
//! @@ -2,1 +2,1 @@
//! -line 2
//! +line two
//! ```
//! "#;
//! let patch = parse_single_patch(diff_content)?;
//! let options = ApplyOptions::new();
//!
//! // 2. Create the applier.
//! let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
//!
//! // 3. Apply the first (and only) hunk.
//! let status = applier.next().unwrap();
//! assert!(matches!(status, HunkApplyStatus::Applied { .. }));
//!
//! // 4. Check that there are no more hunks.
//! assert!(applier.next().is_none());
//!
//! // 5. Finalize the content.
//! let new_content = applier.into_content();
//! assert_eq!(new_content, "line 1\nline two\nline 3\n");
//! # Ok(())
//! # }
//! ````
//!
//! ## Feature Flags
//!
//! `mpatch` includes the following optional features:
//!
//! ### `parallel`
//!
//! - **Enabled by default.**
//! - This feature enables parallel processing for the fuzzy matching algorithm using the
//!   [`rayon`](https://crates.io/crates/rayon) crate. When an exact match for a hunk
//!   is not found, `mpatch` performs a computationally intensive search for the best
//!   fuzzy match. The `parallel` feature significantly speeds up this process on
//!   multi-core systems by distributing the search across multiple threads.
//!
//! - **To disable this feature**, specify `default-features = false` in your `Cargo.toml`:
//!   ```toml
//!   [dependencies]
//!   mpatch = { version = "1.3.1", default-features = false }
//!   ```
//!   You might want to disable this feature if you are compiling for a target that
//!   does not support threading (like `wasm32-unknown-unknown`) or if you want to
//!   minimize dependencies and binary size.
use log::{debug, info, trace, warn};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use similar::udiff::unified_diff;
use similar::TextDiff;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

// --- Error Types ---

/// Represents errors that can occur during the parsing of a diff file.
///
/// This error is returned by parsing functions like [`parse_patches()`] and
/// [`parse_auto()`] when the input content is syntactically invalid.
///
/// Note that [`parse_diffs()`] is lenient and will typically skip blocks that do
/// not look like valid patches rather than returning this error.
///
/// # Example
///
/// ````rust
/// use mpatch::{parse_patches, ParseError};
///
/// // This raw diff is missing the required `--- a/path` header.
/// let malformed_diff = r#"
/// @@ -1,2 +1,2 @@
/// -foo
/// +bar
/// "#;
///
/// let result = parse_patches(malformed_diff);
///
/// assert!(matches!(result, Err(ParseError::MissingFileHeader { .. })));
/// ````
#[derive(Error, Debug, PartialEq)]
pub enum ParseError {
    /// A diff block or raw patch was found, but it was missing the `--- a/path/to/file`
    /// header required to identify the target file.
    #[error("Diff block starting on line {line} was found without a file path header (e.g., '--- a/path/to/file')")]
    MissingFileHeader {
        /// The line number where the diff block started.
        line: usize,
    },
}

/// Represents errors that can occur when parsing a diff expected to contain exactly one patch.
///
/// This enum is returned by [`parse_single_patch()`] when the input content does not
/// result in exactly one `Patch` object. It handles errors from format detection,
/// parsing, and patch count validation.
///
/// # Example
///
/// ````rust
/// use mpatch::{parse_single_patch, SingleParseError};
///
/// // This diff content contains two patches, which is not allowed.
/// let multi_patch_diff = r#"
/// ```diff
/// --- a/file1.txt
/// +++ b/file1.txt
/// @@ -1 +1 @@
/// -a
/// +b
/// --- a/file2.txt
/// +++ b/file2.txt
/// @@ -1 +1 @@
/// -c
/// +d
/// ```
/// "#;
///
/// let result = parse_single_patch(multi_patch_diff);
/// assert!(matches!(result, Err(SingleParseError::MultiplePatchesFound(2))));
/// ````
#[derive(Error, Debug, PartialEq)]
#[non_exhaustive]
pub enum SingleParseError {
    /// An error occurred during the underlying diff parsing.
    #[error("Failed to parse diff content")]
    Parse(#[from] ParseError),

    /// The provided diff content did not contain any valid patches (Markdown blocks,
    /// Unified Diffs, or Conflict Markers).
    #[error("No patches were found in the provided diff content")]
    NoPatchesFound,

    /// The provided diff content contained patches for more than one file, which is not
    /// supported by this function. Use [`parse_diffs()`] for multi-file operations.
    #[error(
        "Found patches for multiple files ({0} patches), but this function only supports single-file diffs"
    )]
    MultiplePatchesFound(usize),
}

/// Represents "hard" errors that can occur during patch operations.
///
/// This error type is returned by functions like [`apply_patch_to_file()`] for
/// unrecoverable issues such as I/O errors, permission problems, or security
/// violations like path traversal. It is distinct from a partial apply, which
/// is handled by the result structs.
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_single_patch, apply_patch_to_file, ApplyOptions, PatchError};
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// // Note: "missing.txt" does not exist in the directory.
///
/// let diff = r#"
/// ```diff
/// --- a/missing.txt
/// +++ b/missing.txt
/// @@ -1 +1 @@
/// -foo
/// +bar
/// ```
/// "#;
/// let patch = parse_single_patch(diff)?;
/// let options = ApplyOptions::new();
///
/// // This will fail because the target file doesn't exist and it's not a creation patch.
/// let result = apply_patch_to_file(&patch, dir.path(), options);
///
/// assert!(matches!(result, Err(PatchError::TargetNotFound(_))));
/// # Ok(())
/// # }
/// ````
#[derive(Error, Debug)]
pub enum PatchError {
    /// The patch attempted to access a path outside the target directory.
    /// This is a security measure to prevent malicious patches from modifying
    /// unintended files (e.g., `--- a/../../etc/passwd`).
    #[error("Path '{0}' resolves outside the target directory. Aborting for security.")]
    PathTraversal(PathBuf),
    /// The target file for a patch could not be found, and the patch did not
    /// appear to be for file creation (i.e., its first hunk was not an addition-only hunk).
    #[error("Target file not found for patching: {0}")]
    TargetNotFound(PathBuf),
    /// The user does not have permission to read or write to the specified path.
    #[error("Permission denied for path: {path:?}")]
    PermissionDenied { path: PathBuf },
    /// The target path for a patch exists but is a directory, not a file.
    #[error("Target path is a directory, not a file: {path:?}")]
    TargetIsDirectory { path: PathBuf },
    /// An I/O error occurred while reading or writing a file.
    /// This is a "hard" error that stops the entire process.
    #[error("I/O error while processing {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Represents errors that can occur during "strict" apply operations.
///
/// This enum is returned by functions like [`try_apply_patch_to_file()`] and
/// [`try_apply_patch_to_content()`], which treat partial applications as an error.
/// It consolidates hard failures ([`PatchError`]) and soft failures ([`StrictApplyError::PartialApply`])
/// into a single error type for easier handling in apply-or-fail workflows.
///
/// # Example
///
/// ````rust
/// use mpatch::{parse_single_patch, try_apply_patch_to_content, ApplyOptions, StrictApplyError};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_content = "line 1\nline 2\n";
/// // This patch will fail because the context "WRONG CONTEXT" is not in the original content.
/// let failing_diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -WRONG CONTEXT
/// +line two
/// ```
/// "#;
/// let patch = parse_single_patch(failing_diff)?;
/// let options = ApplyOptions::exact();
///
/// // Using the try_ variant simplifies error handling for partial applications.
/// let result = try_apply_patch_to_content(&patch, Some(original_content), &options);
///
/// assert!(matches!(result, Err(StrictApplyError::PartialApply { .. })));
/// if let Err(StrictApplyError::PartialApply { report }) = result {
///     assert!(!report.all_applied_cleanly());
/// }
/// # Ok(())
/// # }
/// ````
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum StrictApplyError {
    /// A hard error occurred during the patch operation (e.g., I/O error,
    /// file not found).
    #[error(transparent)]
    Patch(#[from] PatchError),

    /// The patch was only partially applied, with some hunks failing.
    #[error("Patch applied partially. See report for details.")]
    PartialApply {
        /// The detailed report of the operation, including which hunks succeeded/failed.
        report: ApplyResult,
    },
}

/// Represents errors that can occur during the high-level [`patch_content_str()`] operation.
///
/// This enum consolidates all possible failures from the one-shot workflow,
/// including parsing errors, finding the wrong number of patches, or failures
/// during the strict application process.
///
/// # Example
///
/// ````rust
/// use mpatch::{patch_content_str, ApplyOptions, OneShotError};
///
/// // This diff content contains two patches, which is not allowed by `patch_content_str`.
/// let multi_patch_diff = r#"
/// ```diff
/// --- a/file1.txt
/// +++ b/file1.txt
/// @@ -1 +1 @@
/// -a
/// +b
/// --- a/file2.txt
/// +++ b/file2.txt
/// @@ -1 +1 @@
/// -c
/// +d
/// ```
/// "#;
///
/// let options = ApplyOptions::new();
/// let result = patch_content_str(multi_patch_diff, Some("a\n"), &options);
///
/// assert!(matches!(result, Err(OneShotError::MultiplePatchesFound(2))));
/// ````
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum OneShotError {
    /// An error occurred while parsing the diff content.
    #[error("Failed to parse diff content")]
    Parse(#[from] ParseError),

    /// An error occurred while applying the patch. This includes partial applications.
    #[error("Failed to apply patch")]
    Apply(#[from] StrictApplyError),

    /// The provided diff content did not contain any valid patches (Markdown blocks,
    /// Unified Diffs, or Conflict Markers).
    #[error("No patches were found in the provided diff content")]
    NoPatchesFound,

    /// The provided diff content contained patches for more than one file, which is not
    /// supported by this simplified function. Use [`parse_diffs()`] and
    /// [`apply_patches_to_dir()`] for multi-file operations.
    #[error(
        "Found patches for multiple files ({0} files), but this function only supports single-file diffs"
    )]
    MultiplePatchesFound(usize),
}

/// The reason a hunk failed to apply.
///
/// This enum provides specific details about why a hunk could not be applied to the
/// target content. It is found within the [`HunkApplyStatus::Failed`] variant.
///
/// # Example
///
/// ````rust
/// use mpatch::{apply_patch_to_content, parse_single_patch, ApplyOptions, HunkApplyStatus, HunkApplyError};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_content = "line 1\nline 2\n";
/// // This patch will fail because the context is wrong.
/// let diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -WRONG CONTEXT
/// +line two
/// ```
/// "#;
/// let patch = parse_single_patch(diff)?;
/// let options = ApplyOptions::exact();
///
/// let result = apply_patch_to_content(&patch, Some(original_content), &options);
///
/// // We can inspect the status of the first hunk.
/// let hunk_status = &result.report.hunk_results[0];
///
/// assert!(matches!(hunk_status, HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)));
/// # Ok(())
/// # }
/// ````
#[derive(Error, Debug, Clone, PartialEq)]
pub enum HunkApplyError {
    /// The context lines for the hunk could not be found in the target file.
    #[error("Context not found")]
    ContextNotFound,
    /// An exact match for the hunk's context was found in multiple locations,
    /// and the ambiguity could not be resolved by the line number hint.
    #[error("Ambiguous exact match found at lines: {0:?}")]
    AmbiguousExactMatch(Vec<usize>),
    /// A fuzzy match for the hunk's context was found in multiple locations with
    /// the same top score, and the ambiguity could not be resolved.
    #[error("Ambiguous fuzzy match found at locations: {0:?}")]
    AmbiguousFuzzyMatch(Vec<(usize, usize)>),
    /// The best fuzzy match found was below the required similarity threshold.
    #[error("Best fuzzy match at {location} (score: {best_score:.3}) was below threshold ({threshold:.3})")]
    FuzzyMatchBelowThreshold {
        best_score: f64,
        threshold: f32,
        /// The location of the best-scoring (but rejected) fuzzy match.
        location: HunkLocation,
    },
}

/// Describes the method used to successfully locate and apply a hunk.
///
/// This enum is included in the [`HunkApplyStatus::Applied`] variant and provides
/// insight into how `mpatch` found the location for a hunk, which is useful for
/// logging and diagnostics.
///
/// # Example
///
/// ````rust
/// use mpatch::{apply_patch_to_content, parse_single_patch, ApplyOptions, HunkApplyStatus, MatchType};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // The file content has an extra space, which will prevent an `Exact` match.
/// let original_content = "line 1  \nline 2\n";
/// let diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -line 2
/// +line two
/// ```
/// "#;
/// let patch = parse_single_patch(diff)?;
/// let options = ApplyOptions::new();
///
/// let result = apply_patch_to_content(&patch, Some(original_content), &options);
/// let hunk_status = &result.report.hunk_results[0];
///
/// assert!(matches!(hunk_status, HunkApplyStatus::Applied { match_type: MatchType::ExactIgnoringWhitespace, .. }));
/// # Ok(())
/// # }
/// ````
#[derive(Debug, Clone, PartialEq)]
pub enum MatchType {
    /// An exact, character-for-character match of the context/deletion lines.
    Exact,
    /// An exact match after ignoring trailing whitespace on each line.
    ExactIgnoringWhitespace,
    /// A fuzzy match found using a similarity algorithm.
    Fuzzy {
        /// The similarity score of the match (0.0 to 1.0).
        score: f64,
    },
}

/// The result of applying a single hunk.
///
/// This enum is returned by [`apply_hunk_to_lines()`] and is the item type for the
/// [`HunkApplier`] iterator. It provides a detailed outcome for each individual
/// hunk within a patch.
///
/// # Example
///
/// ````rust
/// use mpatch::{apply_hunk_to_lines, parse_single_patch, ApplyOptions, HunkApplyStatus, HunkApplyError};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -line 2
/// +line two
/// ```
/// "#;
/// let hunk = parse_single_patch(diff)?.hunks.remove(0);
/// let options = ApplyOptions::new();
///
/// // --- Success Case ---
/// let mut lines_success = vec!["line 1".to_string(), "line 2".to_string()];
/// let status_success = apply_hunk_to_lines(&hunk, &mut lines_success, &options);
/// assert!(matches!(status_success, HunkApplyStatus::Applied { .. }));
/// assert_eq!(lines_success, vec!["line 1", "line two"]);
///
/// // --- Failure Case ---
/// let mut lines_fail = vec!["wrong".to_string(), "content".to_string()];
/// let fail_status = apply_hunk_to_lines(&hunk, &mut lines_fail, &options);
/// assert!(matches!(fail_status, HunkApplyStatus::Failed(HunkApplyError::FuzzyMatchBelowThreshold { .. })));
/// assert_eq!(lines_fail, vec!["wrong", "content"]); // Content is unchanged
/// # Ok(())
/// # }
/// ````
#[derive(Debug, Clone, PartialEq)]
pub enum HunkApplyStatus {
    /// The hunk was applied successfully.
    Applied {
        /// The location where the hunk was applied.
        location: HunkLocation,
        /// The type of match that was used to find the location.
        match_type: MatchType,
        /// The original lines that were replaced by the hunk.
        replaced_lines: Vec<String>,
    },
    /// The hunk was skipped because it contained no effective changes.
    SkippedNoChanges,
    /// The hunk failed to apply for the specified reason.
    Failed(HunkApplyError),
}

/// Options for configuring how a patch is applied.
///
/// This struct controls the behavior of patch application functions like
/// [`apply_patch_to_file()`] and [`apply_patch_to_content()`]. It allows you to
/// enable dry-run mode, configure the fuzzy matching threshold, and more.
///
/// While you can construct it directly, it's often more convenient to use one of
/// the associated functions like [`ApplyOptions::new()`], [`ApplyOptions::dry_run()`],
/// or the fluent [`with_dry_run()`](ApplyOptions::with_dry_run) and
/// [`with_fuzz_factor()`](ApplyOptions::with_fuzz_factor) methods.
///
/// # Example
///
/// ```
/// use mpatch::ApplyOptions;
///
/// // Direct construction for full control.
/// let custom_options = ApplyOptions {
///     dry_run: true,
///     fuzz_factor: 0.9,
/// };
///
/// // Using a convenience constructor for common cases.
/// let dry_run_options = ApplyOptions::dry_run();
/// assert_eq!(dry_run_options.dry_run, true);
///
/// // Using fluent methods for a chainable style.
/// let fluent_options = ApplyOptions::new()
///     .with_dry_run(true)
///     .with_fuzz_factor(0.5);
/// assert_eq!(fluent_options.fuzz_factor, 0.5);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ApplyOptions {
    /// If `true`, no files will be modified. Instead, a diff of the proposed
    /// changes will be generated and returned in [`PatchResult`].
    ///
    /// This is the primary way to preview the outcome of a patch operation without
    /// making any changes to the filesystem. When `dry_run` is enabled, functions
    /// like [`apply_patch_to_file()`] will populate the `diff` field of the
    /// returned [`PatchResult`].
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// // Create options for a dry run.
    /// let options = ApplyOptions {
    ///     dry_run: true,
    ///     fuzz_factor: 0.7,
    /// };
    ///
    /// assert!(options.dry_run);
    /// ```
    pub dry_run: bool,
    /// The similarity threshold for fuzzy matching (0.0 to 1.0).
    /// Higher is stricter. `0.0` disables fuzzy matching.
    pub fuzz_factor: f32,
}

impl Default for ApplyOptions {
    /// Creates a new [`ApplyOptions`] instance with default values.
    ///
    /// This is the standard way to get a default configuration, which has `dry_run`
    /// set to `false` and `fuzz_factor` set to `0.7`.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options: ApplyOptions = Default::default();
    ///
    /// assert_eq!(options.dry_run, false);
    /// assert_eq!(options.fuzz_factor, 0.7);
    /// ```
    fn default() -> Self {
        Self {
            dry_run: false,
            fuzz_factor: 0.7,
        }
    }
}

impl ApplyOptions {
    /// Creates a new [`ApplyOptions`] instance with default values.
    ///
    /// This is an alias for `ApplyOptions::default()`.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions::new();
    /// assert_eq!(options.dry_run, false);
    /// assert_eq!(options.fuzz_factor, 0.7);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new [`ApplyOptions`] instance configured for a dry run.
    ///
    /// All other options are set to their default values.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions::dry_run();
    /// assert_eq!(options.dry_run, true);
    /// assert_eq!(options.fuzz_factor, 0.7);
    /// ```
    pub fn dry_run() -> Self {
        Self {
            dry_run: true,
            ..Self::default()
        }
    }

    /// Creates a new [`ApplyOptions`] instance configured for an exact match (fuzz factor 0.0).
    ///
    /// All other options are set to their default values.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions::exact();
    ///
    /// assert_eq!(options.dry_run, false);
    /// assert_eq!(options.fuzz_factor, 0.0);
    /// ```
    pub fn exact() -> Self {
        Self {
            fuzz_factor: 0.0,
            ..Self::default()
        }
    }

    /// Returns a new [`ApplyOptions`] instance with the `dry_run` flag set.
    ///
    /// This is a fluent method that allows for chaining.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions::new().with_dry_run(true);
    /// assert_eq!(options.dry_run, true);
    ///
    /// let options2 = options.with_dry_run(false);
    /// assert_eq!(options2.dry_run, false);
    /// ```
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Returns a new [`ApplyOptions`] instance with the `fuzz_factor` set.
    ///
    /// This is a fluent method that allows for chaining.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions::new().with_fuzz_factor(0.9);
    /// assert_eq!(options.fuzz_factor, 0.9);
    ///
    /// let options2 = options.with_fuzz_factor(0.5);
    /// assert_eq!(options2.fuzz_factor, 0.5);
    /// ```
    pub fn with_fuzz_factor(mut self, fuzz_factor: f32) -> Self {
        self.fuzz_factor = fuzz_factor;
        self
    }

    /// Creates a new builder for [`ApplyOptions`].
    ///
    /// This provides a classic builder pattern for constructing an [`ApplyOptions`] struct,
    /// which can be useful when the configuration is built conditionally or comes from
    /// multiple sources.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions::builder()
    ///     .dry_run(true)
    ///     .fuzz_factor(0.8)
    ///     .build();
    ///
    /// assert_eq!(options.dry_run, true);
    /// assert_eq!(options.fuzz_factor, 0.8);
    /// ```
    pub fn builder() -> ApplyOptionsBuilder {
        ApplyOptionsBuilder::default()
    }
}

/// Creates a new builder for [`ApplyOptions`].
///
/// This provides a classic builder pattern for constructing an [`ApplyOptions`] struct,
/// which can be useful when the configuration is built conditionally or comes from
/// multiple sources.
///
/// # Example
///
/// ```
/// use mpatch::ApplyOptions;
///
/// let mut builder = ApplyOptions::builder();
/// let is_dry_run = true;
///
/// if is_dry_run {
///     builder = builder.dry_run(true);
/// }
///
/// let options = builder.fuzz_factor(0.8).build();
///
/// assert_eq!(options.dry_run, true);
/// assert_eq!(options.fuzz_factor, 0.8);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplyOptionsBuilder {
    dry_run: Option<bool>,
    fuzz_factor: Option<f32>,
}

impl ApplyOptionsBuilder {
    /// Sets the `dry_run` flag for the patch operation.
    ///
    /// If `true`, no files will be modified. Instead, a diff of the proposed
    /// changes will be generated and returned in [`PatchResult`]. This is useful
    /// for previewing changes before they are applied.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions::builder()
    ///     .dry_run(true) // Enable dry-run mode
    ///     .build();
    /// assert!(options.dry_run);
    /// ```
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = Some(dry_run);
        self
    }

    /// Sets the similarity threshold for fuzzy matching.
    ///
    /// The `fuzz_factor` is a value between 0.0 and 1.0 that determines how
    /// closely a block of text in the target file must match a hunk's context
    /// to be considered a "fuzzy match". A higher value requires a closer match.
    /// Setting it to `0.0` disables fuzzy matching entirely, requiring an exact match.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// // Require a very high similarity (90%) for a fuzzy match to be accepted.
    /// let options = ApplyOptions::builder().fuzz_factor(0.9).build();
    /// assert_eq!(options.fuzz_factor, 0.9);
    /// ```
    pub fn fuzz_factor(mut self, fuzz_factor: f32) -> Self {
        self.fuzz_factor = Some(fuzz_factor);
        self
    }

    /// Builds the [`ApplyOptions`] struct from the builder's configuration.
    ///
    /// This method consumes the builder and returns a final [`ApplyOptions`] instance.
    /// Any options not explicitly set on the builder will fall back to their
    /// default values.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions::builder()
    ///     .dry_run(true) // Finalize the configuration
    ///     .fuzz_factor(0.8)
    ///     .build();
    ///
    /// assert!(options.dry_run);
    /// assert_eq!(options.fuzz_factor, 0.8);
    /// ```
    pub fn build(self) -> ApplyOptions {
        let default = ApplyOptions::default();
        ApplyOptions {
            dry_run: self.dry_run.unwrap_or(default.dry_run),
            fuzz_factor: self.fuzz_factor.unwrap_or(default.fuzz_factor),
        }
    }
}

/// The result of an [`apply_patch_to_file()`] operation.
///
/// This struct is returned when a patch is applied to the filesystem. It contains
/// a detailed report of the outcome for each hunk and, if a dry run was performed,
/// a diff of the proposed changes.
///
/// # Example
///
/// ````
/// # use mpatch::{parse_single_patch, apply_patch_to_file, ApplyOptions};
/// # use std::fs;
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let file_path = dir.path().join("test.txt");
/// fs::write(&file_path, "line one\n")?;
///
/// let diff = r#"
/// ```diff
/// --- a/test.txt
/// +++ b/test.txt
/// @@ -1 +1 @@
/// -line one
/// +line 1
/// ```
/// "#;
/// let patch = parse_single_patch(diff)?;
///
/// // Perform a dry run to get a diff.
/// let options = ApplyOptions::dry_run();
/// let result = apply_patch_to_file(&patch, dir.path(), options)?;
///
/// // Check the report.
/// assert!(result.report.all_applied_cleanly());
///
/// // Inspect the generated diff.
/// assert!(result.diff.is_some());
/// println!("Proposed changes:\n{}", result.diff.unwrap());
/// # Ok(())
/// # }
/// ````
#[derive(Debug, Clone, PartialEq)]
pub struct PatchResult {
    /// Detailed results for each hunk within the patch operation.
    pub report: ApplyResult,
    /// The unified diff of the proposed changes. This is only populated
    /// when `dry_run` was set to `true` in [`ApplyOptions`].
    pub diff: Option<String>,
}

/// The result of an in-memory patch operation.
///
/// This struct is returned by functions like [`apply_patch_to_content()`] and
/// [`apply_patch_to_lines()`]. It contains the newly generated content as a string,
/// along with a detailed report of the outcome for each hunk.
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_single_patch, apply_patch_to_content, ApplyOptions};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_content = "line one\n";
/// let diff = r#"
/// ```diff
/// --- a/test.txt
/// +++ b/test.txt
/// @@ -1 +1 @@
/// -line one
/// +line 1
/// ```
/// "#;
/// let patch = parse_single_patch(diff)?;
/// let options = ApplyOptions::new();
///
/// let result = apply_patch_to_content(&patch, Some(original_content), &options);
///
/// assert!(result.report.all_applied_cleanly());
/// assert_eq!(result.new_content, "line 1\n");
/// # Ok(())
/// # }
/// ````
#[derive(Debug, Clone, PartialEq)]
pub struct InMemoryResult {
    /// The new content after applying the patch.
    pub new_content: String,
    /// Detailed results for each hunk within the patch operation.
    pub report: ApplyResult,
}

/// Contains detailed results for each hunk within a patch operation.
///
/// This struct provides a granular report on the outcome of a patch application.
/// It is a key component of both [`PatchResult`] and [`InMemoryResult`]. You can
/// use its methods like [`all_applied_cleanly()`](Self::all_applied_cleanly) for a
/// high-level summary or [`failures()`](Self::failures) to inspect specific issues.
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_single_patch, apply_patch_to_content, ApplyOptions, HunkApplyError};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_content = "line 1\n";
/// // This patch will fail because the context is wrong.
/// let diff = r#"
/// ```diff
/// --- a/test.txt
/// +++ b/test.txt
/// @@ -1 +1 @@
/// -WRONG CONTEXT
/// +line 1
/// ```
/// "#;
/// let patch = parse_single_patch(diff)?;
/// let options = ApplyOptions::exact();
///
/// let result = apply_patch_to_content(&patch, Some(original_content), &options);
/// let report = result.report; // This is the ApplyResult
///
/// assert!(!report.all_applied_cleanly());
/// assert_eq!(report.failure_count(), 1);
///
/// let failure = &report.failures()[0];
/// assert_eq!(failure.hunk_index, 1);
/// assert!(matches!(failure.reason, HunkApplyError::ContextNotFound));
/// # Ok(())
/// # }
/// ````
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyResult {
    /// A list of statuses, one for each hunk in the original patch.
    pub hunk_results: Vec<HunkApplyStatus>,
}

/// Details about a hunk that failed to apply.
///
/// This struct is returned by [`ApplyResult::failures()`] and provides a convenient
/// way to inspect which hunk failed and for what reason.
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_single_patch, apply_patch_to_content, ApplyOptions, HunkApplyError, HunkFailure};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_content = "line 1\n";
/// let diff = r#"
/// ```diff
/// --- a/test.txt
/// +++ b/test.txt
/// @@ -1 +1 @@
/// -WRONG CONTEXT
/// +line 1
/// ```
/// "#;
/// let patch = parse_single_patch(diff)?;
/// let options = ApplyOptions::exact();
///
/// let result = apply_patch_to_content(&patch, Some(original_content), &options);
/// let failures: Vec<HunkFailure> = result.report.failures();
///
/// assert_eq!(failures.len(), 1);
/// let failure = &failures[0];
///
/// assert_eq!(failure.hunk_index, 1); // 1-based index
/// assert!(matches!(failure.reason, HunkApplyError::ContextNotFound));
/// # Ok(())
/// # }
/// ````
#[derive(Debug, Clone, PartialEq)]
pub struct HunkFailure {
    /// The 1-based index of the hunk that failed.
    pub hunk_index: usize,
    /// The reason for the failure.
    pub reason: HunkApplyError,
}

impl ApplyResult {
    /// Checks if all hunks in the patch were applied successfully or skipped.
    ///
    /// Returns `false` if any hunk failed to apply.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{ApplyResult, HunkApplyStatus, HunkApplyError, HunkFailure, HunkLocation, MatchType};
    /// let successful_result = ApplyResult {
    ///     hunk_results: vec![
    ///         HunkApplyStatus::Applied { location: HunkLocation { start_index: 0, length: 1 }, match_type: MatchType::Exact, replaced_lines: vec!["old".to_string()] },
    ///         HunkApplyStatus::SkippedNoChanges
    ///     ],
    /// };
    /// assert!(successful_result.all_applied_cleanly());
    ///
    /// let failed_result = ApplyResult {
    ///     hunk_results: vec![
    ///         HunkApplyStatus::Applied { location: HunkLocation { start_index: 0, length: 1 }, match_type: MatchType::Exact, replaced_lines: vec!["old".to_string()] },
    ///         HunkApplyStatus::Failed(HunkApplyError::ContextNotFound),
    ///     ],
    /// };
    /// assert!(!failed_result.all_applied_cleanly());
    /// ```
    pub fn all_applied_cleanly(&self) -> bool {
        self.hunk_results
            .iter()
            .all(|r| !matches!(r, HunkApplyStatus::Failed(_)))
    }

    /// Returns a list of all hunks that failed to apply, along with their index.
    ///
    /// This provides a more convenient way to inspect failures than iterating
    /// through [`hunk_results`](Self::hunk_results) manually.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{ApplyResult, HunkApplyStatus, HunkApplyError, HunkFailure, HunkLocation, MatchType};
    /// let failed_result = ApplyResult {
    ///     hunk_results: vec![
    ///         // The first hunk applied successfully.
    ///         HunkApplyStatus::Applied { location: HunkLocation { start_index: 0, length: 1 }, match_type: MatchType::Exact, replaced_lines: vec!["old".to_string()] },
    ///         HunkApplyStatus::Failed(HunkApplyError::ContextNotFound),
    ///     ],
    /// };
    /// let failures = failed_result.failures();
    /// assert_eq!(failures.len(), 1);
    /// assert_eq!(failures[0], HunkFailure {
    ///     hunk_index: 2, // 1-based index
    ///     reason: HunkApplyError::ContextNotFound,
    /// });
    /// ```
    pub fn failures(&self) -> Vec<HunkFailure> {
        self.hunk_results
            .iter()
            .enumerate()
            .filter_map(|(i, status)| {
                if let HunkApplyStatus::Failed(reason) = status {
                    Some(HunkFailure {
                        hunk_index: i + 1,
                        reason: reason.clone(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }
}

/// The result of applying a batch of patches to a directory.
///
/// This struct is returned by [`apply_patches_to_dir()`] and aggregates the results
/// for each individual patch operation. It allows you to check for "hard" errors
/// (like I/O issues) separately from "soft" errors (like a hunk failing to apply).
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_auto, apply_patches_to_dir, ApplyOptions};
/// # use std::fs;
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// fs::write(dir.path().join("file1.txt"), "foo\n")?;
/// fs::write(dir.path().join("file2.txt"), "baz\n")?;
///
/// let diff = r#"
/// ```diff
/// --- a/file1.txt
/// +++ b/file1.txt
/// @@ -1 +1 @@
/// -foo
/// +bar
/// --- a/file2.txt
/// +++ b/file2.txt
/// @@ -1 +1 @@
/// -WRONG
/// +qux
/// ```
/// "#;
/// let patches = parse_auto(diff)?;
/// let options = ApplyOptions::exact();
///
/// let batch_result = apply_patches_to_dir(&patches, dir.path(), options);
///
/// // The overall batch succeeded (no I/O errors).
/// assert!(batch_result.all_succeeded());
///
/// // But we can inspect individual results for partial failures.
/// for (path, result) in &batch_result.results {
///     let patch_result = result.as_ref().unwrap();
///     if path.to_str() == Some("file1.txt") {
///         assert!(patch_result.report.all_applied_cleanly());
///     } else {
///         assert!(!patch_result.report.all_applied_cleanly());
///     }
/// }
/// # Ok(())
/// # }
/// ````
#[derive(Debug)]
pub struct BatchResult {
    /// A list of results for each patch operation attempted.
    /// Each entry is a tuple of the target file path and the result of the operation.
    pub results: Vec<(PathBuf, Result<PatchResult, PatchError>)>,
}

impl BatchResult {
    /// Checks if all patches in the batch were applied without "hard" errors (like I/O errors).
    /// This does *not* check if all hunks were applied cleanly. For that, you must
    /// inspect the individual `PatchResult` objects.
    ///
    /// # Example
    ///
    /// ````rust
    /// # use mpatch::{parse_auto, apply_patches_to_dir, ApplyOptions};
    /// # use std::fs;
    /// # use tempfile::tempdir;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let dir = tempdir()?;
    /// fs::write(dir.path().join("file1.txt"), "foo\n")?;
    /// // Note: file2.txt does not exist, which will cause a hard error.
    ///
    /// let diff = r#"
    /// ```diff
    /// --- a/file1.txt
    /// +++ b/file1.txt
    /// @@ -1 +1 @@
    /// -foo
    /// +bar
    /// --- a/file2.txt
    /// +++ b/file2.txt
    /// @@ -1 +1 @@
    /// -baz
    /// +qux
    /// ```
    /// "#;
    /// let patches = parse_auto(diff)?;
    /// let options = ApplyOptions::new();
    ///
    /// let batch_result = apply_patches_to_dir(&patches, dir.path(), options);
    ///
    /// // The batch did not fully succeed because of the missing file.
    /// assert!(!batch_result.all_succeeded());
    /// # Ok(())
    /// # }
    /// ````
    pub fn all_succeeded(&self) -> bool {
        self.results.iter().all(|(_, res)| res.is_ok())
    }

    /// Returns a list of all operations that resulted in a "hard" error (e.g., I/O).
    ///
    /// This method is useful for isolating critical failures that prevented a patch
    /// from being attempted, such as file system errors, permission issues, or
    /// security violations. It filters the results to only include `Err` variants,
    /// providing a direct way to report or handle unrecoverable problems in a batch
    /// run.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use mpatch::{parse_auto, apply_patches_to_dir, ApplyOptions, PatchError};
    /// # use std::fs;
    /// # use tempfile::tempdir;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let dir = tempdir()?;
    /// # let diff = "```diff\n--- a/missing.txt\n+++ b/missing.txt\n@@ -1 +1 @@\n-a\n+b\n```";
    /// # let patches = parse_auto(diff)?;
    /// let batch_result = apply_patches_to_dir(&patches, dir.path(), ApplyOptions::new());
    ///
    /// // Check for any hard failures in the batch.
    /// let failures = batch_result.hard_failures();
    /// assert_eq!(failures.len(), 1);
    /// assert_eq!(failures[0].0.to_str(), Some("missing.txt"));
    /// assert!(matches!(failures[0].1, PatchError::TargetNotFound(_)));
    /// # Ok(())
    /// # }
    /// ```
    pub fn hard_failures(&self) -> Vec<(&PathBuf, &PatchError)> {
        self.results
            .iter()
            .filter_map(|(path, res)| res.as_ref().err().map(|e| (path, e)))
            .collect()
    }
}

impl ApplyResult {
    /// Checks if any hunk in the patch failed to apply.
    ///
    /// This is the logical opposite of [`all_applied_cleanly`](Self::all_applied_cleanly).
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{ApplyResult, HunkApplyStatus, HunkApplyError, HunkLocation, MatchType};
    /// let failed_result = ApplyResult {
    ///     hunk_results: vec![
    ///         HunkApplyStatus::Applied { location: HunkLocation { start_index: 0, length: 1 }, match_type: MatchType::Exact, replaced_lines: vec!["old".to_string()] },
    ///         HunkApplyStatus::Failed(HunkApplyError::ContextNotFound),
    ///     ],
    /// };
    /// assert!(failed_result.has_failures());
    ///
    /// let successful_result = ApplyResult {
    ///     hunk_results: vec![ HunkApplyStatus::SkippedNoChanges ],
    /// };
    /// assert!(!successful_result.has_failures());
    /// ```
    pub fn has_failures(&self) -> bool {
        !self.all_applied_cleanly()
    }

    /// Returns the number of hunks that failed to apply.
    ///
    /// This is a convenience method that counts how many hunks in the `hunk_results`
    /// list have a status of `HunkApplyStatus::Failed`.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{ApplyResult, HunkApplyStatus, HunkApplyError, HunkLocation, MatchType};
    /// let result = ApplyResult {
    ///     hunk_results: vec![
    ///         HunkApplyStatus::Applied { location: HunkLocation { start_index: 0, length: 1 }, match_type: MatchType::Exact, replaced_lines: vec!["old".to_string()] },
    ///         HunkApplyStatus::Failed(HunkApplyError::ContextNotFound),
    ///         HunkApplyStatus::Failed(HunkApplyError::AmbiguousExactMatch(vec![])),
    ///     ],
    /// };
    /// assert_eq!(result.failure_count(), 2);
    /// ```
    pub fn failure_count(&self) -> usize {
        self.failures().len()
    }

    /// Returns the number of hunks that were applied successfully or skipped.
    ///
    /// This method counts how many hunks in the `hunk_results` list have a status
    /// of either `HunkApplyStatus::Applied` or `HunkApplyStatus::SkippedNoChanges`.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{ApplyResult, HunkApplyStatus, HunkApplyError, HunkLocation, MatchType};
    /// let result = ApplyResult {
    ///     hunk_results: vec![
    ///         HunkApplyStatus::Applied { location: HunkLocation { start_index: 0, length: 1 }, match_type: MatchType::Exact, replaced_lines: vec!["old".to_string()] },
    ///         HunkApplyStatus::SkippedNoChanges,
    ///         HunkApplyStatus::Failed(HunkApplyError::ContextNotFound),
    ///     ],
    /// };
    /// assert_eq!(result.success_count(), 2);
    /// ```
    pub fn success_count(&self) -> usize {
        self.hunk_results.len() - self.failure_count()
    }
}

// --- Data Structures ---

/// Represents a single hunk of changes within a patch.
///
/// Structurally, this models a hunk from a Unified Diff (the `@@ ... @@` blocks),
/// storing lines prefixed with `+`, `-`, or space. However, it serves as the
/// universal internal representation for all patch formats in `mpatch`.
///
/// - **Unified Diffs:** Parsed directly.
/// - **Conflict Markers:** Converted into a `Hunk` where the "old" block becomes
///   deletions and the "new" block becomes additions.
///
/// You typically get `Hunk` objects as part of a [`Patch`] after parsing a diff.
///
/// # Example
///
/// ````rust
/// # use mpatch::parse_single_patch;
/// let diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -10,2 +10,2 @@
///  context line
/// -removed line
/// +added line
/// ```
/// "#;
/// let patch = parse_single_patch(diff).unwrap();
/// let hunk = &patch.hunks[0];
///
/// assert_eq!(hunk.old_start_line, Some(10));
/// assert_eq!(hunk.removed_lines(), vec!["removed line"]);
/// assert_eq!(hunk.added_lines(), vec!["added line"]);
///
/// // You can convert the hunk back to a unified diff string:
/// assert_eq!(hunk.to_string(), "@@ -10,2 +10,2 @@\n context line\n-removed line\n+added line\n");
/// ````
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// The raw lines of the hunk, each prefixed with ' ', '+', or '-'.
    ///
    /// This vector stores the content exactly as it would appear in a Unified Diff body.
    /// Lines starting with ` ` are context, `-` are deletions, and `+` are additions.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, Hunk};
    /// # let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,2 +1,2\n-a\n+b\n```";
    /// # let patch = parse_single_patch(diff).unwrap();
    /// let hunk = &patch.hunks[0];
    ///
    /// // Iterate over the raw lines
    /// for line in &hunk.lines {
    ///     if line.starts_with('+') {
    ///         println!("Added line: {}", &line[1..]);
    ///     }
    /// }
    /// ```
    ///
    /// When parsing Conflict Markers, `mpatch` synthesizes these lines: the "before"
    /// block becomes `-` lines, and the "after" block becomes `+` lines.
    pub lines: Vec<String>,
    /// The starting line number in the original file (1-based).
    ///
    /// This corresponds to the `l` in the `@@ -l,s ...` header of a unified diff.
    /// In `mpatch`, this value is primarily used as a **hint** to resolve ambiguity.
    /// If the context matches in multiple places, the location closest to this line
    /// is chosen.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{Hunk};
    /// let hunk = Hunk {
    ///     lines: vec!["-old".to_string()],
    ///     old_start_line: Some(10), // Hint: look near line 10
    ///     new_start_line: Some(10),
    /// };
    /// ```
    ///
    /// This may be `None` if the patch source (like Conflict Markers) did not provide line numbers.
    pub old_start_line: Option<usize>,
    /// The starting line number in the new file (1-based).
    ///
    /// This corresponds to the `l` in the `@@ ... +l,s @@` header of a unified diff.
    /// It represents the intended location in the resulting file.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{Hunk};
    /// let hunk = Hunk {
    ///     lines: vec!["+new".to_string()],
    ///     old_start_line: Some(10),
    ///     new_start_line: Some(12), // Lines shifted down by 2
    /// };
    /// ```
    ///
    /// This may be `None` if the patch source did not provide line numbers.
    pub new_start_line: Option<usize>,
}

impl Hunk {
    /// Creates a new `Hunk` that reverses the changes in this one.
    ///
    /// Additions become deletions, and deletions become additions. Context lines
    /// remain unchanged. The old and new line number hints are swapped.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk = Hunk {
    ///     lines: vec![
    ///         " context".to_string(),
    ///         "-deleted".to_string(),
    ///         "+added".to_string(),
    ///     ],
    ///     old_start_line: Some(10),
    ///     new_start_line: Some(12),
    /// };
    /// let inverted_hunk = hunk.invert();
    /// assert_eq!(inverted_hunk.lines, vec![
    ///     " context".to_string(),
    ///     "+deleted".to_string(),
    ///     "-added".to_string(),
    /// ]);
    /// assert_eq!(inverted_hunk.old_start_line, Some(12));
    /// assert_eq!(inverted_hunk.new_start_line, Some(10));
    /// ```
    pub fn invert(&self) -> Hunk {
        let inverted_lines = self
            .lines
            .iter()
            .map(|line| {
                if let Some(stripped) = line.strip_prefix('+') {
                    format!("-{}", stripped)
                } else if let Some(stripped) = line.strip_prefix('-') {
                    format!("+{}", stripped)
                } else {
                    line.clone()
                }
            })
            .collect();

        Hunk {
            lines: inverted_lines,
            old_start_line: self.new_start_line,
            new_start_line: self.old_start_line,
        }
    }

    /// Extracts the lines that need to be matched in the target file.
    ///
    /// This includes context lines (starting with ' ') and deletion lines
    /// (starting with '-'). The leading character is stripped. These lines form
    /// the "search pattern" that `mpatch` looks for in the target file.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk = Hunk {
    ///     lines: vec![
    ///         " context".to_string(),
    ///         "-deleted".to_string(),
    ///         "+added".to_string(),
    ///     ],
    ///     old_start_line: None,
    ///     new_start_line: None,
    /// };
    /// assert_eq!(hunk.get_match_block(), vec!["context", "deleted"]);
    /// ```
    pub fn get_match_block(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| !l.starts_with('+'))
            .map(|l| &l[1..])
            .collect()
    }

    /// Extracts the lines that will replace the matched block in the target file.
    ///
    /// This includes context lines (starting with ' ') and addition lines
    /// (starting with '+'). The leading character is stripped. This is the
    /// content that will be "spliced" into the file.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk = Hunk {
    ///     lines: vec![
    ///         " context".to_string(),
    ///         "-deleted".to_string(),
    ///         "+added".to_string(),
    ///     ],
    ///     old_start_line: None,
    ///     new_start_line: None,
    /// };
    /// assert_eq!(hunk.get_replace_block(), vec!["context", "added"]);
    /// ```
    pub fn get_replace_block(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| !l.starts_with('-'))
            .map(|l| &l[1..])
            .collect()
    }

    /// Extracts the context lines from the hunk.
    ///
    /// These are lines that start with ' ' and are stripped of the prefix.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk = Hunk {
    ///     lines: vec![
    ///         " context".to_string(),
    ///         "-deleted".to_string(),
    ///         "+added".to_string(),
    ///     ],
    ///     old_start_line: None,
    ///     new_start_line: None,
    /// };
    /// assert_eq!(hunk.context_lines(), vec!["context"]);
    /// ```
    pub fn context_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| l.starts_with(' '))
            .map(|l| &l[1..])
            .collect()
    }

    /// Extracts the added lines from the hunk.
    ///
    /// These are lines that start with '+' and are stripped of the prefix.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk = Hunk {
    ///     lines: vec![
    ///         " context".to_string(),
    ///         "-deleted".to_string(),
    ///         "+added".to_string(),
    ///     ],
    ///     old_start_line: None,
    ///     new_start_line: None,
    /// };
    /// assert_eq!(hunk.added_lines(), vec!["added"]);
    /// ```
    pub fn added_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| l.starts_with('+'))
            .map(|l| &l[1..])
            .collect()
    }

    /// Extracts the removed lines from the hunk.
    ///
    /// These are lines that start with '-' and are stripped of the prefix.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk = Hunk {
    ///     lines: vec![
    ///         " context".to_string(),
    ///         "-deleted".to_string(),
    ///         "+added".to_string(),
    ///     ],
    ///     old_start_line: None,
    ///     new_start_line: None,
    /// };
    /// assert_eq!(hunk.removed_lines(), vec!["deleted"]);
    /// ```
    pub fn removed_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| l.starts_with('-'))
            .map(|l| &l[1..])
            .collect()
    }

    /// Checks if the hunk contains any effective changes (additions or deletions).
    ///
    /// A hunk with only context lines has no changes and can be skipped.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk_with_changes = Hunk {
    ///     lines: vec![ "+ a".to_string() ],
    ///     old_start_line: None,
    ///     new_start_line: None,
    /// };
    /// assert!(hunk_with_changes.has_changes());
    ///
    /// let hunk_without_changes = Hunk {
    ///     lines: vec![ " a".to_string() ],
    ///     old_start_line: None,
    ///     new_start_line: None,
    /// };
    /// assert!(!hunk_without_changes.has_changes());
    /// ```
    pub fn has_changes(&self) -> bool {
        self.lines.iter().any(|l| l.starts_with(['+', '-']))
    }
}

impl std::fmt::Display for Hunk {
    /// Formats the hunk into a valid unified diff hunk block.
    ///
    /// This generates the `@@ ... @@` header based on the start lines and the
    /// count of lines in the `lines` vector, followed by the content. This allows
    /// any `Hunk` (even those from Conflict Markers) to be serialized as standard diffs.
    ///
    /// If `old_start_line` or `new_start_line` are `None`, they default to `1` in the output.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk = Hunk {
    ///     lines: vec![
    ///         " context".to_string(),
    ///         "-deleted".to_string(),
    ///         "+added".to_string(),
    ///     ],
    ///     old_start_line: Some(10),
    ///     new_start_line: Some(12),
    /// };
    /// let expected_str = "@@ -10,2 +12,2 @@\n context\n-deleted\n+added\n";
    /// assert_eq!(hunk.to_string(), expected_str);
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let old_len = self.lines.iter().filter(|l| !l.starts_with('+')).count();
        let new_len = self.lines.iter().filter(|l| !l.starts_with('-')).count();
        let old_start = self.old_start_line.unwrap_or(1);
        let new_start = self.new_start_line.unwrap_or(1);

        writeln!(
            f,
            "@@ -{},{} +{},{} @@",
            old_start, old_len, new_start, new_len
        )?;

        for line in &self.lines {
            writeln!(f, "{}", line)?;
        }
        Ok(())
    }
}

/// Represents the location where a hunk should be applied.
///
/// This is returned by [`find_hunk_location()`] and provides the necessary
/// information to manually apply a patch to a slice of lines.
///
/// # Example
///
/// ````rust
/// # use mpatch::{find_hunk_location, parse_single_patch, ApplyOptions, HunkLocation, MatchType};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_content = "line 1\nline 2\nline 3\n";
/// let diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,3 +1,3 @@
///  line 1
/// -line 2
/// +line two
///  line 3
/// ```
/// "#;
/// let hunk = parse_single_patch(diff)?.hunks.remove(0);
/// let options = ApplyOptions::exact();
///
/// let (location, _) = find_hunk_location(&hunk, original_content, &options)?;
///
/// assert_eq!(location.start_index, 0); // 0-based index
/// assert_eq!(location.length, 3);
/// assert_eq!(location.to_string(), "line 1"); // User-friendly 1-based display
/// # Ok(())
/// # }
/// ````
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HunkLocation {
    /// The 0-based starting line index in the target content where the hunk should be applied.
    ///
    /// This index indicates the first line of the slice in the target content that
    /// will be replaced by the hunk's changes. You can use this along with the
    /// `length` field to understand the exact range of lines affected by the patch.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{HunkLocation};
    /// let location = HunkLocation { start_index: 4, length: 3 };
    ///
    /// // Note that the user-facing line number is start_index + 1.
    /// assert_eq!(location.start_index, 4);
    /// println!(
    ///     "Patch will be applied starting at line {} (index {}).",
    ///     location.start_index + 1,
    ///     location.start_index
    /// );
    /// ```
    pub start_index: usize,
    /// The number of lines in the target content that will be replaced. This may
    /// differ from the number of lines in the hunk's "match block" when a fuzzy
    /// match is found.
    pub length: usize,
}

/// Formats the location for display, showing a user-friendly 1-based line number.
///
/// This implementation provides a more intuitive, human-readable representation of the
/// hunk's location. It converts the internal 0-based `start_index` into a 1-based
/// line number (e.g., index `9` becomes `"line 10"`), which is the standard
/// convention in text editors and log messages. This makes it easy to use
/// `HunkLocation` directly in formatted strings for clear diagnostic output.
///
/// # Example
///
/// ```
/// # use mpatch::HunkLocation;
/// let location = HunkLocation { start_index: 9, length: 3 };
/// assert_eq!(location.to_string(), "line 10");
/// assert_eq!(format!("Hunk applied at {}", location), "Hunk applied at line 10");
/// ```
impl std::fmt::Display for HunkLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Adding 1 to start_index for a more user-friendly 1-based line number.
        write!(f, "line {}", self.start_index + 1)
    }
}

/// Represents all the changes to be applied to a single file.
///
/// A `Patch` contains a target file path and a list of [`Hunk`]s. It is typically
/// created by parsing a diff string (Unified Diff, Markdown block, or Conflict Markers)
/// using functions like [`parse_auto()`] or [`parse_diffs()`].
///
/// It is the primary unit of work for the `apply` functions.
///
/// # Example
///
/// ````rust
/// # use mpatch::parse_single_patch;
/// let diff = r#"
/// ```diff
/// --- a/src/main.rs
/// +++ b/src/main.rs
/// @@ -1,3 +1,3 @@
///  fn main() {
/// -    println!("Hello, world!");
/// +    println!("Hello, mpatch!");
///  }
/// ```
/// "#;
/// let patch = parse_single_patch(diff).unwrap();
///
/// assert_eq!(patch.file_path.to_str(), Some("src/main.rs"));
/// assert_eq!(patch.hunks.len(), 1);
/// assert!(patch.ends_with_newline);
/// ````
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// The relative path of the file to be patched, from the target directory.
    ///
    /// This path is extracted from the `--- a/path/to/file` header in the diff.
    /// It's a `PathBuf`, so you can use it directly with filesystem operations
    /// or convert it to a string for display.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::parse_single_patch;
    /// # let diff = "```diff\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,1 +1,1\n-a\n+b\n```";
    /// let patch = parse_single_patch(diff).unwrap();
    ///
    /// assert_eq!(patch.file_path.to_str(), Some("src/main.rs"));
    /// println!("Patch targets the file: {}", patch.file_path.display());
    /// ```
    pub file_path: PathBuf,
    /// A list of hunks to be applied to the file.
    pub hunks: Vec<Hunk>,
    /// Indicates whether the file should end with a newline.
    /// This is determined by the presence of `\ No newline at end of file`
    /// in the diff.
    pub ends_with_newline: bool,
}

impl Patch {
    /// Creates a new `Patch` by comparing two texts.
    ///
    /// This function generates a unified diff between the `old_text` and `new_text`
    /// and then parses it into a `Patch` object. This allows `mpatch` to be used
    /// not just for applying patches, but also for creating them.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to associate with the patch (e.g., `src/main.rs`).
    /// * `old_text` - The original text content.
    /// * `new_text` - The new, modified text content.
    /// * `context_len` - The number of context lines to include around changes.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::Patch;
    /// let old_code = "fn main() {\n    println!(\"old\");\n}\n";
    /// let new_code = "fn main() {\n    println!(\"new\");\n}\n";
    ///
    /// let patch = Patch::from_texts("src/main.rs", old_code, new_code, 3).unwrap();
    ///
    /// assert_eq!(patch.file_path.to_str(), Some("src/main.rs"));
    /// assert_eq!(patch.hunks.len(), 1);
    /// assert_eq!(patch.hunks[0].removed_lines(), vec!["    println!(\"old\");"]);
    /// assert_eq!(patch.hunks[0].added_lines(), vec!["    println!(\"new\");"]);
    /// ```
    pub fn from_texts(
        file_path: impl Into<PathBuf>,
        old_text: &str,
        new_text: &str,
        context_len: usize,
    ) -> Result<Self, ParseError> {
        let path = file_path.into();
        let path_str = path.to_string_lossy();

        let old_header = format!("a/{}", path_str);
        let new_header = format!("b/{}", path_str);
        let diff = TextDiff::from_lines(old_text, new_text);
        let diff_text = format!(
            "{}",
            diff.unified_diff()
                .context_radius(context_len)
                .header(&old_header, &new_header)
        );

        // If there's no difference, the text will be empty.
        if diff_text.trim().is_empty() {
            return Ok(Patch {
                file_path: path,
                hunks: vec![],
                ends_with_newline: new_text.ends_with('\n') || new_text.is_empty(),
            });
        }

        // Parse the raw diff directly
        let patches = parse_patches(diff_text.trim())?;

        if let Some(patch) = patches.into_iter().next() {
            Ok(patch)
        } else {
            // This should not happen if diff_text was not empty, but as a safeguard:
            Ok(Patch {
                file_path: path,
                hunks: vec![],
                ends_with_newline: new_text.ends_with('\n') || new_text.is_empty(),
            })
        }
    }

    /// Creates a new `Patch` that reverses the changes in this one.
    ///
    /// Each hunk in the patch is inverted, swapping additions and deletions.
    /// This is useful for "un-applying" a patch.
    ///
    /// **Note:** The `ends_with_newline` status of the reversed patch is ambiguous
    /// in the unified diff format, so it defaults to `true`.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{Patch, Hunk};
    /// let patch = Patch {
    ///     file_path: "file.txt".into(),
    ///     hunks: vec![Hunk {
    ///         lines: vec![
    ///             " context".to_string(),
    ///             "-deleted".to_string(),
    ///             "+added".to_string(),
    ///         ],
    ///         old_start_line: Some(10),
    ///         new_start_line: Some(10),
    ///     }],
    ///     ends_with_newline: true,
    /// };
    ///
    /// let inverted = patch.invert();
    /// let inverted_hunk = &inverted.hunks[0];
    ///
    /// assert_eq!(inverted_hunk.removed_lines(), vec!["added"]);
    /// assert_eq!(inverted_hunk.added_lines(), vec!["deleted"]);
    /// ```
    pub fn invert(&self) -> Patch {
        Patch {
            file_path: self.file_path.clone(),
            hunks: self.hunks.iter().map(|h| h.invert()).collect(),
            // Inverting this is non-trivial. A standard diff doesn't record
            // the newline status of the original file if the new file has one.
            // We'll assume the inverted patch will result in a file with a newline.
            ends_with_newline: true,
        }
    }

    /// Checks if the patch represents a file creation.
    ///
    /// A patch is considered a creation if its first hunk is an addition-only
    /// hunk that applies to an empty file (i.e., its "match block" is empty).
    ///
    /// # Example
    ///
    /// ````
    /// # use mpatch::parse_single_patch;
    /// let creation_diff = r#"
    /// ```diff
    /// --- a/new_file.txt
    /// +++ b/new_file.txt
    /// @@ -0,0 +1,2 @@
    /// +Hello
    /// +World
    /// ```
    /// "#;
    /// let patch = parse_single_patch(creation_diff).unwrap();
    /// assert!(patch.is_creation());
    /// ````
    pub fn is_creation(&self) -> bool {
        self.hunks
            .first()
            .is_some_and(|h| h.get_match_block().is_empty())
    }

    /// Checks if the patch represents a full file deletion.
    ///
    /// A patch is considered a deletion if it contains at least one hunk, and
    /// all of its hunks result in removing content without adding any new content
    /// (i.e., their "replace blocks" are empty). This is typical for a diff
    /// that empties a file.
    ///
    /// # Example
    ///
    /// ````
    /// # use mpatch::parse_single_patch;
    /// let deletion_diff = r#"
    /// ```diff
    /// --- a/old_file.txt
    /// +++ b/old_file.txt
    /// @@ -1,2 +0,0 @@
    /// -Hello
    /// -World
    /// ```
    /// "#;
    /// let patch = parse_single_patch(deletion_diff).unwrap();
    /// assert!(patch.is_deletion());
    /// ````
    pub fn is_deletion(&self) -> bool {
        !self.hunks.is_empty() && self.hunks.iter().all(|h| h.get_replace_block().is_empty())
    }
}

impl std::fmt::Display for Patch {
    /// Formats the patch into a valid unified diff string for a single file.
    ///
    /// This provides a canonical string representation of the entire patch,
    /// including the `---` and `+++` file headers, followed by the
    /// formatted content of all its hunks. It also correctly handles the
    /// `\ No newline at end of file` marker when necessary.
    ///
    /// This is useful for logging, debugging, or serializing a `Patch` object
    /// back to its original text format.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{Patch, Hunk};
    /// let patch = Patch {
    ///     file_path: "src/main.rs".into(),
    ///     hunks: vec![Hunk {
    ///         lines: vec![
    ///             "-old".to_string(),
    ///             "+new".to_string(),
    ///         ],
    ///         old_start_line: Some(1),
    ///         new_start_line: Some(1),
    ///     }],
    ///     ends_with_newline: false, // To test the marker
    /// };
    ///
    /// let expected_output = concat!(
    ///     "--- a/src/main.rs\n",
    ///     "+++ b/src/main.rs\n",
    ///     "@@ -1,1 +1,1 @@\n",
    ///     "-old\n",
    ///     "+new\n",
    ///     "\\ No newline at end of file"
    /// );
    ///
    /// assert_eq!(patch.to_string(), expected_output);
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "--- a/{}", self.file_path.display())?;
        writeln!(f, "+++ b/{}", self.file_path.display())?;

        for hunk in &self.hunks {
            write!(f, "{}", hunk)?;
        }

        if !self.ends_with_newline && !self.hunks.is_empty() {
            write!(f, "\\ No newline at end of file")?;
        }

        Ok(())
    }
}

// --- Core Logic ---

/// Identifies the syntactic format of a patch content string.
///
/// This enum is returned by [`detect_patch()`] and used internally by
/// [`parse_auto()`] to determine which parsing strategy to apply.
///
/// It distinguishes between raw diffs (commonly output by `git diff`), diffs wrapped
/// in Markdown code blocks (commonly output by LLMs), and conflict marker blocks
/// (used in merge conflicts or specific AI suggestions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatchFormat {
    /// A standard Unified Diff format.
    ///
    /// This format is characterized by file headers starting with `---` and `+++`,
    /// or hunk headers starting with `@@`.
    ///
    /// # Example
    /// ```text
    /// --- a/file.rs
    /// +++ b/file.rs
    /// @@ -1,3 +1,3 @@
    ///  fn main() {
    /// -    println!("Old");
    /// +    println!("New");
    ///  }
    /// ```
    Unified,

    /// A Markdown file containing diff code blocks.
    ///
    /// This format is characterized by the presence of code fences (e.g., ` ```diff `)
    /// containing patch data. `mpatch` will extract and parse the content inside these blocks.
    ///
    /// # Example
    /// ````text
    /// Here is the fix for your issue:
    ///
    /// ```diff
    /// --- a/src/main.rs
    /// +++ b/src/main.rs
    /// @@ -1 +1 @@
    /// -old_function();
    /// +new_function();
    /// ```
    /// ````
    Markdown,

    /// A file containing Conflict Markers.
    ///
    /// This format is characterized by the specific markers `<<<<`, `====`, and `>>>>`.
    /// It is commonly found in Git merge conflicts or AI code suggestions that use
    /// this format to denote "before" and "after" states without full diff headers.
    ///
    /// # Example
    /// ```text
    /// fn calculate() {
    /// <<<<
    ///     return x + y;
    /// ====
    ///     return x * y;
    /// >>>>
    /// }
    /// ```
    Conflict,

    /// The format could not be determined.
    ///
    /// The content did not contain any recognizable signatures (such as diff headers,
    /// markdown fences, or conflict markers).
    Unknown,
}

/// Automatically detects the patch format of the provided content.
///
/// This function scans the content efficiently (without parsing the full structure)
/// to determine if it contains Markdown code blocks, standard unified diff headers,
/// or conflict markers.
///
/// # Priority
/// 1. **Markdown**: If code fences (3+ backticks) are found containing diff signatures, it is treated as Markdown.
/// 2. **Unified**: If `--- a/` or `diff --git` headers are found, it is treated as a Unified Diff.
/// 3. **Conflict**: If `<<<<` markers are found, it is treated as Conflict Markers.
///
/// # Example
///
/// ```rust
/// use mpatch::{detect_patch, PatchFormat};
///
/// let md = "```diff\n--- a/f\n+++ b/f\n```";
/// assert_eq!(detect_patch(md), PatchFormat::Markdown);
///
/// let raw = "--- a/f\n+++ b/f\n@@ -1 +1 @@";
/// assert_eq!(detect_patch(raw), PatchFormat::Unified);
/// ```
pub fn detect_patch(content: &str) -> PatchFormat {
    let mut lines = content.lines().peekable();
    let mut in_code_block = false;
    let mut current_fence_len = 0;
    let mut has_unified_headers = false;
    let mut has_conflict_markers = false;

    while let Some(line) = lines.next() {
        // Check for Markdown code blocks
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            let fence_len = trimmed.chars().take_while(|&c| c == '`').count();
            if fence_len >= 3 {
                if !in_code_block {
                    in_code_block = true;
                    current_fence_len = fence_len;
                    let info = &trimmed[fence_len..];
                    if info.contains("diff") || info.contains("patch") {
                        return PatchFormat::Markdown;
                    }
                } else if fence_len >= current_fence_len {
                    in_code_block = false;
                    current_fence_len = 0;
                }
                continue;
            }
        }

        // Check for Unified Diff headers
        let is_diff_git = line.starts_with("diff --git");
        let is_unified_header =
            line.starts_with("--- ") && lines.peek().is_some_and(|l| l.starts_with("+++ "));
        let is_hunk_header = line.starts_with("@@ -") && line.contains(" @@");

        if is_diff_git || is_unified_header || is_hunk_header {
            if in_code_block {
                return PatchFormat::Markdown;
            }
            has_unified_headers = true;
        }

        // Check for Conflict Markers
        let trimmed = line.trim_start();
        let is_conflict = trimmed.starts_with("<<<<")
            || trimmed.starts_with("====")
            || trimmed.starts_with(">>>>");

        if is_conflict {
            if in_code_block {
                return PatchFormat::Markdown;
            }
            has_conflict_markers = true;
        }
    }

    if has_unified_headers {
        PatchFormat::Unified
    } else if has_conflict_markers {
        PatchFormat::Conflict
    } else {
        PatchFormat::Unknown
    }
}

/// Automatically detects the format of the input text and parses it into a list of patches.
///
/// This is the recommended entry point for most use cases, as it robustly handles
/// the various ways diffs are commonly presented (e.g., inside Markdown code blocks
/// from LLMs, as raw output from `git diff`, or as conflict markers in source files).
///
/// # Supported Formats
///
/// 1.  **Markdown:** Code blocks fenced with backticks (e.g., ` ```diff `) containing
///     diff content. This is the standard output format for AI coding assistants.
/// 2.  **Unified Diff:** Standard diffs containing `--- a/path` and `+++ b/path` headers.
/// 3.  **Conflict Markers:** Blocks delimited by `<<<<`, `====`, and `>>>>`. These are
///     parsed into patches where the "old" content is removed and the "new" content is added.
///
/// # Behavior
///
/// The function first attempts to detect the format using lightweight heuristics
/// (see [`detect_patch`]).
///
/// - If **Markdown** is detected, it extracts patches from all valid code blocks.
/// - If **Unified Diff** headers are detected, it parses the entire string as a raw diff.
/// - If **Conflict Markers** are detected, it parses the blocks into patches targeting a generic file path.
/// - If the format is **Unknown**, it attempts to parse the content as a raw diff
///   as a fallback. This allows parsing fragments that might lack full file headers
///   but contain valid hunks.
///
/// # Examples
///
/// **Parsing a Markdown string:**
/// ````
/// use mpatch::parse_auto;
///
/// let md = r#"
/// Here is the fix:
/// ```diff
/// --- a/src/main.rs
/// +++ b/src/main.rs
/// @@ -1 +1 @@
/// -println!("Old");
/// +println!("New");
/// ```
/// "#;
///
/// let patches = parse_auto(md).unwrap();
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].file_path.to_str(), Some("src/main.rs"));
/// ````
///
/// **Parsing a Raw Diff:**
/// ````
/// use mpatch::parse_auto;
///
/// let raw = r#"
/// --- a/config.toml
/// +++ b/config.toml
/// @@ -1 +1 @@
/// -debug = false
/// +debug = true
/// "#;
///
/// let patches = parse_auto(raw).unwrap();
/// assert_eq!(patches.len(), 1);
/// ````
///
/// **Parsing Conflict Markers:**
/// ````
/// use mpatch::parse_auto;
///
/// let conflict = r#"
/// <<<<
/// old_code();
/// ====
/// new_code();
/// >>>>
/// "#;
///
/// let patches = parse_auto(conflict).unwrap();
/// // Conflict markers don't specify a file, so they get a generic path.
/// assert_eq!(patches[0].file_path.to_str(), Some("patch_target"));
/// ````
pub fn parse_auto(content: &str) -> Result<Vec<Patch>, ParseError> {
    match detect_patch(content) {
        PatchFormat::Markdown => parse_diffs(content),
        PatchFormat::Unified => parse_patches(content),
        PatchFormat::Conflict => Ok(parse_conflict_markers(content)),
        PatchFormat::Unknown => {
            // If unknown, we try parsing as raw patches as a fallback,
            // as it might be a fragment without headers.
            let patches = parse_patches(content)?;
            if !patches.is_empty() {
                Ok(patches)
            } else {
                // If that yields nothing, return empty.
                Ok(Vec::new())
            }
        }
    }
}

/// Parses a string containing one or more markdown diff blocks into a vector of [`Patch`] objects.
///
/// This function scans the input `content` for markdown-style code blocks. It supports
/// variable-length code fences (e.g., ` ``` ` or ` ```` `) and correctly handles nested
/// code blocks.
///
/// It checks every block to see if it contains valid diff content (Unified Diff or Conflict Markers)
/// at the top level of the block. Diffs inside nested code blocks (e.g., examples within documentation)
/// are ignored. Blocks that do not contain recognizable patch signatures are skipped efficiently.
///
/// It supports two formats within the blocks:
/// 1. **Unified Diff:** Standard `--- a/file`, `+++ b/file`, `@@ ... @@` format.
/// 2. **Conflict Markers:** `<<<<`, `====`, `>>>>` blocks. Since these lack file headers,
///    patches will be assigned a generic file path (`patch_target`).
///
/// For automatic format detection (supporting raw diffs and conflict markers outside of markdown),
/// use [`parse_auto()`].
///
/// # Arguments
///
/// * `content` - A string slice containing the text to parse.
///
/// # Errors
///
/// Returns `Err(ParseError)` if a block looks like a patch (e.g. has `--- a/file`) but fails
/// to parse correctly. Blocks that simply lack headers are ignored.
///
/// # Example
///
/// ````rust
/// use mpatch::parse_diffs;
///
/// let diff_content = r#"
/// ```rust
/// // This block will be checked, and if it contains a diff, it will be parsed.
/// --- a/src/main.rs
/// +++ b/src/main.rs
/// @@ -1,3 +1,3 @@
///  fn main() {
/// -    println!("Hello, world!");
/// +    println!("Hello, mpatch!");
///  }
/// ```
/// "#;
///
/// let patches = parse_diffs(diff_content).unwrap();
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].file_path.to_str(), Some("src/main.rs"));
/// assert_eq!(patches[0].hunks.len(), 1);
/// ````
pub fn parse_diffs(content: &str) -> Result<Vec<Patch>, ParseError> {
    debug!("Starting to parse diffs from content (Markdown mode).");
    let mut all_patches = Vec::new();
    let mut lines = content.lines().enumerate().peekable();

    // The `find` call consumes the iterator until it finds the start of a diff block.
    // The loop continues searching for more blocks from where the last one ended.
    while let Some((line_index, line_text)) = lines.by_ref().find(|(_, line)| {
        let trimmed = line.trim_start();
        trimmed.starts_with("```") && trimmed.chars().take_while(|&c| c == '`').count() >= 3
    }) {
        let trimmed = line_text.trim_start();
        let fence_len = trimmed.chars().take_while(|&c| c == '`').count();

        trace!(
            "Found potential diff block start on line {}: '{}'",
            line_index,
            line_text
        );
        let diff_block_start_line = line_index + 1;

        let mut block_lines = Vec::new();

        // Consume lines until end of block
        while let Some((_, line)) = lines.peek() {
            let inner_trimmed = line.trim_start();
            if inner_trimmed.starts_with("```")
                && inner_trimmed.chars().take_while(|&c| c == '`').count() >= fence_len
            {
                lines.next(); // Consume the closing fence
                break;
            }
            let (_, line) = lines.next().unwrap();
            block_lines.push(line);
        }

        if has_patch_signature_at_level_1(&block_lines) {
            debug!(
                "Parsing diff block starting on line {}.",
                diff_block_start_line
            );
            let block_patches = parse_generic_block_lines(block_lines, diff_block_start_line)?;
            all_patches.extend(block_patches);
        } else {
            trace!(
                "Skipping code block starting on line {} (no patch markers found).",
                diff_block_start_line
            );
        }
    }

    debug!(
        "Finished parsing. Found {} patch(es) in total.",
        all_patches.len()
    );
    Ok(all_patches)
}

/// Checks if the provided lines contain a patch signature at the first level of nesting.
///
/// This ensures that we don't parse diffs that are inside nested code blocks (e.g.,
/// a diff example inside a markdown block).
fn has_patch_signature_at_level_1<S: AsRef<str>>(lines: &[S]) -> bool {
    let mut in_nested_block = false;
    let mut current_fence_len = 0;

    for line in lines {
        let line = line.as_ref();
        let trimmed = line.trim_start();

        // Check for nested block boundaries
        if trimmed.starts_with("```") {
            let fence_len = trimmed.chars().take_while(|&c| c == '`').count();
            if fence_len >= 3 {
                if !in_nested_block {
                    in_nested_block = true;
                    current_fence_len = fence_len;
                    continue;
                } else if fence_len >= current_fence_len {
                    in_nested_block = false;
                    current_fence_len = 0;
                    continue;
                }
            }
        }

        if !in_nested_block
            && (line.starts_with("--- ")
                || line.starts_with("diff --git")
                || trimmed.starts_with("<<<<")
                || trimmed.starts_with("====")
                || trimmed.starts_with(">>>>"))
        {
            return true;
        }
    }
    false
}

/// Helper function to parse a block of lines that could be Unified or Conflict.
/// This consolidates the fallback logic previously inside `parse_diffs`.
fn parse_generic_block_lines(
    lines: Vec<&str>,
    start_line: usize,
) -> Result<Vec<Patch>, ParseError> {
    // 1. Try parsing as standard unified diff
    let standard_result = parse_patches_from_lines(lines.clone().into_iter());

    match standard_result {
        Ok(patches) => {
            if !patches.is_empty() {
                Ok(patches)
            } else {
                // 2. If standard parsing found nothing, try conflict markers
                Ok(parse_conflict_markers_from_lines(lines.into_iter()))
            }
        }
        Err(e) => {
            // 3. If standard parsing failed (e.g. missing header), check for conflict markers
            let conflict_patches = parse_conflict_markers_from_lines(lines.into_iter());
            if !conflict_patches.is_empty() {
                Ok(conflict_patches)
            } else {
                // 4. Return original error if both failed
                match e {
                    ParseError::MissingFileHeader { .. } => {
                        Err(ParseError::MissingFileHeader { line: start_line })
                    }
                }
            }
        }
    }
}

/// Parses a string containing a diff and returns a single [`Patch`] object.
///
/// This is a convenience function that wraps [`parse_auto()`] but enforces that the
/// input `content` results in exactly one `Patch`. It is useful when you expect
/// a diff for a single file and want to handle the "zero or many" cases as an error.
///
/// # Arguments
///
/// * `content` - A string slice containing the text to parse. This can be a raw
///   Unified Diff, a Markdown block, or a set of Conflict Markers.
///
/// # Errors
///
/// Returns a [`SingleParseError`] if:
/// - The underlying parsing fails (e.g., a diff block is missing a file header).
/// - No patches are found in the content.
/// - More than one patch is found in the content.
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_single_patch, SingleParseError};
/// // --- Success Case ---
/// let diff_content = r#"
/// ```diff
/// --- a/src/main.rs
/// +++ b/src/main.rs
/// @@ -1,3 +1,3 @@
///  fn main() {
/// -    println!("Hello, world!");
/// +    println!("Hello, mpatch!");
///  }
/// ```
/// "#;
/// let patch = parse_single_patch(diff_content).unwrap();
/// assert_eq!(patch.file_path.to_str(), Some("src/main.rs"));
///
/// // --- Error Case (Multiple Patches) ---
/// let multi_file_diff = r#"
/// ```diff
/// --- a/file1.txt
/// +++ b/file1.txt
/// @@ -1 +1 @@
/// -a
/// +b
/// --- a/file2.txt
/// +++ b/file2.txt
/// @@ -1 +1 @@
/// -c
/// +d
/// ```
/// "#;
/// let result = parse_single_patch(multi_file_diff);
/// assert!(matches!(result, Err(SingleParseError::MultiplePatchesFound(2))));
/// ````
pub fn parse_single_patch(content: &str) -> Result<Patch, SingleParseError> {
    let mut patches = parse_auto(content)?;

    if patches.len() > 1 {
        Err(SingleParseError::MultiplePatchesFound(patches.len()))
    } else if patches.is_empty() {
        Err(SingleParseError::NoPatchesFound)
    } else {
        // .remove(0) is safe here because we've confirmed the length is 1.
        Ok(patches.remove(0))
    }
}
/// Parses a string containing raw unified diff content into a vector of [`Patch`] objects.
///
/// Unlike [`parse_diffs()`], this function does not look for markdown code blocks.
/// It assumes the entire input string is valid unified diff content. This is useful
/// when you have a raw `.diff` or `.patch` file, or the output of a `git diff` command.
///
/// For automatic format detection, use [`parse_auto()`].
///
/// # Arguments
///
/// * `content` - A string slice containing the raw unified diff to parse.
///
/// # Errors
///
/// Returns `Err(ParseError::MissingFileHeader)` if the content contains patch
/// hunks but no `--- a/path/to/file` header.
///
/// # Example
///
/// ```rust
/// use mpatch::parse_patches;
///
/// let raw_diff = r#"
/// --- a/src/main.rs
/// +++ b/src/main.rs
/// @@ -1,3 +1,3 @@
///  fn main() {
/// -    println!("Hello, world!");
/// +    println!("Hello, mpatch!");
///  }
/// "#;
///
/// let patches = parse_patches(raw_diff).unwrap();
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].file_path.to_str(), Some("src/main.rs"));
/// ```
pub fn parse_patches(content: &str) -> Result<Vec<Patch>, ParseError> {
    debug!("Starting to parse raw diff content.");
    parse_patches_from_lines(content.lines())
}

/// Parses a string containing "Conflict Marker" style diffs (<<<<, ====, >>>>).
///
/// This format is common in Git merge conflicts or AI-generated code suggestions.
/// Since this format typically lacks file headers, the resulting [`Patch`] objects
/// will have a generic file path (`patch_target`).
///
/// This function treats text outside the markers as context lines, text between
/// `<<<<` and `====` as deletions, and text between `====` and `>>>>` as additions.
///
/// For automatic format detection, use [`parse_auto()`].
///
/// # Example
///
/// ```rust
/// use mpatch::parse_conflict_markers;
///
/// let content = r#"
/// fn main() {
/// <<<<
///     println!("Old");
/// ====
///     println!("New");
/// >>>>
/// }
/// "#;
///
/// let patches = parse_conflict_markers(content);
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].hunks[0].removed_lines(), vec!["    println!(\"Old\");"]);
/// assert_eq!(patches[0].hunks[0].added_lines(), vec!["    println!(\"New\");"]);
/// ```
pub fn parse_conflict_markers(content: &str) -> Vec<Patch> {
    debug!("Starting to parse conflict marker content.");
    parse_conflict_markers_from_lines(content.lines())
}

/// Parses an iterator of lines containing raw unified diff content into a vector of [`Patch`] objects.
///
/// This is a lower-level, more flexible alternative to [`parse_patches()`]. It is useful
/// when you already have the diff content as a sequence of lines (e.g., from reading a
/// file line-by-line) and want to avoid allocating the entire content as a single string.
///
/// It assumes the entire sequence of lines is valid unified diff content and does not
/// look for markdown code blocks.
///
/// # Arguments
///
/// * `lines` - An iterator that yields string slices, where each slice is a line of the diff.
///
/// # Errors
///
/// Returns `Err(ParseError::MissingFileHeader)` if the content contains patch
/// hunks but no `--- a/path/to/file` header. The `line` number in the error will
/// correspond to the first hunk header (e.g., `@@ ... @@`) found.
///
/// # Example
///
/// ```rust
/// use mpatch::parse_patches_from_lines;
///
/// let raw_diff_lines = vec![
///     "--- a/src/main.rs",
///     "+++ b/src/main.rs",
///     "@@ -1,3 +1,3 @@",
///     " fn main() {",
///     "-    println!(\"Hello, world!\");",
///     "+    println!(\"Hello, mpatch!\");",
///     " }",
/// ];
///
/// let patches = parse_patches_from_lines(raw_diff_lines.into_iter()).unwrap();
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].file_path.to_str(), Some("src/main.rs"));
/// ```
pub fn parse_patches_from_lines<'a, I>(lines: I) -> Result<Vec<Patch>, ParseError>
where
    I: Iterator<Item = &'a str>,
{
    let mut unmerged_patches: Vec<Patch> = Vec::new();

    // State variables for the parser as it moves through the diff block.
    let mut first_hunk_header_line: Option<usize> = None;
    let mut current_file: Option<PathBuf> = None;
    let mut current_hunks: Vec<Hunk> = Vec::new();
    let mut current_hunk_lines: Vec<String> = Vec::new();
    let mut current_hunk_old_start_line: Option<usize> = None;
    let mut current_hunk_new_start_line: Option<usize> = None;
    let mut ends_with_newline_for_section = true;

    for (line_idx, line) in lines.enumerate() {
        if let Some(stripped_line) = line.strip_prefix("--- ") {
            trace!("  Found file header line: '{}'", line);
            // A `---` line always signals a new file section.
            // Finalize the previous file's patch section if it exists.
            if let Some(existing_file) = &current_file {
                if !current_hunk_lines.is_empty() {
                    trace!(
                        "    Finalizing previous hunk with {} lines.",
                        current_hunk_lines.len()
                    );
                    current_hunks.push(Hunk {
                        lines: std::mem::take(&mut current_hunk_lines),
                        old_start_line: current_hunk_old_start_line,
                        new_start_line: current_hunk_new_start_line,
                    });
                }
                if !current_hunks.is_empty() {
                    debug!(
                        "  Finalizing patch section for '{}' with {} hunk(s).",
                        existing_file.display(),
                        current_hunks.len()
                    );
                    unmerged_patches.push(Patch {
                        file_path: existing_file.clone(),
                        hunks: std::mem::take(&mut current_hunks),
                        ends_with_newline: ends_with_newline_for_section,
                    });
                }
            }

            // Reset for the new file section.
            trace!("  Resetting parser state for new file section.");
            current_file = None;
            current_hunk_lines.clear();
            current_hunk_old_start_line = None;
            current_hunk_new_start_line = None;
            ends_with_newline_for_section = true;

            let path_part = stripped_line.trim();
            if path_part == "/dev/null" || path_part == "a/dev/null" {
                trace!("    Path is /dev/null, indicating file creation.");
                // File creation, path will be in `+++` line.
            } else {
                let path_str = path_part.strip_prefix("a/").unwrap_or(path_part);
                debug!("  Starting new patch section for file: '{}'", path_str);
                current_file = Some(PathBuf::from(path_str.trim()));
            }
        } else if let Some(stripped_line) = line.strip_prefix("+++ ") {
            trace!("  Found '+++' line: '{}'", line);
            if current_file.is_none() {
                let path_part = stripped_line.trim();
                let path_str = path_part.strip_prefix("b/").unwrap_or(path_part);
                debug!("  Set file path from '+++' line: '{}'", path_str);
                current_file = Some(PathBuf::from(path_str.trim()));
            }
        } else if line.starts_with("@@") {
            trace!("  Found hunk header: '{}'", line);
            if !current_hunk_lines.is_empty() {
                trace!(
                    "    Finalizing previous hunk with {} lines.",
                    current_hunk_lines.len()
                );
                current_hunks.push(Hunk {
                    lines: std::mem::take(&mut current_hunk_lines),
                    old_start_line: current_hunk_old_start_line,
                    new_start_line: current_hunk_new_start_line,
                });
            }
            if first_hunk_header_line.is_none() {
                first_hunk_header_line = Some(line_idx + 1);
            }
            let (old, new) = parse_hunk_header(line);
            trace!("    Parsed old_start={:?}, new_start={:?}", old, new);
            current_hunk_old_start_line = old;
            current_hunk_new_start_line = new;
        } else if line.starts_with(['+', '-', ' ']) {
            // Only treat this as a hunk line if we're actually inside a hunk.
            if current_hunk_old_start_line.is_some() {
                trace!("    Adding line to current hunk: '{}'", line.trim_end());
                current_hunk_lines.push(line.to_string());
            }
        } else if line.starts_with('\\') {
            // This line only makes sense inside a hunk.
            if current_hunk_old_start_line.is_some() {
                trace!("  Found '\\ No newline at end of file' marker.");
                ends_with_newline_for_section = false;
            }
        } else if current_hunk_old_start_line.is_some() {
            trace!(
                "    Adding unrecognized line as context to current hunk: '{}'",
                line.trim_end()
            );
            current_hunk_lines.push(format!(" {}", line));
        }
    }

    // Finalize the last hunk and patch section after the loop.
    debug!("  End of diff block. Finalizing last hunk and patch section.");
    if !current_hunk_lines.is_empty() {
        trace!(
            "    Finalizing final hunk with {} lines.",
            current_hunk_lines.len()
        );
        current_hunks.push(Hunk {
            lines: current_hunk_lines,
            old_start_line: current_hunk_old_start_line,
            new_start_line: current_hunk_new_start_line,
        });
    }

    if let Some(file_path) = current_file {
        if !current_hunks.is_empty() {
            debug!(
                "  Finalizing patch section for '{}' with {} hunk(s).",
                file_path.display(),
                current_hunks.len()
            );
            unmerged_patches.push(Patch {
                file_path,
                hunks: current_hunks,
                ends_with_newline: ends_with_newline_for_section,
            });
        }
    } else if !current_hunks.is_empty() {
        let error_line = first_hunk_header_line.unwrap_or(1);
        warn!(
            "Found hunks starting near line {} but no file path header ('--- a/path').",
            error_line
        );
        return Err(ParseError::MissingFileHeader { line: error_line });
    }

    // Merge patch sections for the same file.
    if unmerged_patches.is_empty() {
        return Ok(vec![]);
    }

    debug!(
        "Merging {} patch section(s) found in the block.",
        unmerged_patches.len()
    );
    let mut merged_patches: Vec<Patch> = Vec::new();
    for patch_section in unmerged_patches {
        if let Some(existing_patch) = merged_patches
            .iter_mut()
            .find(|p| p.file_path == patch_section.file_path)
        {
            debug!(
                "  Merging {} hunk(s) for '{}' into existing patch.",
                patch_section.hunks.len(),
                patch_section.file_path.display()
            );
            existing_patch.hunks.extend(patch_section.hunks);
            existing_patch.ends_with_newline = patch_section.ends_with_newline;
        } else {
            debug!(
                "  Adding new patch for '{}'.",
                patch_section.file_path.display()
            );
            merged_patches.push(patch_section);
        }
    }

    Ok(merged_patches)
}

/// Parses an iterator of lines containing "Conflict Marker" style diffs.
///
/// See [`parse_conflict_markers`] for details.
fn parse_conflict_markers_from_lines<'a, I>(lines: I) -> Vec<Patch>
where
    I: Iterator<Item = &'a str>,
{
    let mut hunk_lines = Vec::new();
    let mut has_markers = false;

    enum State {
        Context,
        Old,
        New,
    }
    let mut state = State::Context;

    for line in lines {
        if line.trim_start().starts_with("<<<<") {
            state = State::Old;
            has_markers = true;
            continue;
        } else if line.trim_start().starts_with("====") {
            state = State::New;
            has_markers = true;
            continue;
        } else if line.trim_start().starts_with(">>>>") {
            state = State::Context;
            has_markers = true;
            continue;
        }

        match state {
            State::Context => hunk_lines.push(format!(" {}", line)),
            State::Old => hunk_lines.push(format!("-{}", line)),
            State::New => hunk_lines.push(format!("+{}", line)),
        }
    }

    if !has_markers {
        return Vec::new();
    }

    // Create a single patch with a single hunk representing the entire block.
    // Since we don't have line numbers, we leave them as None.
    let hunk = Hunk {
        lines: hunk_lines,
        old_start_line: None,
        new_start_line: None,
    };

    // Since conflict markers don't specify a file, we use a placeholder.
    // The user can override this or use `patch_content_str` where it doesn't matter.
    vec![Patch {
        file_path: PathBuf::from("patch_target"),
        hunks: vec![hunk],
        ends_with_newline: true, // Assumption
    }]
}

/// Converts a `std::io::Error` into a more specific `PatchError`.
fn map_io_error(path: PathBuf, e: std::io::Error) -> PatchError {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => PatchError::PermissionDenied { path },
        std::io::ErrorKind::IsADirectory => PatchError::TargetIsDirectory { path },
        _ => PatchError::Io { path, source: e },
    }
}

/// Ensures a relative path, when joined to a base directory, resolves to a location
/// that is still inside that base directory.
///
/// This is a critical security function to prevent path traversal attacks (e.g.,
/// a malicious patch trying to modify `../../etc/passwd`). It works by canonicalizing
/// both the base directory and the final target path to their absolute, symlink-resolved
/// forms and then checking if the target path is a child of the base directory.
///
/// # Arguments
///
/// * `base_dir` - The trusted root directory.
/// * `relative_path` - The untrusted relative path to be validated.
///
/// # Returns
///
/// - `Ok(PathBuf)`: The safe, canonicalized, absolute path of the target if validation succeeds.
/// - `Err(PatchError::PathTraversal)`: If the path resolves outside the `base_dir`.
/// - `Err(PatchError::Io)`: If an I/O error occurs during path canonicalization (e.g., `base_dir` does not exist).
///
/// # Example
///
/// ```rust
/// # use mpatch::{ensure_path_is_safe, PatchError};
/// # use std::path::Path;
/// # use std::fs;
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let base_dir = dir.path();
///
/// // A safe path
/// let safe_path = Path::new("src/main.rs");
/// let resolved_path = ensure_path_is_safe(base_dir, safe_path)?;
/// let canonical_base = fs::canonicalize(base_dir)?;
/// assert!(resolved_path.starts_with(&canonical_base));
///
/// // An unsafe path
/// let unsafe_path = Path::new("../secret.txt");
/// let result = ensure_path_is_safe(base_dir, unsafe_path);
/// assert!(matches!(result, Err(PatchError::PathTraversal(_))));
/// # Ok(())
/// # }
/// ```
pub fn ensure_path_is_safe(base_dir: &Path, relative_path: &Path) -> Result<PathBuf, PatchError> {
    trace!(
        "  Checking path safety for base '{}' and relative path '{}'",
        base_dir.display(),
        relative_path.display()
    );
    let base_path =
        fs::canonicalize(base_dir).map_err(|e| map_io_error(base_dir.to_path_buf(), e))?;
    let target_file_path = base_dir.join(relative_path);
    let parent = target_file_path.parent().unwrap_or(Path::new(""));
    fs::create_dir_all(parent).map_err(|e| map_io_error(parent.to_path_buf(), e))?;
    let final_path = fs::canonicalize(parent)
        .map_err(|e| map_io_error(parent.to_path_buf(), e))?
        .join(target_file_path.file_name().unwrap_or_default());
    if !final_path.starts_with(&base_path) {
        return Err(PatchError::PathTraversal(relative_path.to_path_buf()));
    }
    Ok(final_path)
}

/// A convenience function that applies a slice of [`Patch`] objects to a target directory.
///
/// This is a high-level convenience function that iterates through a list of
/// patches and applies each one to the filesystem using [`apply_patch_to_file()`].
/// It aggregates the results, including both successful applications and any
/// "hard" errors encountered (like I/O errors).
///
/// This function will continue applying patches even if some fail.
///
/// # Example
///
/// ````
/// # use mpatch::{parse_auto, apply_patches_to_dir, ApplyOptions};
/// # use std::fs;
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// fs::write(dir.path().join("file1.txt"), "foo\n")?;
/// fs::write(dir.path().join("file2.txt"), "baz\n")?;
///
/// let diff = r#"
/// ```diff
/// --- a/file1.txt
/// +++ b/file1.txt
/// @@ -1 +1 @@
/// -foo
/// +bar
/// --- a/file2.txt
/// +++ b/file2.txt
/// @@ -1 +1 @@
/// -baz
/// +qux
/// ```
/// "#;
/// let patches = parse_auto(diff)?;
/// let options = ApplyOptions::new();
///
/// let batch_result = apply_patches_to_dir(&patches, dir.path(), options);
///
/// assert!(batch_result.all_succeeded());
/// assert_eq!(fs::read_to_string(dir.path().join("file1.txt"))?, "bar\n");
/// assert_eq!(fs::read_to_string(dir.path().join("file2.txt"))?, "qux\n");
/// # Ok(())
/// # }
/// ````
pub fn apply_patches_to_dir(
    patches: &[Patch],
    target_dir: &Path,
    options: ApplyOptions,
) -> BatchResult {
    let results = patches
        .iter()
        .map(|patch| {
            let result = apply_patch_to_file(patch, target_dir, options);
            (patch.file_path.clone(), result)
        })
        .collect();

    BatchResult { results }
}

/// A convenience function that applies a single [`Patch`] to the filesystem.
///
/// This function orchestrates the patching process for a single file. It handles
/// filesystem interactions like reading the original file and writing the new
/// content, while delegating the core patching logic to [`apply_patch_to_content()`].
///
/// # Arguments
///
/// * `patch` - The [`Patch`] object to apply.
/// * `target_dir` - The base directory where the patch should be applied. The
///   `patch.file_path` will be joined to this directory.
/// * `options` - Configuration for the patch operation, such as `dry_run` and
///   `fuzz_factor`.
///
/// # Returns
///
/// - `Ok(PatchResult)` on success. The `PatchResult` contains a detailed report
///   for each hunk and, if `dry_run` was enabled, a diff of the proposed changes.
///   If some hunks failed, the file may be in a partially patched state (unless
///   in dry-run mode).
/// - `Err(PatchError)` for "hard" errors like I/O problems, path traversal violations,
///   or a missing target file.
///
/// # Example
///
/// ````
/// # use mpatch::{parse_single_patch, apply_patch_to_file, ApplyOptions};
/// # use std::fs;
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // 1. Setup a temporary directory and a file to patch.
/// let dir = tempdir()?;
/// let file_path = dir.path().join("hello.txt");
/// fs::write(&file_path, "Hello, world!\n")?;
///
/// // 2. Define and parse the patch.
/// let diff_content = r#"
/// ```diff
/// --- a/hello.txt
/// +++ b/hello.txt
/// @@ -1 +1 @@
/// -Hello, world!
/// +Hello, mpatch!
/// ```
/// "#;
/// let patch = parse_single_patch(diff_content)?;
///
/// // 3. Apply the patch to the directory.
/// let options = ApplyOptions::exact();
/// let result = apply_patch_to_file(&patch, dir.path(), options)?;
///
/// // 4. Verify the results.
/// assert!(result.report.all_applied_cleanly());
/// let new_content = fs::read_to_string(&file_path)?;
/// assert_eq!(new_content, "Hello, mpatch!\n");
/// # Ok(())
/// # }
/// ````
pub fn apply_patch_to_file(
    patch: &Patch,
    target_dir: &Path,
    options: ApplyOptions,
) -> Result<PatchResult, PatchError> {
    info!("Applying patch to: {}", patch.file_path.display());

    // --- Path Safety Check ---
    // This is a critical security measure. `ensure_path_is_safe` returns a
    // canonicalized, absolute path that is confirmed to be inside the target_dir.
    let safe_target_path = ensure_path_is_safe(target_dir, &patch.file_path)?;
    trace!(
        "    Path is safe. Absolute target path: '{}'",
        safe_target_path.display()
    );

    // --- Read Original File ---
    // All subsequent operations use the verified `safe_target_path`.
    if safe_target_path.is_dir() {
        return Err(PatchError::TargetIsDirectory {
            path: safe_target_path,
        });
    }

    let (original_content, is_new_file) = if safe_target_path.is_file() {
        trace!("  Reading target file '{}'", patch.file_path.display());
        let content = fs::read_to_string(&safe_target_path)
            .map_err(|e| map_io_error(safe_target_path.clone(), e))?;
        trace!("    Read {} bytes from target file.", content.len());
        (content, false)
    } else {
        // File doesn't exist. This is only okay if it's a file creation patch.
        if !patch.is_creation() {
            // For user-facing errors, show the original path, not the canonicalized one.
            return Err(PatchError::TargetNotFound(
                target_dir.join(&patch.file_path),
            ));
        }
        info!("  Target file does not exist. Assuming file creation.");
        (String::new(), true)
    };
    trace!(
        "  Read {} lines from target file.",
        original_content.lines().count()
    );

    // --- Apply Patch to Content ---
    trace!("  Calling apply_patch_to_content...");
    let result = apply_patch_to_content(
        patch,
        if is_new_file {
            None
        } else {
            Some(&original_content)
        },
        &options,
    );
    let new_content = result.new_content;
    let apply_result = result.report;

    let mut diff = None;
    if options.dry_run {
        // In dry-run mode, generate a diff instead of writing to the file.
        info!(
            "  DRY RUN: Would write changes to '{}'",
            patch.file_path.display()
        );
        trace!("  Generating diff for dry run...");
        let diff_text = unified_diff(
            similar::Algorithm::default(),
            &original_content,
            &new_content,
            3,
            Some(("a", "b")),
        );
        diff = Some(diff_text.to_string());
    } else {
        // Write the modified content to the file system.
        // The parent directory might have been created by `ensure_path_is_safe`
        // for a new file, but we ensure it again just in case.
        if let Some(parent) = safe_target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| map_io_error(parent.to_path_buf(), e))?;
        }
        trace!(
            "  Writing {} bytes to '{}'",
            new_content.len(),
            safe_target_path.display()
        );
        fs::write(&safe_target_path, new_content)
            .map_err(|e| map_io_error(safe_target_path.clone(), e))?;
        if apply_result.all_applied_cleanly() {
            info!(
                "  Successfully wrote changes to '{}'",
                patch.file_path.display()
            );
        } else {
            warn!("  Wrote partial changes to '{}'", patch.file_path.display());
        }
    }

    Ok(PatchResult {
        report: apply_result,
        diff,
    })
}

/// A strict variant of [`apply_patch_to_file()`] that treats partial applications as an error.
///
/// This function provides a simpler error handling model for workflows where any
/// failed hunk should be considered a failure for the entire operation.
///
/// # Returns
///
/// - `Ok(PatchResult)`: If all hunks were applied successfully.
/// - `Err(StrictApplyError::PartialApply)`: If some hunks failed to apply. The file may
///   be in a partially patched state (unless in dry-run mode). The `report` within
///   the error contains the detailed results.
/// - `Err([`StrictApplyError::Patch`])`: For "hard" errors like I/O problems or a missing target file.
///
/// # Example
///
/// ````rust
/// use mpatch::{parse_single_patch, try_apply_patch_to_file, ApplyOptions, StrictApplyError};
/// use std::fs;
/// use tempfile::tempdir;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // --- Success Case ---
/// let dir = tempdir()?;
/// let file_path = dir.path().join("hello.txt");
/// fs::write(&file_path, "Hello, world!\n")?;
///
/// let success_diff = r#"
/// ```diff
/// --- a/hello.txt
/// +++ b/hello.txt
/// @@ -1 +1 @@
/// -Hello, world!
/// +Hello, mpatch!
/// ```
/// "#;
/// let patch = parse_single_patch(success_diff)?;
///
/// let options = ApplyOptions::new();
/// let result = try_apply_patch_to_file(&patch, dir.path(), options)?;
/// assert!(result.report.all_applied_cleanly());
///
/// // --- Failure Case (Partial Apply) ---
/// let dir_fail = tempdir()?;
/// let file_path_fail = dir_fail.path().join("partial.txt");
/// fs::write(&file_path_fail, "line 1\nline 2\n")?;
///
/// let failing_diff = r#"
/// ```diff
/// --- a/partial.txt
/// +++ b/partial.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -WRONG CONTEXT
/// +line two
/// ```
/// "#;
/// let patch_fail = parse_single_patch(failing_diff)?;
///
/// let result = try_apply_patch_to_file(&patch_fail, dir_fail.path(), options);
/// assert!(matches!(result, Err(StrictApplyError::PartialApply { .. })));
///
/// if let Err(StrictApplyError::PartialApply { report }) = result {
///     assert!(!report.all_applied_cleanly());
///     assert_eq!(report.failures().len(), 1);
/// }
/// # Ok(())
/// # }
/// ````
pub fn try_apply_patch_to_file(
    patch: &Patch,
    target_dir: &Path,
    options: ApplyOptions,
) -> Result<PatchResult, StrictApplyError> {
    // This line was already correct
    let result = apply_patch_to_file(patch, target_dir, options)?;
    if result.report.all_applied_cleanly() {
        Ok(result)
    } else {
        Err(StrictApplyError::PartialApply {
            report: result.report,
        })
    }
}

/// An iterator that applies hunks from a patch one by one.
///
/// This struct provides fine-grained control over the patch application process.
/// It allows you to apply hunks sequentially, inspect the intermediate state of
/// the content, and handle results on a per-hunk basis.
///
/// The iterator yields a [`HunkApplyStatus`] for each hunk in the patch.
///
/// # Example
///
/// ````rust
/// use mpatch::{parse_single_patch, HunkApplier, HunkApplyStatus, ApplyOptions};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // 1. Define original content and a patch.
/// let original_lines = vec!["line 1", "line 2", "line 3"];
/// let diff_content = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -2,1 +2,1 @@
/// -line 2
/// +line two
/// ```
/// "#;
/// let patch = parse_single_patch(diff_content)?;
/// let options = ApplyOptions::new();
///
/// // 2. Create the applier.
/// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
///
/// // 3. Apply the first (and only) hunk.
/// let status = applier.next().unwrap();
/// assert!(matches!(status, HunkApplyStatus::Applied { .. }));
///
/// // 4. Check that there are no more hunks.
/// assert!(applier.next().is_none());
///
/// // 5. Finalize the content.
/// let new_content = applier.into_content();
/// assert_eq!(new_content, "line 1\nline two\nline 3\n");
/// # Ok(())
/// # }
/// ````
#[derive(Debug)]
pub struct HunkApplier<'a> {
    hunks: std::slice::Iter<'a, Hunk>,
    current_lines: Vec<String>,
    options: &'a ApplyOptions,
    patch_ends_with_newline: bool,
}

impl<'a> HunkApplier<'a> {
    /// Creates a new `HunkApplier` to begin a step-by-step patch operation.
    ///
    /// This constructor initializes the applier with the patch to be applied and the
    /// original content. The content is provided as an optional slice of lines,
    /// allowing for both file modifications (`Some(lines)`) and file creations (`None`).
    /// The applier can then be used as an iterator to apply hunks one by one.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
    /// let patch = parse_single_patch(diff)?;
    /// let options = ApplyOptions::new();
    ///
    /// // Create the applier for a step-by-step operation.
    /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
    ///
    /// // Now `applier` is ready to be used as an iterator.
    /// let status = applier.next().unwrap();
    /// # Ok(())
    /// # }
    /// ```
    pub fn new<T: AsRef<str>>(
        patch: &'a Patch,
        original_lines: Option<&'a [T]>,
        options: &'a ApplyOptions,
    ) -> Self {
        let current_lines: Vec<String> = original_lines
            .map(|lines| lines.iter().map(|s| s.as_ref().to_string()).collect())
            .unwrap_or_default();
        Self {
            hunks: patch.hunks.iter(),
            current_lines,
            options,
            patch_ends_with_newline: patch.ends_with_newline,
        }
    }

    /// Returns a slice of the current lines, reflecting all hunks applied so far.
    ///
    /// This method provides read-only access to the intermediate state of the
    /// content being patched. It is useful for inspecting the content between
    /// applying hunks with the `HunkApplier` iterator.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
    /// let patch = parse_single_patch(diff)?;
    /// let options = ApplyOptions::new();
    ///
    /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
    ///
    /// // Before applying, it's the original content.
    /// assert_eq!(applier.current_lines(), &["line 1", "line 2"]);
    ///
    /// // Apply the hunk.
    /// applier.next();
    ///
    /// // After applying, the lines are updated.
    /// assert_eq!(applier.current_lines(), &["line 1", "line two"]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn current_lines(&self) -> &[String] {
        &self.current_lines
    }

    /// Consumes the applier and returns the final vector of lines.
    ///
    /// After iterating through the `HunkApplier` and applying all desired hunks,
    /// this method can be called to take ownership of the final, modified vector
    /// of strings.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
    /// let patch = parse_single_patch(diff)?;
    /// let options = ApplyOptions::new();
    ///
    /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
    /// applier.next(); // Apply all hunks
    ///
    /// let final_lines = applier.into_lines();
    /// assert_eq!(final_lines, vec!["line 1".to_string(), "line two".to_string()]);
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_lines(self) -> Vec<String> {
        self.current_lines
    }

    /// Consumes the applier and returns the final content as a single string.
    ///
    /// This method joins the final lines with newlines and ensures the content
    /// has a trailing newline if required by the patch's `ends_with_newline`
    /// property. It is the most common way to get the final result from a
    /// `HunkApplier`.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,1 +1,1\n-line 1\n+line one\n```";
    /// let patch = parse_single_patch(diff)?;
    /// let options = ApplyOptions::new();
    ///
    /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
    /// applier.next(); // Apply all hunks
    ///
    /// let final_content = applier.into_content();
    /// assert_eq!(final_content, "line one\n");
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_content(self) -> String {
        let mut new_content = self.current_lines.join("\n");
        if self.patch_ends_with_newline && !new_content.is_empty() {
            new_content.push('\n');
        }
        new_content
    }
}

impl<'a> Iterator for HunkApplier<'a> {
    type Item = HunkApplyStatus;

    /// Applies the next hunk in the patch and returns its status.
    ///
    /// This method advances the iterator, applying one hunk to the internal state
    /// of the `HunkApplier`. It returns `Some(HunkApplyStatus)` for each hunk in
    /// the patch, and `None` when all hunks have been processed.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions, HunkApplyStatus};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
    /// let patch = parse_single_patch(diff)?;
    /// let options = ApplyOptions::new();
    ///
    /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
    ///
    /// // Call next() to apply the first hunk.
    /// let status = applier.next();
    /// assert!(matches!(status, Some(HunkApplyStatus::Applied { .. })));
    ///
    /// // Call next() again; there are no more hunks.
    /// assert!(applier.next().is_none());
    /// # Ok(())
    /// # }
    /// ```
    fn next(&mut self) -> Option<Self::Item> {
        let hunk = self.hunks.next()?;
        Some(apply_hunk_to_lines(
            hunk,
            &mut self.current_lines,
            self.options,
        ))
    }
}

/// Applies the logic of a patch to a slice of lines.
///
/// This is a high-level convenience function that drives a [`HunkApplier`] iterator
/// to completion and returns the final result. For more granular control, create
/// and use a `HunkApplier` directly.
///
/// # Arguments
///
/// * `patch` - The [`Patch`] object to apply.
/// * `original_lines` - An `Option` containing a slice of strings representing the file's content.
///   `Some(lines)` for an existing file, `None` for a new file (creation).
///   The slice can contain `String` or `&str`.
/// * `options` - Configuration for the patch operation, such as `fuzz_factor`.
///
/// # Returns
///
/// An [`InMemoryResult`] containing the new content and a detailed report.
///
/// # Example
///
/// ```rust
/// # use mpatch::{parse_single_patch, apply_patch_to_lines, ApplyOptions};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // 1. Define original content and the patch.
/// let original_lines = vec!["Hello, world!"];
/// // Construct the diff string programmatically to avoid rustdoc parsing issues with ```.
/// let diff_str = [
///     "```diff",
///     "--- a/hello.txt",
///     "+++ b/hello.txt",
///     "@@ -1 +1 @@",
///     "-Hello, world!",
///     "+Hello, mpatch!",
///     "```",
/// ].join("\n");
///
/// // 2. Parse the diff to get a Patch object.
/// let patch = parse_single_patch(&diff_str)?;
///
/// // 3. Apply the patch to the lines in memory.
/// let options = ApplyOptions::exact();
/// let result = apply_patch_to_lines(&patch, Some(&original_lines), &options);
///
/// // 4. Check the results.
/// assert_eq!(result.new_content, "Hello, mpatch!\n");
/// assert!(result.report.all_applied_cleanly());
/// # Ok(())
/// # }
/// ```
pub fn apply_patch_to_lines<T: AsRef<str>>(
    patch: &Patch,
    original_lines: Option<&[T]>,
    options: &ApplyOptions,
) -> InMemoryResult {
    trace!(
        "  apply_patch_to_lines called with {} lines of original content.",
        original_lines.map_or(0, |l| l.len())
    );

    let mut applier = HunkApplier::new(patch, original_lines, options);
    let total_hunks = patch.hunks.len();

    // Drive the iterator to completion, logging progress along the way.
    let hunk_results: Vec<_> = applier
        .by_ref()
        .enumerate()
        .map(|(i, status)| {
            let hunk_index = i + 1;
            info!("  Applying Hunk {}/{}...", hunk_index, total_hunks);
            match &status {
                HunkApplyStatus::Applied {
                    location,
                    match_type,
                    replaced_lines,
                } => {
                    debug!(
                        "    Successfully applied Hunk {} at {} via {:?}",
                        hunk_index, location, match_type
                    );
                    trace!("    Replaced lines:");
                    for line in replaced_lines {
                        trace!("      - {}", line);
                    }
                }
                HunkApplyStatus::SkippedNoChanges => {
                    debug!("    Skipped Hunk {} (no changes).", hunk_index);
                }
                HunkApplyStatus::Failed(error) => {
                    warn!("  Failed to apply Hunk {}. {}", hunk_index, error);
                }
            }
            status
        })
        .collect();

    // Finalize the result from the consumed applier.
    let new_content = applier.into_content();

    let report = ApplyResult { hunk_results };
    InMemoryResult {
        new_content,
        report,
    }
}

/// A strict variant of [`apply_patch_to_lines()`] that treats partial applications as an error.
///
/// This function provides a simpler error handling model for workflows where any
/// failed hunk should be considered a failure for the entire operation.
///
/// # Returns
///
/// - `Ok(InMemoryResult)`: If all hunks were applied successfully.
/// - `Err(StrictApplyError::PartialApply)`: If some hunks failed to apply. The returned
///   `report` within the error contains the detailed results.
///
/// # Example
///
/// ````rust
/// use mpatch::{parse_single_patch, try_apply_patch_to_lines, ApplyOptions, StrictApplyError};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_lines = vec!["line 1", "line 2"];
///
/// // --- Success Case ---
/// let success_diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -line 2
/// +line two
/// ```
/// "#;
/// let patch = parse_single_patch(success_diff)?;
/// let options = ApplyOptions::new();
/// let result = try_apply_patch_to_lines(&patch, Some(&original_lines), &options)?;
/// assert!(result.report.all_applied_cleanly());
/// assert_eq!(result.new_content, "line 1\nline two\n");
///
/// // --- Failure Case ---
/// let failing_diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -WRONG CONTEXT
/// +line two
/// ```
/// "#;
/// let failing_patch = parse_single_patch(failing_diff)?;
/// let result = try_apply_patch_to_lines(&failing_patch, Some(&original_lines), &options);
///
/// assert!(matches!(result, Err(StrictApplyError::PartialApply { .. })));
/// if let Err(StrictApplyError::PartialApply { report }) = result {
///     assert!(!report.all_applied_cleanly());
/// }
/// # Ok(())
/// # }
/// ````
pub fn try_apply_patch_to_lines<T: AsRef<str>>(
    patch: &Patch,
    original_lines: Option<&[T]>,
    options: &ApplyOptions,
) -> Result<InMemoryResult, StrictApplyError> {
    // This line was already correct
    let result = apply_patch_to_lines(patch, original_lines, options);
    if result.report.all_applied_cleanly() {
        Ok(result)
    } else {
        Err(StrictApplyError::PartialApply {
            report: result.report,
        })
    }
}

/// Applies the logic of a patch to a string content.
///
/// This is a pure function that takes the patch definition and the original content
/// of a file as a string, and returns the transformed content. It does not
/// interact with the filesystem. This is useful for testing, in-memory operations,
/// or integrating `mpatch`'s logic into other tools.
///
/// # Arguments
///
/// **Note:** For improved performance when content is already available as a slice
/// of lines, consider using [`apply_patch_to_lines()`].
///
/// * `patch` - The [`Patch`] object to apply.
/// * `original_content` - An `Option<&str>` representing the file's content.
///   `Some(content)` for an existing file, `None` for a new file (creation).
/// * `options` - Configuration for the patch operation, such as `fuzz_factor`.
///
/// # Returns
///
/// An [`InMemoryResult`] containing the new content and a detailed report.
///
/// # Example
///
/// ```rust
/// # use mpatch::{parse_single_patch, apply_patch_to_content, ApplyOptions};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // 1. Define original content and the patch.
/// let original_content = "Hello, world!\n";
/// // Construct the diff string programmatically to avoid rustdoc parsing issues with ```.
/// let diff_str = [
///     "```diff",
///     "--- a/hello.txt",
///     "+++ b/hello.txt",
///     "@@ -1 +1 @@",
///     "-Hello, world!",
///     "+Hello, mpatch!",
///     "```",
/// ].join("\n");
///
/// // 2. Parse the diff to get a Patch object.
/// let patch = parse_single_patch(&diff_str)?;
///
/// // 3. Apply the patch to the content in memory.
/// let options = ApplyOptions::exact();
/// let result = apply_patch_to_content(&patch, Some(original_content), &options);
///
/// // 4. Check the results.
/// assert_eq!(result.new_content, "Hello, mpatch!\n");
/// assert!(result.report.all_applied_cleanly());
/// # Ok(())
/// # }
/// ```
pub fn apply_patch_to_content(
    patch: &Patch,
    original_content: Option<&str>,
    options: &ApplyOptions,
) -> InMemoryResult {
    let original_lines: Option<Vec<String>> =
        original_content.map(|c| c.lines().map(String::from).collect());
    apply_patch_to_lines(patch, original_lines.as_deref(), options)
}

/// A strict variant of [`apply_patch_to_content()`] that treats partial applications as an error.
///
/// This function provides a simpler error handling model for workflows where any
/// failed hunk should be considered a failure for the entire operation.
///
/// # Returns
///
/// - `Ok(InMemoryResult)`: If all hunks were applied successfully.
/// - `Err(StrictApplyError::PartialApply)`: If some hunks failed to apply. The returned
///   `report` within the error contains the detailed results.
///
/// # Example
///
/// ````rust
/// use mpatch::{parse_single_patch, try_apply_patch_to_content, ApplyOptions, StrictApplyError};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_content = "line 1\nline 2\n";
///
/// // --- Success Case ---
/// let success_diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -line 2
/// +line two
/// ```
/// "#;
/// let patch = parse_single_patch(success_diff)?;
/// let options = ApplyOptions::new();
/// let result = try_apply_patch_to_content(&patch, Some(original_content), &options)?;
/// assert!(result.report.all_applied_cleanly());
/// assert_eq!(result.new_content, "line 1\nline two\n");
///
/// // --- Failure Case ---
/// let failing_diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,2 +1,2 @@
///  line 1
/// -WRONG CONTEXT
/// +line two
/// ```
/// "#;
/// let failing_patch = parse_single_patch(failing_diff)?;
/// let result = try_apply_patch_to_content(&failing_patch, Some(original_content), &options);
///
/// assert!(matches!(result, Err(StrictApplyError::PartialApply { .. })));
/// if let Err(StrictApplyError::PartialApply { report }) = result {
///     assert!(!report.all_applied_cleanly());
/// }
/// # Ok(())
/// # }
/// ````
pub fn try_apply_patch_to_content(
    patch: &Patch,
    original_content: Option<&str>,
    options: &ApplyOptions,
) -> Result<InMemoryResult, StrictApplyError> {
    // This line was already correct
    let result = apply_patch_to_content(patch, original_content, options);
    if result.report.all_applied_cleanly() {
        Ok(result)
    } else {
        Err(StrictApplyError::PartialApply {
            report: result.report,
        })
    }
}

/// A high-level, one-shot function to parse a diff and apply it to a string.
///
/// This function is the most convenient entry point for the common workflow of
/// taking a diff (e.g., from a markdown file) and applying it to some existing
/// content in memory. It combines parsing and strict application into a single call.
///
/// It performs the following steps:
/// 1.  Parses the `diff_content` using [`parse_auto()`] (supporting Markdown,
///     Unified Diffs, and Conflict Markers).
/// 2.  Ensures that exactly one `Patch` is found. If zero or more than one are
///     found, it returns an error.
/// 3.  Applies the single patch to `original_content` using the strict logic of
///     [`try_apply_patch_to_content()`].
///
/// # Arguments
///
/// * `diff_content` - A string slice containing the diff. This can be a Markdown
///   code block, a raw Unified Diff, or Conflict Markers.
/// * `original_content` - An `Option<&str>` representing the content to be patched.
///   Use `Some(content)` for an existing file, or `None` for a file creation patch.
/// * `options` - Configuration for the patch operation, such as `fuzz_factor`.
///
/// # Returns
///
/// - `Ok(String)`: The new, patched content if the patch applied cleanly.
/// - `Err(OneShotError)`: If any step fails, including parsing errors, finding
///   the wrong number of patches, or if the patch does not apply cleanly (i.e.,
///   any hunk fails).
///
/// # Example
///
/// ````rust
/// # use mpatch::{patch_content_str, ApplyOptions, OneShotError};
/// # fn main() -> Result<(), OneShotError> {
/// // 1. Define the original content and the diff.
/// let original_content = "fn main() {\n    println!(\"Hello, world!\");\n}\n";
/// let diff_content = r#"
/// ```diff
/// --- a/src/main.rs
/// +++ b/src/main.rs
/// @@ -1,3 +1,3 @@
///  fn main() {
/// -    println!("Hello, world!");
/// +    println!("Hello, mpatch!");
///  }
/// ```
/// "#;
///
/// // 2. Call the one-shot function.
/// let options = ApplyOptions::new();
/// let new_content = patch_content_str(diff_content, Some(original_content), &options)?;
///
/// // 3. Verify the new content.
/// let expected_content = "fn main() {\n    println!(\"Hello, mpatch!\");\n}\n";
/// assert_eq!(new_content, expected_content);
///
/// Ok(())
/// # }
/// ````
pub fn patch_content_str(
    diff_content: &str,
    original_content: Option<&str>,
    options: &ApplyOptions,
) -> Result<String, OneShotError> {
    let mut patches = parse_auto(diff_content)?;
    if patches.is_empty() {
        return Err(OneShotError::NoPatchesFound);
    }
    if patches.len() > 1 {
        return Err(OneShotError::MultiplePatchesFound(patches.len()));
    }
    let patch = patches.remove(0);
    let result = try_apply_patch_to_content(&patch, original_content, options)?;
    Ok(result.new_content)
}

/// Applies a single hunk to a mutable vector of lines in-place.
///
/// This function provides granular control over the patching process, allowing library
/// users to apply changes hunk-by-hunk. It modifies the `target_lines` vector
/// directly based on the changes defined in the `hunk`.
///
/// # Arguments
///
/// * `hunk` - The [`Hunk`] to apply.
/// * `target_lines` - A mutable vector of strings representing the file's content.
///   This vector will be modified by the function.
/// * `options` - Configuration for the patch operation, such as `fuzz_factor`.
///
/// # Returns
///
/// A [`HunkApplyStatus`] indicating the outcome:
/// - [`HunkApplyStatus::Applied`]: The hunk was successfully applied. The `location` and `match_type` are provided.
/// - [`HunkApplyStatus::SkippedNoChanges`]: The hunk contained only context lines and was skipped.
/// - [`HunkApplyStatus::Failed`]: The hunk could not be applied. The reason is provided in the associated [`HunkApplyError`].
///
/// # Example
///
/// ```rust
/// # use mpatch::{parse_single_patch, apply_hunk_to_lines, ApplyOptions, HunkApplyStatus, HunkLocation, MatchType};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // 1. Define original content and the patch.
/// let mut original_lines = vec!["Hello, world!".to_string()];
/// let diff_str = [
///     "```diff",
///     "--- a/hello.txt",
///     "+++ b/hello.txt",
///     "@@ -1 +1 @@",
///     "-Hello, world!",
///     "+Hello, mpatch!",
///     "```",
/// ].join("\n");
///
/// // 2. Parse the diff to get a Hunk object.
/// let patch = parse_single_patch(&diff_str)?;
/// let hunk = &patch.hunks[0];
///
/// // 3. Apply the hunk to the lines in memory.
/// let options = ApplyOptions::exact();
/// let status = apply_hunk_to_lines(hunk, &mut original_lines, &options);
///
/// // 4. Check the results.
/// assert!(matches!(status, HunkApplyStatus::Applied { replaced_lines, .. } if replaced_lines == vec!["Hello, world!"]));
/// assert_eq!(original_lines, vec!["Hello, mpatch!"]);
/// # Ok(())
/// # }
/// ```
pub fn apply_hunk_to_lines(
    hunk: &Hunk,
    target_lines: &mut Vec<String>,
    options: &ApplyOptions,
) -> HunkApplyStatus {
    trace!("Applying hunk with {} lines.", hunk.lines.len());
    trace!("  Match block: {:?}", hunk.get_match_block());
    trace!("  Replace block: {:?}", hunk.get_replace_block());
    if !hunk.has_changes() {
        trace!("  Hunk has no changes, skipping.");
        return HunkApplyStatus::SkippedNoChanges;
    }

    match find_hunk_location_in_lines(hunk, target_lines, options) {
        Ok((location, match_type)) => {
            trace!(
                "  Found location {:?} with match type {:?}. Applying changes.",
                location,
                match_type
            );
            let final_replace_block: Vec<String> =
                // If the match was fuzzy, we need to be more careful about applying it
                // to preserve the file's actual context, which may differ slightly.
                if let MatchType::Fuzzy { .. } = match_type {
                    debug!("    Applying hunk via fuzzy logic, preserving file context.");
                    trace!(
                        "      Fuzzy match location: start={}, len={}",
                        location.start_index,
                        location.length
                    );
                    let file_matched_lines: Vec<_> = target_lines
                        [location.start_index..location.start_index + location.length]
                        .to_vec();
                    trace!("      File content in matched range: {:?}", file_matched_lines);

                    // Create a pure delta patch (no context) from the hunk's changes. This isolates
                    // the exact lines to be removed and added.
                    let removed_text = hunk.removed_lines().join("\n");
                    let added_text = hunk.added_lines().join("\n");

                    // Use a context of 0 to ensure we only match the delta.
                    match Patch::from_texts("delta", &removed_text, &added_text, 0) {
                        Ok(delta_patch) => {
                            if delta_patch.hunks.is_empty() {
                                // No effective changes in the delta, so return the original file block.
                                trace!("      Delta patch has no changes. Preserving original file block.");
                                file_matched_lines
                            } else {
                                let mut temp_lines = file_matched_lines.clone();
                                let mut all_succeeded = true;
                                // The inner search must be exact, as we are looking for the specific lines to remove
                                // within the fuzzy-matched block.
                                let inner_options =
                                    ApplyOptions::exact();

                                for delta_hunk in &delta_patch.hunks {
                                    if !delta_hunk.has_changes() {
                                        continue;
                                    }
                                    // Recursively call this function to apply the delta hunk.
                                    // This is safe because the inner call will use an exact match.
                                    let status =
                                        apply_hunk_to_lines(delta_hunk, &mut temp_lines, &inner_options);
                                    if let HunkApplyStatus::Failed(e) = status {
                                        trace!("      Inner delta hunk application failed: {}", e);
                                        all_succeeded = false;
                                        break;
                                    }
                                }

                                if all_succeeded {
                                    trace!("      Inner delta patch application succeeded. Using transformed lines: {:?}", temp_lines);
                                    temp_lines
                                } else {
                                    warn!("    Internal delta patch application failed. Overwriting block with patch content as a last resort.");
                                    hunk.get_replace_block()
                                        .iter()
                                        .map(|s| s.to_string())
                                        .collect()
                                }
                            }
                        }
                        Err(e) => {
                            warn!("    Internal error: Failed to create delta patch for fuzzy apply: {}. Falling back to simple replace.", e);
                            hunk.get_replace_block()
                                .iter()
                                .map(|s| s.to_string())
                                .collect()
                        }
                    }
                } else {
                    // For exact matches, the original behavior is correct and faster.
                    trace!("    Applying hunk via exact logic.");
                    hunk.get_replace_block().iter().map(|s| s.to_string()).collect()
                };

            let replaced_lines: Vec<String> = target_lines
                .splice(
                    location.start_index..location.start_index + location.length,
                    final_replace_block,
                )
                .collect();
            trace!(
                "  Successfully spliced changes into target lines. Replaced {} lines.",
                replaced_lines.len()
            );
            HunkApplyStatus::Applied {
                location,
                match_type,
                replaced_lines,
            }
        }
        Err(error) => {
            // The calling function will log the failure with context (e.g., hunk index).
            HunkApplyStatus::Failed(error)
        }
    }
}

/// A trait for strategies that find the location to apply a hunk.
///
/// This allows the core matching algorithm to be pluggable, enabling different
/// search strategies to be used if needed.
/// The library provides a robust [`DefaultHunkFinder`] that should be sufficient
/// for most use cases.
///
/// # Example
///
/// This example shows a hypothetical, simplified implementation of a `HunkFinder`
/// that only performs an exact match search.
///
/// ```
/// use mpatch::{Hunk, HunkFinder, HunkLocation, MatchType, HunkApplyError};
///
/// struct ExactOnlyFinder;
///
/// impl HunkFinder for ExactOnlyFinder {
///     fn find_location<T: AsRef<str> + Sync>(
///         &self,
///         hunk: &Hunk,
///         target_lines: &[T],
///     ) -> Result<(HunkLocation, MatchType), HunkApplyError> {
///         let match_block = hunk.get_match_block();
///         if match_block.is_empty() {
///             return Err(HunkApplyError::ContextNotFound);
///         }
///
///         target_lines
///             .windows(match_block.len())
///             .enumerate()
///             .find(|(_, window)| {
///                 window.iter().map(|s| s.as_ref()).eq(match_block.iter().copied())
///             })
///             .map(|(i, _)| (
///                 HunkLocation { start_index: i, length: match_block.len() },
///                 MatchType::Exact
///             ))
///             .ok_or(HunkApplyError::ContextNotFound)
///     }
/// }
/// ```
pub trait HunkFinder {
    /// Finds the location to apply a hunk to a slice of lines.
    ///
    /// This is the core method for any `HunkFinder` implementation. It encapsulates
    /// the search logic used to determine where a hunk's changes should be applied
    /// within the target content. Implementations should return the location and the
    /// type of match found, or an error if no suitable location can be determined.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, DefaultHunkFinder, HunkFinder, ApplyOptions, HunkLocation, MatchType};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // 1. Create a hunk to search for.
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,2 +1,2\n line 1\n-line 2\n+line two\n```";
    /// let hunk = parse_single_patch(diff)?.hunks.remove(0);
    ///
    /// // 2. Define the content to search within.
    /// let target_lines = vec!["line 1", "line 2"];
    ///
    /// // 3. Instantiate a finder and call the method.
    /// let options = ApplyOptions::new();
    /// let finder = DefaultHunkFinder::new(&options);
    /// let (location, match_type) = finder.find_location(&hunk, &target_lines)?;
    ///
    /// // 4. Check the result.
    /// assert_eq!(location, HunkLocation { start_index: 0, length: 2 });
    /// assert!(matches!(match_type, MatchType::Exact));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Arguments
    ///
    /// * `hunk` - The hunk to locate.
    /// * `target_lines` - The content to search within.
    ///
    /// # Returns
    ///
    /// - `Ok((HunkLocation, MatchType))` on success.
    /// - `Err(HunkApplyError)` if no suitable location could be found.
    fn find_location<T: AsRef<str> + Sync>(
        &self,
        hunk: &Hunk,
        target_lines: &[T],
    ) -> Result<(HunkLocation, MatchType), HunkApplyError>;
}

/// The default, built-in strategy for finding hunk locations.
///
/// This implementation uses a hierarchical approach:
/// 1.  Exact match.
/// 2.  Exact match ignoring trailing whitespace.
/// 3.  Flexible fuzzy match using a similarity algorithm.
///
/// It uses line number hints from the patch to resolve ambiguities.
/// While you can use this struct directly, it's typically used internally by
/// functions like [`find_hunk_location_in_lines()`].
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_single_patch, DefaultHunkFinder, HunkFinder, ApplyOptions, HunkLocation, MatchType};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_lines = vec!["line 1", "line two", "line 3"];
/// let diff = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,3 +1,3 @@
///  line 1
/// -line two
/// +line 2
///  line 3
/// ```
/// "#;
/// let hunk = parse_single_patch(diff)?.hunks.remove(0);
///
/// let options = ApplyOptions::exact();
/// let finder = DefaultHunkFinder::new(&options);
///
/// // Use the finder to locate the hunk.
/// let (location, match_type) = finder.find_location(&hunk, &original_lines)?;
///
/// assert_eq!(location, HunkLocation { start_index: 0, length: 3 });
/// assert!(matches!(match_type, MatchType::Exact));
/// # Ok(())
/// # }
/// ````
#[derive(Debug)]
pub struct DefaultHunkFinder<'a> {
    options: &'a ApplyOptions,
}

impl<'a> DefaultHunkFinder<'a> {
    /// Creates a new finder with the given options.
    ///
    /// This is the standard way to instantiate the `DefaultHunkFinder`. The provided
    /// [`ApplyOptions`] will control the behavior of the finder, particularly the
    /// `fuzz_factor` which determines the threshold for fuzzy matching.
    ///
    /// # Example
    ///
    /// ```
    /// # use mpatch::{DefaultHunkFinder, ApplyOptions};
    /// // Create a finder that requires a high similarity for fuzzy matches.
    /// let options = ApplyOptions::new().with_fuzz_factor(0.9);
    /// let finder = DefaultHunkFinder::new(&options);
    /// ```
    pub fn new(options: &'a ApplyOptions) -> Self {
        Self { options }
    }

    /// Finds optimized search ranges within the target file to perform the fuzzy search.
    ///
    /// This is a performance heuristic. It tries to find an "anchor" line from the
    /// hunk that is relatively uncommon in the target file. If successful, it returns
    /// small search windows around the occurrences of that anchor. If no good anchor
    /// is found, it returns a single range covering the entire file.
    fn find_search_ranges<T: AsRef<str>>(
        match_block: &[&str],
        target_lines: &[T],
        hunk_size: usize,
    ) -> Vec<(usize, usize)> {
        const MAX_ANCHOR_OCCURRENCES: usize = 5;
        const MIN_ANCHOR_LEN: usize = 5;
        // Search radius is this factor times the hunk size, with a minimum.
        const SEARCH_RADIUS_FACTOR: usize = 2;
        const MIN_SEARCH_RADIUS: usize = 15;

        if hunk_size == 0 {
            return vec![(0, target_lines.len())];
        }

        // Iterate from the middle of the hunk outwards to find a good anchor line.
        // The middle is often more stable than the edges.
        let mid = hunk_size / 2;
        for i in 0..=mid {
            let indices_to_check = [Some(mid + i), if i > 0 { Some(mid - i) } else { None }];

            for &line_idx_opt in &indices_to_check {
                if let Some(line_idx) = line_idx_opt {
                    if line_idx >= hunk_size {
                        continue;
                    }

                    let anchor_line = match_block[line_idx].trim_end();
                    // Ignore short or empty lines as they are poor anchors.
                    if anchor_line.trim().len() < MIN_ANCHOR_LEN {
                        continue;
                    }

                    // Find all occurrences of the anchor line.
                    let occurrences: Vec<_> = target_lines
                        .iter()
                        .enumerate()
                        .filter(|(_, l)| l.as_ref().trim_end() == anchor_line)
                        .map(|(i, _)| i)
                        .collect();

                    // If the line is unique enough, use it to create search ranges.
                    if !occurrences.is_empty() && occurrences.len() <= MAX_ANCHOR_OCCURRENCES {
                        trace!(
                            "      Found good anchor line (hunk line {}) with {} occurrences: '{}'",
                            line_idx + 1,
                            occurrences.len(),
                            anchor_line.trim()
                        );
                        let mut ranges = Vec::new();
                        let search_radius =
                            (hunk_size * SEARCH_RADIUS_FACTOR).max(MIN_SEARCH_RADIUS);

                        for &occurrence_idx in &occurrences {
                            // Estimate where the hunk would start based on the anchor's position.
                            let estimated_start = occurrence_idx.saturating_sub(line_idx);
                            let start = estimated_start.saturating_sub(search_radius);
                            let end = (estimated_start + hunk_size + search_radius)
                                .min(target_lines.len());
                            ranges.push((start, end));
                        }
                        // Merge any overlapping ranges created by nearby occurrences.
                        return Self::merge_ranges(ranges);
                    }
                }
            }
        }

        // If no good anchor was found, we must search the entire file.
        debug!("      No suitable anchor line found. Falling back to full file scan.");
        vec![(0, target_lines.len())]
    }

    /// Merges a list of overlapping or adjacent ranges into a minimal set of disjoint ranges.
    fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
        if ranges.is_empty() {
            return vec![];
        }
        ranges.sort_unstable_by_key(|k| k.0);
        let mut merged = Vec::with_capacity(ranges.len());
        let mut current_range = ranges[0];

        for &(start, end) in &ranges[1..] {
            if start <= current_range.1 {
                // Overlap or adjacent, merge them.
                current_range.1 = current_range.1.max(end);
            } else {
                // No overlap, push the current range and start a new one.
                merged.push(current_range);
                current_range = (start, end);
            }
        }
        merged.push(current_range);
        merged
    }

    /// Finds the starting index of the hunk's match block in the target lines.
    /// This function implements the core hierarchical search strategy.
    fn find_hunk_location_internal<T: AsRef<str> + Sync>(
        &self,
        match_block: &[&str],
        target_lines: &[T],
        old_start_line: Option<usize>,
    ) -> Result<(HunkLocation, MatchType), HunkApplyError> {
        trace!(
            "  find_hunk_location_internal called for a hunk with {} lines to match against {} target lines.",
            match_block.len(),
            target_lines.len()
        );

        if match_block.is_empty() {
            // An empty match block (file creation) can only be applied to an empty file.
            trace!("    Match block is empty (file creation).");
            return if target_lines.is_empty() {
                trace!("    Target is empty, match successful at (0, 0).");
                Ok((
                    HunkLocation {
                        start_index: 0,
                        length: 0,
                    },
                    MatchType::Exact,
                ))
            } else {
                trace!("    Target is not empty, match failed.");
                Err(HunkApplyError::ContextNotFound)
            };
        }

        // --- STRATEGY 1: Exact Match ---
        // The fastest and most reliable method.
        trace!("    Attempting exact match for hunk...");
        {
            let exact_matches: Box<dyn Iterator<Item = usize>> =
                if match_block.len() <= target_lines.len() {
                    Box::new(
                        target_lines
                            .windows(match_block.len())
                            .enumerate()
                            .filter(|(_, window)| {
                                window
                                    .iter()
                                    .map(|s| s.as_ref())
                                    .eq(match_block.iter().copied())
                            })
                            .map(|(i, _)| i),
                    )
                } else {
                    Box::new(std::iter::empty())
                };

            match Self::tie_break_with_line_number(exact_matches, old_start_line, "exact") {
                Ok(Some(index)) => {
                    debug!("    Found unique exact match at index {}.", index);
                    return Ok((
                        HunkLocation {
                            start_index: index,
                            length: match_block.len(),
                        },
                        MatchType::Exact,
                    ));
                }
                Ok(None) => {} // No exact matches, continue to next strategy.
                Err(matches) => return Err(HunkApplyError::AmbiguousExactMatch(matches)),
            }
        }

        // Optimization: Pre-calculate trimmed lines for subsequent strategies.
        // This avoids repeated allocation and trimming in loops.
        let target_trimmed: Vec<String> = target_lines
            .iter()
            .map(|s| s.as_ref().trim_end().to_string())
            .collect();
        // Create references to the trimmed strings to avoid allocations in TextDiff
        let target_refs: Vec<&str> = target_trimmed.iter().map(|s| s.as_str()).collect();

        // --- STRATEGY 2: Exact Match (Ignoring Trailing Whitespace) ---
        // Handles minor formatting differences.
        trace!("    Attempting exact match (ignoring trailing whitespace)...");
        {
            let match_stripped: Vec<_> = match_block.iter().map(|s| s.trim_end()).collect();
            let stripped_matches: Box<dyn Iterator<Item = usize>> =
                if match_block.len() <= target_lines.len() {
                    Box::new(
                        target_trimmed
                            .windows(match_block.len())
                            .enumerate()
                            .filter(move |(_, window)| {
                                window
                                    .iter()
                                    .map(|s| s.as_str())
                                    .eq(match_stripped.iter().copied())
                            })
                            .map(|(i, _)| i),
                    )
                } else {
                    Box::new(std::iter::empty())
                };

            match Self::tie_break_with_line_number(
                stripped_matches,
                old_start_line,
                "exact (ignoring whitespace)",
            ) {
                Ok(Some(index)) => {
                    debug!(
                        "    Found unique whitespace-insensitive match at index {}.",
                        index
                    );
                    return Ok((
                        HunkLocation {
                            start_index: index,
                            length: match_block.len(),
                        },
                        MatchType::ExactIgnoringWhitespace,
                    ));
                }
                Ok(None) => {} // No matches, continue.
                Err(matches) => return Err(HunkApplyError::AmbiguousExactMatch(matches)),
            }
        }

        // --- STRATEGY 3: Fuzzy Match (with flexible window) ---
        // This is the core "smart" logic. If an exact match fails, we search for
        // the best-fitting slice in the target file, allowing the slice to be
        // slightly larger or smaller than the patch's context. This handles cases
        // where lines have been added or removed near the patch location.
        if self.options.fuzz_factor > 0.0 && !match_block.is_empty() {
            trace!(
                "    Exact matches failed. Attempting flexible fuzzy match (threshold={:.2})...",
                self.options.fuzz_factor
            );
            trace!(
                "      Hunk match block ({} lines): {:?}",
                match_block.len(),
                match_block
            );

            // Hoist invariants for performance
            let match_stripped_lines: Vec<&str> =
                match_block.iter().map(|s| s.trim_end()).collect();
            let match_content = match_stripped_lines.join("\n");

            let mut best_score = -1.0;
            let mut best_ratio_at_best_score = -1.0;
            let mut potential_matches = Vec::new(); // Vec<(start_index, length)>

            let len = match_block.len();
            // Define how far to search for different-sized windows.
            // Proportional to hunk size, but with reasonable bounds.
            let fuzz_distance = (len / 4).clamp(3, 8);
            let min_len = len.saturating_sub(fuzz_distance).max(1);
            let max_len = len.saturating_add(fuzz_distance);
            trace!(
                "      Searching with window sizes from {} to {} (hunk size: {}, fuzz distance: {})",
                min_len,
                max_len,
                len,
                fuzz_distance
            );

            // Performance heuristic: narrow down the search space using anchor lines.
            let search_ranges = Self::find_search_ranges(match_block, &target_trimmed, len);
            trace!("    Using search ranges: {:?}", search_ranges);

            // When the anchor heuristic fails, the search can be slow. We parallelize the
            // scoring of all possible windows using Rayon if the `parallel` feature is enabled.
            #[cfg(feature = "parallel")]
            let all_scored_windows: Vec<(f64, f64, f64, f64, usize, usize)> = search_ranges
                .par_iter()
                .flat_map(|&(range_start, range_end)| {
                    // By creating local references, we ensure that the inner `move` closures
                    // capture these references (which are `Copy`) instead of attempting to move
                    // the original non-`Copy` `Vec` and `String` from the outer scope.
                    let match_stripped_lines = &match_stripped_lines;
                    let match_content = &match_content;
                    let target_slice = &target_refs[range_start..range_end];

                    (min_len..=max_len)
                        .into_par_iter()
                        .filter(move |&window_len| window_len <= target_slice.len())
                        .flat_map(move |window_len| {
                            (0..=target_slice.len() - window_len)
                                .into_par_iter()
                                .map(move |i| {
                                    let window_stripped_lines = &target_slice[i..i + window_len];
                                    let absolute_index = range_start + i;

                                    // HYBRID SCORING: (Copied from original sequential loop)
                                    let diff_lines = similar::TextDiff::from_slices(
                                        window_stripped_lines,
                                        match_stripped_lines,
                                    );
                                    let ratio_lines = diff_lines.ratio();

                                    let mut capacity = 0;
                                    for line in window_stripped_lines {
                                        capacity += line.len() + 1;
                                    }
                                    let mut window_content = String::with_capacity(capacity);
                                    for (j, line) in window_stripped_lines.iter().enumerate() {
                                        if j > 0 {
                                            window_content.push('\n');
                                        }
                                        window_content.push_str(line);
                                    }

                                    let diff_words = similar::TextDiff::from_words(
                                        &window_content,
                                        match_content,
                                    );
                                    let ratio_words = diff_words.ratio();
                                    // HYBRID SCORING: Give more weight to word-based ratio, as it's
                                    // better at detecting small changes within a line. Line-based
                                    // ratio is still important for overall structure, especially
                                    // when lines are inserted or deleted.
                                    let ratio = 0.3 * ratio_lines as f64 + 0.7 * ratio_words as f64;
                                    // The ratio from the `similar` crate already implicitly includes a
                                    // penalty for size differences. We use the raw ratio as the score.
                                    let score = ratio;

                                    (
                                        score,
                                        ratio,
                                        ratio_lines as f64,
                                        ratio_words as f64,
                                        absolute_index,
                                        window_len,
                                    )
                                })
                        })
                })
                .collect();

            #[cfg(not(feature = "parallel"))]
            let all_scored_windows: Vec<(f64, f64, f64, f64, usize, usize)> = search_ranges
                .iter()
                .flat_map(|&(range_start, range_end)| {
                    // By creating local references, we ensure that the inner `move` closures
                    // capture these references (which are `Copy`) instead of attempting to move
                    // the original non-`Copy` `Vec` and `String` from the outer scope.
                    let match_stripped_lines = &match_stripped_lines;
                    let match_content = &match_content;
                    let target_slice = &target_refs[range_start..range_end];

                    (min_len..=max_len)
                        .filter(move |&window_len| window_len <= target_slice.len())
                        .flat_map(move |window_len| {
                            (0..=target_slice.len() - window_len).map(move |i| {
                                let window_stripped_lines = &target_slice[i..i + window_len];
                                let absolute_index = range_start + i;

                                // HYBRID SCORING: (Copied from original sequential loop)
                                let diff_lines = similar::TextDiff::from_slices(
                                    window_stripped_lines,
                                    match_stripped_lines,
                                );
                                let ratio_lines = diff_lines.ratio();

                                let mut capacity = 0;
                                for line in window_stripped_lines {
                                    capacity += line.len() + 1;
                                }
                                let mut window_content = String::with_capacity(capacity);
                                for (j, line) in window_stripped_lines.iter().enumerate() {
                                    if j > 0 {
                                        window_content.push('\n');
                                    }
                                    window_content.push_str(line);
                                }

                                let diff_words =
                                    similar::TextDiff::from_words(&window_content, match_content);
                                let ratio_words = diff_words.ratio();
                                // HYBRID SCORING: Give more weight to word-based ratio, as it's
                                // better at detecting small changes within a line. Line-based
                                // ratio is still important for overall structure, especially
                                // when lines are inserted or deleted.
                                let ratio = 0.3 * ratio_lines as f64 + 0.7 * ratio_words as f64;
                                // The ratio from the `similar` crate already implicitly includes a
                                // penalty for size differences. We use the raw ratio as the score.
                                let score = ratio;

                                (
                                    score,
                                    ratio,
                                    ratio_lines as f64,
                                    ratio_words as f64,
                                    absolute_index,
                                    window_len,
                                )
                            })
                        })
                })
                .collect();

            if log::log_enabled!(log::Level::Trace) {
                let mut sorted_windows = all_scored_windows.clone();
                sorted_windows
                    .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                trace!("      Top fuzzy match candidates:");
                for (score, ratio, _, _, idx, len) in sorted_windows.iter().take(5) {
                    let window_content: Vec<_> = target_refs[*idx..*idx + *len].to_vec();
                    trace!(
                        "        - Index {}, Len {}: Score {:.3} (Ratio {:.3}) | Content: {:?}",
                        idx,
                        len,
                        score,
                        ratio,
                        window_content
                    );
                }
            }

            // Process the collected results sequentially to find the best match and handle tie-breaking.
            for (score, ratio, ratio_lines, ratio_words, absolute_index, window_len) in
                all_scored_windows
            {
                // This is the same logic as in the original sequential loop.
                if score > best_score {
                    trace!(
                        "        New best score: {:.3} (ratio {:.3} [l:{:.3},w:{:.3}]) at index {} (window len {})",
                        score,
                        ratio,
                        ratio_lines,
                        ratio_words,
                        absolute_index, window_len
                    );
                    best_score = score;
                    best_ratio_at_best_score = ratio;
                    potential_matches.clear();
                    potential_matches.push((absolute_index, window_len));
                } else if f64::abs(score - best_score) < 1e-9 {
                    // Tie in score. Prefer the one with the higher raw similarity ratio,
                    // as it indicates a better content match before size penalties.
                    if potential_matches.is_empty() {
                        potential_matches.push((absolute_index, window_len));
                        continue;
                    }

                    if ratio > best_ratio_at_best_score {
                        // This is a better match despite the same score (e.g., less penalty, more similarity)
                        trace!(
                            "        Tie in score ({:.3}), but new ratio {:.3} is better than old {:.3}. New best.",
                            score,
                            ratio,
                            best_ratio_at_best_score
                        );
                        best_ratio_at_best_score = ratio;
                        potential_matches.clear();
                        potential_matches.push((absolute_index, window_len));
                    } else if f64::abs(ratio - best_ratio_at_best_score) < 1e-9 {
                        // Also a tie in ratio, so it's a true ambiguity
                        trace!(
                            "        Tie in score ({:.3}) and ratio ({:.3}). Adding candidate: index {}, len {}",
                            score,
                            ratio,
                            absolute_index,
                            window_len
                        );
                        potential_matches.push((absolute_index, window_len));
                    }
                }
            }

            trace!(
                "    Fuzzy search complete. Best score: {:.3}, best ratio: {:.3}, potential matches: {:?}",
                best_score,
                best_ratio_at_best_score,
                potential_matches
            );

            // Check if the best match found meets the user-defined threshold.
            if best_ratio_at_best_score >= f64::from(self.options.fuzz_factor) {
                if potential_matches.len() == 1 {
                    let (start, len) = potential_matches[0];
                    debug!(
                        "    Found best fuzzy match at index {} (length {}, similarity: {:.3} >= threshold: {:.3}).",
                        start, len, best_ratio_at_best_score, self.options.fuzz_factor
                    );
                    return Ok((
                        HunkLocation {
                            start_index: start,
                            length: len,
                        },
                        MatchType::Fuzzy {
                            score: best_ratio_at_best_score,
                        },
                    ));
                }
                // AMBIGUOUS FUZZY MATCH - TRY TO TIE-BREAK
                if let Some(line) = old_start_line {
                    trace!(
                            "    Ambiguous fuzzy match found at {:?}. Attempting to tie-break using line number hint: {}",
                            potential_matches,
                            line
                        );
                    let mut closest_match: Option<(usize, usize)> = None;
                    let mut min_distance = usize::MAX;
                    let mut is_tie = false;

                    for &(match_index, match_len) in &potential_matches {
                        // Hunk line numbers are 1-based, indices are 0-based.
                        trace!(
                            "      Candidate {:?}: distance from line hint is {}",
                            (match_index, match_len),
                            (match_index + 1).abs_diff(line)
                        );
                        let distance = (match_index + 1).abs_diff(line);
                        if distance < min_distance {
                            min_distance = distance;
                            closest_match = Some((match_index, match_len));
                            is_tie = false;
                        } else if distance == min_distance {
                            is_tie = true;
                        }
                    }

                    if !is_tie {
                        if let Some((start, len)) = closest_match {
                            debug!(
                                    "    Tie-broke ambiguous fuzzy match using line number. Best match is at index {} (length {}, similarity: {:.3} >= threshold: {:.3}).",
                                    start, len, best_ratio_at_best_score, self.options.fuzz_factor
                                );
                            return Ok((
                                HunkLocation {
                                    start_index: start,
                                    length: len,
                                },
                                MatchType::Fuzzy {
                                    score: best_ratio_at_best_score,
                                },
                            ));
                        }
                    } else {
                        trace!("    Tie-breaking failed: multiple fuzzy matches are equidistant from the line number hint.");
                    }
                }
                warn!("    Ambiguous fuzzy match: Multiple locations found with same top score ({:.3}): {:?}. Skipping.", best_ratio_at_best_score, potential_matches);
                return Err(HunkApplyError::AmbiguousFuzzyMatch(potential_matches));
            } else if best_ratio_at_best_score >= 0.0 {
                // Did not meet threshold
                let (start, len) = potential_matches.first().copied().unwrap_or((0, 0));
                debug!(
                    "    Fuzzy match failed: Best location (index {}, len {}) had similarity {:.3}, which is below the threshold of {:.3}.",
                    start, len, best_ratio_at_best_score, self.options.fuzz_factor
                );
                return Err(HunkApplyError::FuzzyMatchBelowThreshold {
                    best_score: best_ratio_at_best_score,
                    threshold: self.options.fuzz_factor,
                    location: HunkLocation {
                        start_index: start,
                        length: len,
                    },
                });
            } else {
                // No potential matches found at all
                debug!("    Fuzzy match: Could not find any potential match location.");
                // Fall through to the final ContextNotFound error
            }
        } else if self.options.fuzz_factor <= 0.0 {
            trace!("    Failed exact matches. Fuzzy matching disabled.");
        }

        // --- STRATEGY 4: End-of-file fuzzy match for short files ---
        // This handles cases where the entire file is a good fuzzy match for the
        // start of the hunk context, which can happen if the file is missing
        // context lines that the patch expects to be there at the end.
        if !target_lines.is_empty()
            && target_lines.len() < match_block.len()
            && self.options.fuzz_factor > 0.0
        {
            trace!("    Target file is shorter than hunk. Attempting end-of-file fuzzy match...");
            let match_stripped: Vec<&str> = match_block.iter().map(|s| s.trim_end()).collect();
            let diff = TextDiff::from_slices(&target_refs, &match_stripped);
            let ratio = diff.ratio();

            // Be slightly more lenient for this specific end-of-file prefix case.
            let effective_threshold = (f64::from(self.options.fuzz_factor) - 0.1).max(0.5);
            trace!(
                "      Using effective threshold for EOF match: {:.3}",
                effective_threshold
            );

            if ratio as f64 >= effective_threshold {
                debug!(
                    "    End-of-file fuzzy match succeeded with ratio {:.3} (threshold {:.3}). Treating as full-file match.",
                    ratio, effective_threshold
                );
                // We are matching the entire file from the beginning.
                return Ok((
                    HunkLocation {
                        start_index: 0,
                        length: target_lines.len(),
                    },
                    MatchType::Fuzzy {
                        score: ratio as f64,
                    },
                ));
            } else {
                trace!(
                    "    End-of-file fuzzy match ratio {:.3} did not meet effective threshold {:.3}.",
                    ratio,
                    effective_threshold
                );
            }
        }

        debug!("    Failed to find any suitable match location for hunk.");
        Err(HunkApplyError::ContextNotFound)
    }

    /// Given an iterator of match indices, attempts to find the best one using the
    /// hunk's original line number as a hint. Returns the index of the best match,
    /// or `None` if the ambiguity cannot be resolved.
    /// This function avoids collecting matches into a vector if there are 0 or 1 matches.
    fn tie_break_with_line_number(
        mut matches: impl Iterator<Item = usize>,
        start_line: Option<usize>,
        match_type: &str,
    ) -> Result<Option<usize>, Vec<usize>> {
        // --- Step 1: Check for 0 or 1 matches without allocation ---
        let first_match = match matches.next() {
            Some(m) => m,
            None => {
                trace!("      No {} matches found.", match_type);
                return Ok(None);
            }
        };

        if let Some(second_match) = matches.next() {
            // --- Step 2: Multiple matches found, collect and tie-break ---
            // At least two matches exist. Collect them all for analysis.
            let mut all_matches = vec![first_match, second_match];
            all_matches.extend(matches);

            trace!(
                "      Found {} {} match candidate(s) at indices: {:?}",
                all_matches.len(),
                match_type,
                all_matches
            );

            // More than 1 match, try to tie-break using the line number hint.
            if let Some(line) = start_line {
                trace!(
                "    Ambiguous {} match found at {:?}. Attempting to tie-break using line number hint: {}",
                match_type,
                all_matches,
                line
            );
                let mut closest_index = 0;
                let mut min_distance = usize::MAX;
                let mut is_tie = false;

                // Find the match that is numerically closest to the hint.
                for &match_index in &all_matches {
                    // Hunk line numbers are 1-based, indices are 0-based.
                    trace!(
                        "      Candidate index {}: distance from line hint {} is {}",
                        match_index,
                        line,
                        (match_index + 1).abs_diff(line)
                    );
                    let distance = (match_index + 1).abs_diff(line);
                    if distance < min_distance {
                        min_distance = distance;
                        closest_index = match_index;
                        is_tie = false;
                    } else if distance == min_distance {
                        // If another match has the same minimum distance, it's a tie.
                        is_tie = true;
                    }
                }

                if !is_tie {
                    trace!(
                        "      Successfully tie-broke using line number. Best match is at index {}.",
                        closest_index
                    );
                    return Ok(Some(closest_index));
                }
                trace!(
                    "    Tie-breaking failed: multiple matches are equidistant from the line number hint."
                );
            } else {
                trace!(
                    "    tie_break: Ambiguous '{}' match, but no line number hint provided.",
                    match_type
                );
            }

            // If we reach here, the ambiguity could not be resolved.
            Err(all_matches)
        } else {
            // Exactly one match was found.
            trace!(
                "      Found 1 {} match candidate at index: {}",
                match_type,
                first_match
            );
            trace!(
                "    tie_break: Only one match found for '{}' match at index {}. No tie-break needed.",
                match_type,
                first_match
            );
            Ok(Some(first_match))
        }
    }
}

impl<'a> HunkFinder for DefaultHunkFinder<'a> {
    fn find_location<T: AsRef<str> + Sync>(
        &self,
        hunk: &Hunk,
        target_lines: &[T],
    ) -> Result<(HunkLocation, MatchType), HunkApplyError> {
        let match_block = hunk.get_match_block();
        self.find_hunk_location_internal(&match_block, target_lines, hunk.old_start_line)
    }
}

/// Finds the location to apply a hunk to a given text content without modifying it.
///
/// This function encapsulates the core context-aware search logic of `mpatch`. It
/// performs a series of checks, from exact matching to fuzzy matching, to determine
/// the optimal position to apply the hunk. It is a read-only operation.
///
/// This is useful for tools that want to analyze where a patch would apply without
/// actually performing the patch, or for building custom patch application logic.
///
/// # Arguments
///
/// **Note:** For improved performance when content is already available as a slice
/// of lines, consider using [`find_hunk_location_in_lines()`].
///
/// * `hunk` - A reference to the [`Hunk`] to be located.
/// * `target_content` - A string slice of the content to search within.
/// * `options` - Configuration for the patch operation, such as `fuzz_factor`.
///
/// # Returns
///
/// - `Ok((HunkLocation, MatchType))` on success, containing the location and the
///   type of match that was found.
/// - `Err(HunkApplyError)` if no suitable location could be found, with a reason
///   for the failure (e.g., context not found, ambiguous match).
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_single_patch, find_hunk_location, HunkLocation, ApplyOptions, MatchType};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_content = "line 1\nline two\nline 3\n";
/// let diff_content = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,3 +1,3 @@
///  line 1
/// -line two
/// +line 2
///  line 3
/// ```
/// "#;
///
/// let patch = parse_single_patch(diff_content)?;
/// let hunk = &patch.hunks[0];
///
/// let options = ApplyOptions::exact();
/// let (location, match_type) = find_hunk_location(hunk, original_content, &options)?;
///
/// assert_eq!(location, HunkLocation { start_index: 0, length: 3 });
/// assert!(matches!(match_type, MatchType::Exact));
/// # Ok(())
/// # }
/// ````
pub fn find_hunk_location(
    hunk: &Hunk,
    target_content: &str,
    options: &ApplyOptions,
) -> Result<(HunkLocation, MatchType), HunkApplyError> {
    let target_lines: Vec<_> = target_content.lines().collect();
    find_hunk_location_in_lines(hunk, &target_lines, options)
}

/// Finds the location to apply a hunk to a slice of lines without modifying it.
///
/// This is a more allocation-friendly version of [`find_hunk_location()`] that
/// operates directly on a slice of strings, avoiding the need to join and re-split
/// content. This is useful for tools that already have content in a line-based
/// format.
///
/// # Arguments
///
/// * `hunk` - A reference to the [`Hunk`] to be located.
/// * `target_lines` - A slice of strings representing the content to search within.
///   The slice can contain `String` or `&str`.
/// * `options` - Configuration for the patch operation, such as `fuzz_factor`.
///
/// # Returns
///
/// - `Ok((HunkLocation, MatchType))` on success, containing the location and the
///   type of match that was found.
/// - `Err(HunkApplyError)` if no suitable location could be found.
///
/// # Example
///
/// ````rust
/// # use mpatch::{parse_single_patch, find_hunk_location_in_lines, HunkLocation, ApplyOptions, MatchType};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let original_lines = vec!["line 1", "line two", "line 3"];
/// let diff_content = r#"
/// ```diff
/// --- a/file.txt
/// +++ b/file.txt
/// @@ -1,3 +1,3 @@
///  line 1
/// -line two
/// +line 2
///  line 3
/// ```
/// "#;
///
/// let patch = parse_single_patch(diff_content)?;
/// let hunk = &patch.hunks[0];
///
/// let options = ApplyOptions::exact();
/// let (location, match_type) = find_hunk_location_in_lines(hunk, &original_lines, &options)?;
///
/// assert_eq!(location, HunkLocation { start_index: 0, length: 3 });
/// assert!(matches!(match_type, MatchType::Exact));
/// # Ok(())
/// # }
/// ````
pub fn find_hunk_location_in_lines<T: AsRef<str> + Sync>(
    hunk: &Hunk,
    target_lines: &[T],
    options: &ApplyOptions,
) -> Result<(HunkLocation, MatchType), HunkApplyError> {
    let finder = DefaultHunkFinder::new(options);
    finder.find_location(hunk, target_lines)
}

/// Parses a hunk header line (e.g., "@@ -1,3 +1,3 @@") to extract the starting line number.
fn parse_hunk_header(line: &str) -> (Option<usize>, Option<usize>) {
    // We are interested in the original file's line number, which is the first number after '-'.
    // Example: @@ -21,8 +21,8 @@
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return (None, None);
    }
    let old_line = parts[1]
        .strip_prefix('-')
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse::<usize>().ok());
    let new_line = parts[2]
        .strip_prefix('+')
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse::<usize>().ok());
    (old_line, new_line)
}
