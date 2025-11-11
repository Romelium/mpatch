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
//! The most common use case is to parse a diff from a string (e.g., a markdown
//! file) and apply it to a file on disk. This example shows the end-to-end
//! process in a temporary directory.
//!
//! ````rust
//! use mpatch::{parse_diffs, apply_patch_to_file, ApplyOptions};
//! use std::fs;
//! use tempfile::{tempdir, TempDir};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Set up a temporary directory and a file to be patched.
//! let dir = tempdir()?;
//! let file_path = dir.path().join("src/main.rs");
//! fs::create_dir_all(file_path.parent().unwrap())?;
//! fs::write(&file_path, "fn main() {\n    println!(\"Hello, world!\");\n}\n")?;
//!
//! // 2. Define the patch content, as if it came from a markdown file.
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
//! // 3. Parse the diff content to get a list of patches.
//! let patches = parse_diffs(diff_content)?;
//! assert_eq!(patches.len(), 1);
//! let patch = &patches[0];
//!
//! // 4. Apply the patch.
//! let options = ApplyOptions::default();
//! let result = apply_patch_to_file(patch, dir.path(), options)?;
//!
//! // The patch should apply cleanly.
//! assert!(result.report.all_applied_cleanly());
//! assert!(result.diff.is_none());
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
//! Using the `mpatch` library typically involves a two-step process:
//!
//! 1.  **Parsing:** Use [`parse_diffs`] to read a string and extract a `Vec<Patch>`.
//!     This function is markdown-aware, searching for code blocks annotated with `diff`
//!     or `patch` (e.g., ` ```diff`, ` ```rust, patch`) and parsing their contents.
//!     This step is purely in-memory.
//! 2.  **Applying:** Use one of the `apply` functions to apply the changes.
//!     - [`apply_patch_to_file`]: The most convenient function for CLI tools. It
//!       handles reading the original file and writing the new content back to disk.
//!     - [`apply_patch_to_content`]: A pure function for in-memory operations. It
//!       takes the original content as a string and returns the new content.
//!
//! ### Core Data Structures
//!
//! - [`Patch`]: Represents all the changes for a single file. It contains the
//!   target file path and a list of hunks.
//! - [`Hunk`]: Represents a single block of changes within a patch, corresponding
//!   to a `@@ ... @@` section in a unified diff.
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
//!   `mpatch` uses the line numbers from the `@@ ... @@` header as a *hint* to
//!   find the most likely location.
//! - **Fuzzy Matching:** If no exact match is found, it uses a similarity algorithm
//!   to find the *best* fuzzy match, making it resilient to minor changes in the
//!   surrounding code.
//!
//! ## Advanced Usage
//!
//! ### In-Memory Operations and Error Handling
//!
//! This example demonstrates how to use `apply_patch_to_content` for in-memory
//! operations and how to programmatically handle cases where a patch only
//! partially applies.
//!
//! ````rust
//! use mpatch::{parse_diffs, apply_patch_to_content, HunkApplyError};
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
//! let patches = parse_diffs(diff_content)?;
//! let patch = &patches[0];
//!
//! // 3. Apply the patch to the content in memory.
//! let options = mpatch::ApplyOptions { dry_run: false, fuzz_factor: 0.0 };
//! let result = apply_patch_to_content(patch, Some(original_content), &options);
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
//! ### Step-by-Step Application with `HunkApplier`
//!
//! For maximum control, you can use the [`HunkApplier`] iterator to apply hunks
//! one at a time and inspect the state between each step.
//!
//! ````rust
//! use mpatch::{parse_diffs, HunkApplier, HunkApplyStatus, ApplyOptions};
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
//! let patch = &parse_diffs(diff_content)?[0];
//! let options = ApplyOptions::default();
//!
//! // 2. Create the applier.
//! let mut applier = HunkApplier::new(patch, Some(&original_lines), &options);
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
//!   mpatch = { version = "0.3.1", default-features = false }
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
#[derive(Error, Debug, PartialEq)]
pub enum ParseError {
    /// A ` ```diff` block was found, but it was missing the `--- a/path/to/file`
    /// header required to identify the target file.
    #[error(
        "Diff block starting on line {line} was found without a file path header (e.g., '--- a/path/to/file')"
    )]
    MissingFileHeader {
        /// The line number where the diff block started.
        line: usize,
    },
}

/// Represents the possible errors that can occur during patch operations.
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

/// The reason a hunk failed to apply.
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
#[derive(Debug, Clone, Copy)]
pub struct ApplyOptions {
    /// If `true`, no files will be modified. Instead, a diff of the proposed
    /// changes will be generated and returned in [`PatchResult`].
    pub dry_run: bool,
    /// The similarity threshold for fuzzy matching (0.0 to 1.0).
    /// Higher is stricter. `0.0` disables fuzzy matching.
    pub fuzz_factor: f32,
}

impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            fuzz_factor: 0.7,
        }
    }
}

impl ApplyOptions {
    /// Creates a new builder for `ApplyOptions`.
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

/// A builder for creating `ApplyOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ApplyOptionsBuilder {
    dry_run: Option<bool>,
    fuzz_factor: Option<f32>,
}

impl ApplyOptionsBuilder {
    /// If `true`, no files will be modified. Instead, a diff of the proposed
    /// changes will be generated and returned in [`PatchResult`].
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = Some(dry_run);
        self
    }

    /// Sets the similarity threshold for fuzzy matching (0.0 to 1.0).
    /// Higher is stricter. `0.0` disables fuzzy matching.
    pub fn fuzz_factor(mut self, fuzz_factor: f32) -> Self {
        self.fuzz_factor = Some(fuzz_factor);
        self
    }

    /// Builds the `ApplyOptions`.
    pub fn build(self) -> ApplyOptions {
        let default = ApplyOptions::default();
        ApplyOptions {
            dry_run: self.dry_run.unwrap_or(default.dry_run),
            fuzz_factor: self.fuzz_factor.unwrap_or(default.fuzz_factor),
        }
    }
}

/// The result of an `apply_patch` operation.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchResult {
    /// Detailed results for each hunk within the patch operation.
    pub report: ApplyResult,
    /// The unified diff of the proposed changes. This is only populated
    /// when `dry_run` was set to `true` in [`ApplyOptions`].
    pub diff: Option<String>,
}

/// The result of an in-memory patch operation.
#[derive(Debug, Clone, PartialEq)]
pub struct InMemoryResult {
    /// The new content after applying the patch.
    pub new_content: String,
    /// Detailed results for each hunk within the patch operation.
    pub report: ApplyResult,
}

/// Contains detailed results for each hunk within a patch operation.
#[derive(Debug, Clone, PartialEq)]
pub struct ApplyResult {
    /// A list of statuses, one for each hunk in the original patch.
    pub hunk_results: Vec<HunkApplyStatus>,
}

/// Details about a hunk that failed to apply.
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
    pub fn all_succeeded(&self) -> bool {
        self.results.iter().all(|(_, res)| res.is_ok())
    }

    /// Returns a list of all operations that resulted in a "hard" error (e.g., I/O).
    pub fn hard_failures(&self) -> Vec<(&PathBuf, &PatchError)> {
        self.results
            .iter()
            .filter_map(|(path, res)| res.as_ref().err().map(|e| (path, e)))
            .collect()
    }
}
// --- Data Structures ---

/// Represents a single hunk of changes within a patch.
///
/// A hunk corresponds to a block of lines starting with `@@ ... @@` in a
/// unified diff. It contains the context, added, and removed lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// The raw lines of the hunk, each prefixed with ' ', '+', or '-'.
    pub lines: Vec<String>,
    /// The original starting line number from the `@@ -l,s ...` header.
    /// This is used as a hint to resolve ambiguity if multiple exact matches are found.
    pub old_start_line: Option<usize>,
    /// The new starting line number from the `@@ ..., +l,s @@` header.
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

/// Represents the location where a hunk should be applied.
///
/// This is returned by [`find_hunk_location`] and provides the necessary
/// information to manually apply a patch to a slice of lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HunkLocation {
    /// The 0-based starting line index in the target content where the hunk should be applied.
    pub start_index: usize,
    /// The number of lines in the target content that will be replaced. This may
    /// differ from the number of lines in the hunk's "match block" when a fuzzy
    /// match is found.
    pub length: usize,
}

impl std::fmt::Display for HunkLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Adding 1 to start_index for a more user-friendly 1-based line number.
        write!(f, "line {}", self.start_index + 1)
    }
}

/// Represents all the changes to be applied to a single file.
///
/// A `Patch` is derived from a `--- a/path/to/file` section within a ` ```diff`
/// block and contains one or more [`Hunk`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// The relative path of the file to be patched, from the target directory.
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

        // Wrap in markdown block for our parser
        let full_diff = format!("```diff\n{}\n```", diff_text);

        let patches = parse_diffs(&full_diff)?;

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
    /// # use mpatch::parse_diffs;
    /// let creation_diff = r#"
    /// ```diff
    /// --- a/new_file.txt
    /// +++ b/new_file.txt
    /// @@ -0,0 +1,2 @@
    /// +Hello
    /// +World
    /// ```
    /// "#;
    /// let patches = parse_diffs(creation_diff).unwrap();
    /// assert!(patches[0].is_creation());
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
    /// # use mpatch::parse_diffs;
    /// let deletion_diff = r#"
    /// ```diff
    /// --- a/old_file.txt
    /// +++ b/old_file.txt
    /// @@ -1,2 +0,0 @@
    /// -Hello
    /// -World
    /// ```
    /// "#;
    /// let patches = parse_diffs(deletion_diff).unwrap();
    /// assert!(patches[0].is_deletion());
    /// ````
    pub fn is_deletion(&self) -> bool {
        !self.hunks.is_empty() && self.hunks.iter().all(|h| h.get_replace_block().is_empty())
    }
}

// --- Core Logic ---

/// Parses a string containing one or more markdown diff blocks into a vector of [`Patch`] objects.
///
/// This function scans the input `content` for markdown-style code blocks annotated
/// with `diff` or `patch` (e.g., ` ````diff ... ``` `, ` ````rust, patch ... ``` `).
/// It can handle multiple blocks in one string, and multiple file patches within a
/// single block.
///
/// # Arguments
///
/// * `content` - A string slice containing the text to parse.
///
/// # Errors
///
/// Returns `Err(ParseError::MissingFileHeader)` if a diff block contains patch
/// hunks but no `--- a/path/to/file` header.
///
/// # Example
///
/// ````rust
/// use mpatch::parse_diffs;
///
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
/// let patches = parse_diffs(diff_content).unwrap();
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].file_path.to_str(), Some("src/main.rs"));
/// assert_eq!(patches[0].hunks.len(), 1);
/// ````
pub fn parse_diffs(content: &str) -> Result<Vec<Patch>, ParseError> {
    let mut all_patches = Vec::new();
    let mut lines = content.lines().enumerate().peekable();

    // The `find` call consumes the iterator until it finds the start of a diff block.
    // The loop continues searching for more blocks from where the last one ended.
    while let Some((line_index, _)) = lines.by_ref().find(|(_, line)| {
        if !line.starts_with("```") {
            return false;
        }
        // Take the info string part after the backticks.
        let info_string = &line[3..];
        // Check if any "word" in the info string is "diff" or "patch".
        // We treat it as a comma-separated list of tags, where each tag can have multiple words.
        info_string.split(',').any(|part| {
            part.split_whitespace()
                .any(|word| word == "diff" || word == "patch")
        })
    }) {
        let diff_block_start_line = line_index + 1; // Convert 0-based index to 1-based line number

        // This temporary vec will hold all patch sections found in this block,
        // even if they are for the same file. They will be merged later.
        let mut unmerged_block_patches: Vec<Patch> = Vec::new();

        // State variables for the parser as it moves through the diff block.
        let mut current_file: Option<PathBuf> = None;
        let mut current_hunks: Vec<Hunk> = Vec::new();
        let mut current_hunk_lines: Vec<String> = Vec::new();
        let mut current_hunk_old_start_line: Option<usize> = None;
        let mut current_hunk_new_start_line: Option<usize> = None;
        let mut ends_with_newline_for_section = true;

        // Consume lines within the ```diff block
        for (_, line) in lines.by_ref() {
            if line == "```" {
                break; // End of block
            }

            if let Some(stripped_line) = line.strip_prefix("--- ") {
                // A `---` line always signals a new file section.
                // Finalize the previous file's patch section if it exists.
                if let Some(existing_file) = &current_file {
                    if !current_hunk_lines.is_empty() {
                        current_hunks.push(Hunk {
                            lines: std::mem::take(&mut current_hunk_lines),
                            old_start_line: current_hunk_old_start_line,
                            new_start_line: current_hunk_new_start_line,
                        });
                    }
                    if !current_hunks.is_empty() {
                        unmerged_block_patches.push(Patch {
                            file_path: existing_file.clone(),
                            hunks: std::mem::take(&mut current_hunks),
                            ends_with_newline: ends_with_newline_for_section,
                        });
                    }
                }

                // Reset for the new file section.
                // `current_file` is cleared and will be set by this `---` line or a subsequent `+++` line.
                current_file = None;
                current_hunk_lines.clear();
                current_hunk_old_start_line = None;
                current_hunk_new_start_line = None;
                ends_with_newline_for_section = true;

                let path_part = stripped_line.trim();
                if path_part == "/dev/null" || path_part == "a/dev/null" {
                    // This is a file creation patch. The path will be in the `+++` line.
                    // `current_file` remains `None` for now.
                } else {
                    // The path could be `a/path/to/file` or just `path/to/file`.
                    let path_str = path_part.strip_prefix("a/").unwrap_or(path_part);
                    current_file = Some(PathBuf::from(path_str.trim()));
                }
            } else if let Some(stripped_line) = line.strip_prefix("+++ ") {
                // If `current_file` is `None`, it means we saw `--- /dev/null` (or an unrecognised ---)
                // and are expecting the file path from this `+++` line.
                if current_file.is_none() {
                    let path_part = stripped_line.trim();
                    // The path could be `b/path/to/file` or just `path/to/file`.
                    let path_str = path_part.strip_prefix("b/").unwrap_or(path_part);
                    current_file = Some(PathBuf::from(path_str.trim()));
                }
                // Otherwise, we already have the path from the `---` line, so we ignore this `+++` line.
            } else if line.starts_with("@@") {
                // A hunk header line signals the end of the previous hunk.
                if !current_hunk_lines.is_empty() {
                    current_hunks.push(Hunk {
                        lines: std::mem::take(&mut current_hunk_lines),
                        old_start_line: current_hunk_old_start_line,
                        new_start_line: current_hunk_new_start_line,
                    });
                }
                // Parse the line number hint for the new hunk.
                let (old, new) = parse_hunk_header(line);
                current_hunk_old_start_line = old;
                current_hunk_new_start_line = new;
            } else if line.starts_with(['+', '-', ' ']) {
                // This is a line belonging to the current hunk.
                current_hunk_lines.push(line.to_string());
            } else if line.starts_with('\\') {
                // This special line indicates the file does not end with a newline.
                ends_with_newline_for_section = false;
            }
        }

        // Finalize the last hunk and patch section for the block after the loop ends.
        if !current_hunk_lines.is_empty() {
            current_hunks.push(Hunk {
                lines: current_hunk_lines,
                old_start_line: current_hunk_old_start_line,
                new_start_line: current_hunk_new_start_line,
            });
        }

        if let Some(file_path) = current_file {
            if !current_hunks.is_empty() {
                unmerged_block_patches.push(Patch {
                    file_path,
                    hunks: current_hunks,
                    ends_with_newline: ends_with_newline_for_section,
                });
            }
        } else if !current_hunks.is_empty() {
            // If we have hunks but never determined a file path, it's an error.
            return Err(ParseError::MissingFileHeader {
                line: diff_block_start_line,
            });
        }

        // Merge the collected patch sections from this block. This handles cases
        // where multiple `--- a/file` sections for the same file exist within
        // one ```diff block.
        let mut merged_block_patches: Vec<Patch> = Vec::new();
        for patch_section in unmerged_block_patches {
            if let Some(existing_patch) = merged_block_patches
                .iter_mut()
                .find(|p| p.file_path == patch_section.file_path)
            {
                // If a patch for this file already exists, just add the new hunks to it.
                existing_patch.hunks.extend(patch_section.hunks);
                // The 'ends_with_newline' from the *last* section for a file wins.
                existing_patch.ends_with_newline = patch_section.ends_with_newline;
            } else {
                // Otherwise, add it as a new patch.
                merged_block_patches.push(patch_section);
            }
        }
        all_patches.extend(merged_block_patches);
    }

    Ok(all_patches)
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
/// ```no_run
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

/// Applies a slice of `Patch` objects to a target directory.
///
/// This is a high-level convenience function that iterates through a list of
/// patches and applies each one to the filesystem using `apply_patch_to_file`.
/// It aggregates the results, including both successful applications and any
/// "hard" errors encountered (like I/O errors).
///
/// This function will continue applying patches even if some fail.
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
/// content, while delegating the core patching logic to [`apply_patch_to_content`].
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
/// # use mpatch::{parse_diffs, apply_patch_to_file, ApplyOptions};
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
/// let patches = parse_diffs(diff_content)?;
/// let patch = &patches[0];
///
/// // 3. Apply the patch to the directory.
/// let options = ApplyOptions { dry_run: false, fuzz_factor: 0.0 };
/// let result = apply_patch_to_file(patch, dir.path(), options)?;
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
    trace!("    Path is safe.");

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

/// An iterator that applies hunks from a patch one by one.
///
/// This struct provides fine-grained control over the patch application process.
/// It allows you to apply hunks sequentially, inspect the intermediate state of
/// the content, and handle results on a per-hunk basis.
///
/// The iterator yields a [`HunkApplyStatus`] for each hunk in the patch.
#[derive(Debug)]
pub struct HunkApplier<'a> {
    hunks: std::slice::Iter<'a, Hunk>,
    current_lines: Vec<String>,
    options: &'a ApplyOptions,
    patch_ends_with_newline: bool,
}

impl<'a> HunkApplier<'a> {
    /// Creates a new `HunkApplier` to begin a step-by-step patch operation.
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
    pub fn current_lines(&self) -> &[String] {
        &self.current_lines
    }

    /// Consumes the applier and returns the final vector of lines.
    pub fn into_lines(self) -> Vec<String> {
        self.current_lines
    }

    /// Consumes the applier and returns the final formatted content as a string,
    /// respecting the patch's `ends_with_newline` property.
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
/// # use mpatch::{parse_diffs, apply_patch_to_lines, ApplyOptions};
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
/// let patches = parse_diffs(&diff_str)?;
/// let patch = &patches[0];
///
/// // 3. Apply the patch to the lines in memory.
/// let options = ApplyOptions { dry_run: false, fuzz_factor: 0.0 };
/// let result = apply_patch_to_lines(patch, Some(&original_lines), &options);
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
            if let HunkApplyStatus::Failed(error) = &status {
                warn!("  Failed to apply Hunk {}. {}", hunk_index, error);
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
/// of lines, consider using [`apply_patch_to_lines`].
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
/// # use mpatch::{parse_diffs, apply_patch_to_content, ApplyOptions};
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
/// let patches = parse_diffs(&diff_str)?;
/// let patch = &patches[0];
///
/// // 3. Apply the patch to the content in memory.
/// let options = ApplyOptions { dry_run: false, fuzz_factor: 0.0 };
/// let result = apply_patch_to_content(patch, Some(original_content), &options);
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
/// - `Applied`: The hunk was successfully applied. The `location` and `match_type` are provided.
/// - `SkippedNoChanges`: The hunk contained only context lines and was skipped.
/// - `Failed`: The hunk could not be applied. The reason is provided in the associated [`HunkApplyError`].
///
/// # Example
///
/// ```rust
/// # use mpatch::{parse_diffs, apply_hunk_to_lines, ApplyOptions, HunkApplyStatus, HunkLocation, MatchType};
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
/// let patches = parse_diffs(&diff_str)?;
/// let hunk = &patches[0].hunks[0];
///
/// // 3. Apply the hunk to the lines in memory.
/// let options = ApplyOptions { dry_run: false, fuzz_factor: 0.0 };
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
    if !hunk.has_changes() {
        debug!("  Skipping hunk (no changes).");
        return HunkApplyStatus::SkippedNoChanges;
    }

    match find_hunk_location_in_lines(hunk, target_lines, options) {
        Ok((location, match_type)) => {
            let replace_block = hunk.get_replace_block();
            let replaced_lines: Vec<String> = target_lines
                .splice(
                    location.start_index..location.start_index + location.length,
                    replace_block.into_iter().map(String::from),
                )
                .collect();
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
pub trait HunkFinder {
    /// Finds the location to apply a hunk to a slice of lines.
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
#[derive(Debug)]
pub struct DefaultHunkFinder<'a> {
    options: &'a ApplyOptions,
}

impl<'a> DefaultHunkFinder<'a> {
    /// Creates a new finder with the given options.
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

        // --- STRATEGY 2: Exact Match (Ignoring Trailing Whitespace) ---
        // Handles minor formatting differences.
        trace!("    Attempting exact match (ignoring trailing whitespace)...");
        {
            let match_stripped: Vec<_> = match_block.iter().map(|s| s.trim_end()).collect();
            let stripped_matches: Box<dyn Iterator<Item = usize>> =
                if match_block.len() <= target_lines.len() {
                    Box::new(
                        target_lines
                            .windows(match_block.len())
                            .enumerate()
                            .filter(move |(_, window)| {
                                window
                                    .iter()
                                    .map(|s| s.as_ref().trim_end())
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

            // Hoist invariants for performance
            let match_stripped_lines: Vec<_> = match_block.iter().map(|s| s.trim_end()).collect();
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
            let search_ranges = Self::find_search_ranges(match_block, target_lines, len);
            trace!("    Using search ranges: {:?}", search_ranges);

            // When the anchor heuristic fails, the search can be slow. We parallelize the
            // scoring of all possible windows using Rayon if the `parallel` feature is enabled.
            #[cfg(feature = "parallel")]
            let all_scored_windows: Vec<(f64, f64, f64, f64, f64, usize, usize)> = search_ranges
                .par_iter()
                .flat_map(|&(range_start, range_end)| {
                    // By creating local references, we ensure that the inner `move` closures
                    // capture these references (which are `Copy`) instead of attempting to move
                    // the original non-`Copy` `Vec` and `String` from the outer scope.
                    let match_stripped_lines = &match_stripped_lines;
                    let match_content = &match_content;
                    let target_slice = &target_lines[range_start..range_end];

                    (min_len..=max_len)
                        .into_par_iter()
                        .filter(move |&window_len| window_len <= target_slice.len())
                        .flat_map(move |window_len| {
                            (0..=target_slice.len() - window_len)
                                .into_par_iter()
                                .map(move |i| {
                                    let window = &target_slice[i..i + window_len];
                                    let absolute_index = range_start + i;

                                    // HYBRID SCORING: (Copied from original sequential loop)
                                    let window_stripped_lines: Vec<_> =
                                        window.iter().map(|s| s.as_ref().trim_end()).collect();
                                    let diff_lines = similar::TextDiff::from_slices(
                                        &window_stripped_lines,
                                        match_stripped_lines,
                                    );
                                    let ratio_lines = diff_lines.ratio() as f64;
                                    let window_content = window_stripped_lines.join("\n");
                                    let diff_words = similar::TextDiff::from_words(
                                        &window_content,
                                        match_content,
                                    );
                                    let ratio_words = diff_words.ratio() as f64;
                                    let ratio = 0.6 * ratio_lines + 0.4 * ratio_words;
                                    let size_diff = window_len.abs_diff(len) as f64;
                                    let penalty = (size_diff / len.max(window_len) as f64) * 0.2;
                                    let score = ratio - penalty;

                                    (
                                        score,
                                        ratio,
                                        ratio_lines,
                                        ratio_words,
                                        penalty,
                                        absolute_index,
                                        window_len,
                                    )
                                })
                        })
                })
                .collect();

            #[cfg(not(feature = "parallel"))]
            let all_scored_windows: Vec<(f64, f64, f64, f64, f64, usize, usize)> = search_ranges
                .iter()
                .flat_map(|&(range_start, range_end)| {
                    // By creating local references, we ensure that the inner `move` closures
                    // capture these references (which are `Copy`) instead of attempting to move
                    // the original non-`Copy` `Vec` and `String` from the outer scope.
                    let match_stripped_lines = &match_stripped_lines;
                    let match_content = &match_content;
                    let target_slice = &target_lines[range_start..range_end];

                    (min_len..=max_len)
                        .filter(move |&window_len| window_len <= target_slice.len())
                        .flat_map(move |window_len| {
                            (0..=target_slice.len() - window_len).map(move |i| {
                                let window = &target_slice[i..i + window_len];
                                let absolute_index = range_start + i;

                                // HYBRID SCORING: (Copied from original sequential loop)
                                let window_stripped_lines: Vec<_> =
                                    window.iter().map(|s| s.as_ref().trim_end()).collect();
                                let diff_lines = similar::TextDiff::from_slices(
                                    &window_stripped_lines,
                                    match_stripped_lines,
                                );
                                let ratio_lines = diff_lines.ratio() as f64;
                                let window_content = window_stripped_lines.join("\n");
                                let diff_words =
                                    similar::TextDiff::from_words(&window_content, match_content);
                                let ratio_words = diff_words.ratio() as f64;
                                let ratio = 0.6 * ratio_lines + 0.4 * ratio_words;
                                let size_diff = window_len.abs_diff(len) as f64;
                                let penalty = (size_diff / len.max(window_len) as f64) * 0.2;
                                let score = ratio - penalty;

                                (
                                    score,
                                    ratio,
                                    ratio_lines,
                                    ratio_words,
                                    penalty,
                                    absolute_index,
                                    window_len,
                                )
                            })
                        })
                })
                .collect();

            // Process the collected results sequentially to find the best match and handle tie-breaking.
            for (score, ratio, ratio_lines, ratio_words, penalty, absolute_index, window_len) in
                all_scored_windows
            {
                // This is the same logic as in the original sequential loop.
                if score > best_score {
                    trace!(
                        "        New best score: {:.3} (ratio {:.3} [l:{:.3},w:{:.3}], penalty {:.3}) at index {} (window len {})",
                        score,
                        ratio,
                        ratio_lines,
                        ratio_words,
                        penalty,
                        absolute_index,
                        window_len
                    );
                    best_score = score;
                    best_ratio_at_best_score = ratio;
                    potential_matches.clear();
                    potential_matches.push((absolute_index, window_len));
                } else if (score - best_score).abs() < 1e-9 {
                    // Tie in score. Prefer the window size closer to the match block size.
                    // This is mostly redundant due to the penalty, but good for perfect ties.
                    if potential_matches.is_empty() {
                        potential_matches.push((absolute_index, window_len));
                        continue;
                    }
                    let current_best_len = potential_matches[0].1;
                    let dist_new = window_len.abs_diff(len);
                    let dist_best = current_best_len.abs_diff(len);

                    if dist_new < dist_best {
                        // This window size is a better fit, so it's the new best.
                        trace!(
                            "        Tie in score ({:.3}), but window len {} is closer to hunk len {}. New best.",
                            score,
                            window_len,
                            len
                        );
                        best_ratio_at_best_score = ratio;
                        potential_matches.clear();
                        potential_matches.push((absolute_index, window_len));
                    } else if dist_new == dist_best {
                        // Same distance, so it's also a potential match.
                        trace!(
                            "        Tie in score ({:.3}) and window distance. Adding candidate: index {}, len {}",
                            score,
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
                        "    Found best fuzzy match at index {} (length {}, similarity: {:.3}).",
                        start, len, best_ratio_at_best_score
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
                                    "    Tie-broke ambiguous fuzzy match using line number. Best match is at index {} (length {}, similarity: {:.3}).",
                                    start, len, best_ratio_at_best_score
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
                trace!(
                    "    Fuzzy match: Best location (index {}, len {}, similarity {:.3}) did not meet threshold of {:.2}.",
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
            let target_stripped: Vec<_> =
                target_lines.iter().map(|s| s.as_ref().trim_end()).collect();
            let match_stripped: Vec<_> = match_block.iter().map(|s| s.trim_end()).collect();
            let diff = TextDiff::from_slices(&target_stripped, &match_stripped);
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
/// of lines, consider using [`find_hunk_location_in_lines`].
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
/// # use mpatch::{parse_diffs, find_hunk_location, HunkLocation, ApplyOptions, MatchType};
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
/// let patches = parse_diffs(diff_content)?;
/// let hunk = &patches[0].hunks[0];
///
/// let options = ApplyOptions { dry_run: false, fuzz_factor: 0.0 };
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
/// This is a more allocation-friendly version of [`find_hunk_location`] that
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
/// # use mpatch::{parse_diffs, find_hunk_location_in_lines, HunkLocation, ApplyOptions, MatchType};
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
/// let patches = parse_diffs(diff_content)?;
/// let hunk = &patches[0].hunks[0];
///
/// let options = ApplyOptions { dry_run: false, fuzz_factor: 0.0 };
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
