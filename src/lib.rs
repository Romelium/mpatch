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
//! ## Why mpatch?
//!
//! Standard patching tools are fragile. They rely on exact line numbers and
//! byte-for-byte context matches. In a modern workflow involving LLMs, code
//! often drifts quickly. `mpatch` solves this by:
//!
//! - **Flexible Anchoring**: Searching for the best fit in a file, even if the
//!   target has moved dozens of lines.
//! - **Fuzzy Similarity**: Accepting matches that are "close enough" (e.g.,
//!   modified comments or minor whitespace changes).
//! - **Indentation Awareness**: Dynamically re-aligning the indentation of
//!   injected code to match the target file's style.
//! - **Candidate Backtracking**: Backtracking through alternative candidate
//!   locations if an initial fuzzy window fails validation (e.g., orphan additions).
//! - **Statement Re-alignment**: Recognizing statements across line-break or
//!   formatting differences to prevent multi-line refactoring mismatches.
//! - **Wildcard & Ellipsis Matching**: Recognizing code omissions (`...`, `// ... existing code ...`)
//!   across single and multi-segment hunks, reconstructing multi-line code gaps while preserving untouched code.
//! - **Atomic Application**: Staging multi-file changes in-memory and committing
//!   to disk if and only if all hunks across all patches apply cleanly. If any hunk fails,
//!   the filesystem remains completely untouched.
//! - **Three-Way Merge**: Performing line-level 3-way merges with Diff3 conflict markers
//!   when integrating divergent changes against a common ancestor.
//! - **Inline Diffs & Path Suggestions**: Sub-line word-level diff visualization and
//!   fuzzy path suggestions for misspelled or moved target files.
//!
//! ## Supported Formats
//!
//! `mpatch` automatically recognizes and parses four diff and patch formats:
//!
//! 1. **Markdown Blocks:** Standard chat/assistant output fenced in code blocks
//!    (```` ```diff ````, ```` ```patch ````, or language-tagged blocks with diff headers).
//! 2. **Unified Diffs:** Standard `git diff` or `diff -u` patches with `--- a/file` and `+++ b/file` headers.
//! 3. **Aider Search/Replace Blocks:** Search and replace blocks with `<<<<<<< SEARCH` (or `ORIGINAL`),
//!    `=======`, and `>>>>>>> REPLACE` (or `UPDATED`). Target file paths are inferred automatically
//!    from preceding markdown text, code comments, or block headers.
//! 4. **Conflict Markers:** Standard 3-way merge conflict markers (`<<<<`, `====`, `>>>>`).
//!    Because conflict markers lack file path headers, they default to `patch_target` when applied to files.
//!
//! In both Unified Diffs and Aider Search/Replace blocks, **wildcard ellipsis lines** (such as `...` or
//! `// ... existing code ...`) are supported. `mpatch` reconstructs the multi-line code gaps between anchors,
//! preserving untouched code while strictly preventing runaway gaps across function boundaries.
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
//! ## Applying Aider Search/Replace Blocks with Wildcards
//!
//! For AI agent workflows using Aider search/replace blocks, [`patch_content_str()`] or
//! [`parse_auto()`] handles file path detection and wildcard ellipsis matching automatically:
//!
//! ````rust
//! use mpatch::{patch_content_str, ApplyOptions};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let original_code = "def compute(x):\n    setup()\n    res = x * 2\n    teardown()\n    return res\n";
//! let aider_diff = "math.py\n<<<<<<< SEARCH\ndef compute(x):\n    ...\n    res = x * 2\n=======\ndef compute(x):\n    ...\n    res = x * 4\n>>>>>>> REPLACE\n";
//!
//! let options = ApplyOptions::new();
//! let new_code = patch_content_str(aider_diff, Some(original_code), &options)?;
//!
//! assert!(new_code.contains("res = x * 4"));
//! assert!(new_code.contains("setup()"));
//! assert!(new_code.contains("teardown()"));
//! # Ok(())
//! # }
//! ````
//!
//! ## Atomic (All-or-Nothing) Patch Application
//!
//! When applying changes across multiple files or hunks, you can guarantee that the
//! filesystem is never left in a partially patched state by using [`apply_patches_to_dir_atomic()`]
//! or [`apply_patch_to_file_atomic()`]. Changes are committed if and only if all hunks succeed:
//!
//! ````rust
//! use mpatch::{parse_auto, apply_patches_to_dir_atomic, ApplyOptions};
//! use std::fs;
//! use tempfile::tempdir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let dir = tempdir()?;
//! let file1 = dir.path().join("file1.txt");
//! let file2 = dir.path().join("file2.txt");
//! fs::write(&file1, "foo\n")?;
//! fs::write(&file2, "bar\n")?;
//!
//! let diff = r#"
//! --- a/file1.txt
//! +++ b/file1.txt
//! @@ -1 +1 @@
//! -foo
//! +foo_updated
//! --- a/file2.txt
//! +++ b/file2.txt
//! @@ -1 +1 @@
//! -bar
//! +bar_updated
//! "#;
//!
//! let patches = parse_auto(diff)?;
//! let batch = apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::exact());
//!
//! assert!(batch.all_applied_cleanly());
//! assert_eq!(fs::read_to_string(&file1)?, "foo_updated\n");
//! assert_eq!(fs::read_to_string(&file2)?, "bar_updated\n");
//! # Ok(())
//! # }
//! ````
//!
//! ## Three-Way Line Merging
//!
//! For merging divergent branches or resolving AI suggestions against an ancestor:
//!
//! ````rust
//! use mpatch::merge_three_way;
//!
//! let base   = "Apples\nBananas\nCherries\n";
//! let ours   = "Apples\nBlueberries\nCherries\n";
//! let theirs = "Apples\nBananas\nCranberries\n";
//!
//! let (merged, is_conflicted) = merge_three_way(base, ours, theirs, None);
//! assert!(!is_conflicted);
//! assert_eq!(merged, "Apples\nBlueberries\nCranberries\n");
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
//!   (Markdown, Unified Diff, Aider Search/Replace Blocks, or Conflict Markers) and parses the content accordingly.
//! - [`parse_single_patch()`]: A convenient wrapper around `parse_auto()` that ensures
//!   the input contains exactly one patch, returning a `Result<Patch, _>`.
//! - [`parse_diffs()`]: Scans a string for markdown code blocks containing diffs or search/replace blocks.
//! - [`parse_patches()`]: A lower-level parser that processes a raw unified diff string
//!   directly, without needing markdown fences.
//! - [`parse_aider()`]: Parses a string containing Aider-style search/replace blocks
//!   (`<<<<<<< SEARCH`, `=======`, `>>>>>>> REPLACE`) into patches.
//! - [`parse_aider_from_lines()`]: Parses an iterator of lines containing Aider-style search/replace blocks.
//! - [`parse_conflict_markers()`]: Parses a string containing conflict markers
//!   (`<<<<`, `====`, `>>>>`) into patches.
//! - [`parse_patches_from_lines()`]: The lowest-level parser. It operates on an iterator
//!   of lines, which is useful for streaming or avoiding large string allocations.
//!
//! You can also use [`detect_patch()`] to identify the format (Markdown, Unified, Aider, or Conflict)
//! without parsing the full content.
//!
//! #### 2. Applying
//!
//! Once you have a `Patch`, you can apply it using one of the `apply` functions:
//!
//! - [`apply_patches_to_dir()`]: Applies a list of patches to a directory. This is
//!   ideal for processing multi-file diffs.
//! - [`apply_patches_to_dir_atomic()`]: Applies a list of patches to a directory atomically.
//!   Changes are written to disk if and only if all hunks across all patches apply cleanly.
//! - [`apply_patch_to_file()`]: The most convenient function for applying a single
//!   patch to a file. It handles reading the original file and writing the new content
//!   back to disk. If the patch results in empty content, the file is deleted.
//! - [`apply_patch_to_file_atomic()`]: Applies a single patch to a file atomically,
//!   modifying disk only if all hunks apply cleanly.
//! - [`apply_patch_to_content()`]: A pure function for in-memory operations. It takes
//!   the original content as a string and returns the new content.
//! - [`apply_patch_to_lines()`]: Similar to `apply_patch_to_content()`, but operates
//!   directly on a slice of lines, avoiding string allocations.
//!
//! Each of these also has a "strict" `try_` variant (e.g., [`try_apply_patch_to_file()`],
//! [`try_apply_patch_to_file_atomic()`], [`try_apply_patches_to_dir()`], and
//! [`try_apply_patches_to_dir_atomic()`]) that treats partial applications as an error,
//! simplifying the common apply-or-fail workflow.
//!
//! You can also manipulate patches before application:
//!
//! - [`invert_patches()`]: Reverses a list of patches (swapping additions and deletions).
//!
//! ### Granular Application & Utilities
//!
//! - [`apply_hunk_to_lines()`]: Applies a single hunk to a mutable vector of lines in-place, with automatic candidate backtracking.
//! - [`find_hunk_location()`]: Finds the location to apply a hunk to a given text content without modifying it.
//! - [`find_hunk_location_in_lines()`]: Finds the location to apply a hunk to a slice of lines without modifying it.
//! - [`DefaultHunkFinder`]: The default, built-in search strategy for locating hunks and candidate match locations.
//! - [`format_inline_diff()`]: Formats an inline word-level diff with colored ANSI highlights.
//! - [`merge_three_way()`]: Performs a 3-way line merge with Diff3 conflict markers.
//! - [`suggest_close_file_paths()`]: Finds close matching file paths in a target directory when a patch specifies a missing file.
//! - [`is_ellipsis_line()`]: Tests whether a line represents an omitted code ellipsis wildcard.
//! - [`is_plausible_file_path()`]: Validates whether a candidate string represents a plausible file path.
//! - [`extract_file_path_from_line()`]: Extracts a target file path from conversational headings or preceding markdown lines.
//! - [`normalize_candidate_path()`]: Normalizes relative paths by stripping enclosing delimiters and prefixes.
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
//!   surrounding code. It is also robust against indentation differences (e.g.,
//!   patches nested in Markdown lists).
//! - **Smart Indentation:** When applying a patch via fuzzy matching, `mpatch`
//!   dynamically adjusts the indentation of added lines to match the surrounding
//!   code in the target file, preventing style corruption.
//! - **Wildcard Matching:** When applying search/replace hunks containing wildcard ellipsis lines
//!   (such as `...` or `// ... existing code ...`), `mpatch` reconstructs the multi-line gaps
//!   between anchors, preserving untouched intermediate code while strictly guarding against
//!   runaway gaps or syntax false positives.
//!
//! ## Advanced Usage
//!
//! ### Configuring [`ApplyOptions`]
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
//! ### Atomic (All-or-Nothing) Batch Application
//!
//! For batch operations where any failure across any file must leave the entire filesystem
//! untouched, use [`try_apply_patches_to_dir_atomic()`]:
//!
//! ````rust
//! use mpatch::{parse_auto, try_apply_patches_to_dir_atomic, ApplyOptions};
//! use tempfile::tempdir;
//! use std::fs;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let dir = tempdir()?;
//! let file_path = dir.path().join("server.rs");
//! fs::write(&file_path, "fn start() {}\n")?;
//!
//! let diff = r#"
//! --- a/server.rs
//! +++ b/server.rs
//! @@ -1 +1 @@
//! -fn start() {}
//! +fn start() { init(); }
//! "#;
//!
//! let patches = parse_auto(diff)?;
//! let options = ApplyOptions::exact();
//!
//! // Only writes to disk if all patches and hunks apply cleanly
//! let batch = try_apply_patches_to_dir_atomic(&patches, dir.path(), options)?;
//! assert!(batch.all_applied_cleanly());
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
//! ### Creating Patches
//!
//! You can also use `mpatch` to generate patches by comparing two strings using
//! [`Patch::from_texts()`].
//!
//! ````rust
//! use mpatch::Patch;
//!
//! let old_text = "fn main() { println!(\"Old\"); }";
//! let new_text = "fn main() { println!(\"New\"); }";
//!
//! // Create a patch with 3 lines of context
//! let patch = Patch::from_texts("src/main.rs", old_text, new_text, 3).unwrap();
//!
//! assert_eq!(patch.hunks.len(), 1);
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
//!   mpatch = { version = "1.6.4", default-features = false }
//!   ```
//!   You might want to disable this feature if you are compiling for a target that
//!   does not support threading (like `wasm32-unknown-unknown`) or if you want to
//!   minimize dependencies and binary size.
use log::{debug, info, trace, warn};
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use similar::TextDiff;
use std::collections::HashMap;
use std::fmt::Write as _;
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
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::ParseError;
    /// let err = ParseError::MissingFileHeader { line: 10 };
    /// ```
    #[error("Diff block starting on line {line} was found without a file path header (e.g., '--- a/path/to/file')")]
    MissingFileHeader {
        /// The line number where the diff block started.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::ParseError;
        /// let err = ParseError::MissingFileHeader { line: 10 };
        /// match err {
        ///     ParseError::MissingFileHeader { line } => assert_eq!(line, 10),
        /// }
        /// ```
        line: usize,
    },
}

/// Represents errors that can occur when parsing a diff expected to contain exactly one patch.
///
/// This enum is returned by [`parse_single_patch()`] when the input content does not
/// result in exactly one `Patch` object. It handles errors from format detection,
/// parsing, and patch count validation.
///
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{SingleParseError, ParseError};
    /// let err = SingleParseError::Parse(ParseError::MissingFileHeader { line: 1 });
    /// ```
    #[error("Failed to parse diff content")]
    Parse(#[from] ParseError),

    /// The provided diff content did not contain any valid patches (Markdown blocks,
    /// Unified Diffs, Aider search/replace blocks, or Conflict Markers).
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::SingleParseError;
    /// let err = SingleParseError::NoPatchesFound;
    /// ```
    #[error("No patches were found in the provided diff content")]
    NoPatchesFound,

    /// The provided diff content contained patches for more than one file, which is not
    /// supported by this function. Use [`parse_diffs()`] for multi-file operations.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::SingleParseError;
    /// let err = SingleParseError::MultiplePatchesFound(3);
    /// ```
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
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::PatchError;
    /// use std::path::PathBuf;
    /// let err = PatchError::PathTraversal(PathBuf::from("../../etc/passwd"));
    /// ```
    #[error("Path '{0}' resolves outside the target directory. Aborting for security.")]
    PathTraversal(PathBuf),
    /// The target file for a patch could not be found, and the patch did not
    /// appear to be for file creation (i.e., its first hunk was not an addition-only hunk).
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::PatchError;
    /// use std::path::PathBuf;
    /// let err = PatchError::TargetNotFound(PathBuf::from("missing.txt"));
    /// ```
    #[error("Target file not found for patching: {0}")]
    TargetNotFound(PathBuf),
    /// The user does not have permission to read or write to the specified path.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::PatchError;
    /// use std::path::PathBuf;
    /// let err = PatchError::PermissionDenied { path: PathBuf::from("readonly.txt") };
    /// ```
    #[error("Permission denied for path: {path:?}")]
    PermissionDenied {
        /// The path that could not be accessed.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::PatchError;
        /// use std::path::PathBuf;
        /// let err = PatchError::PermissionDenied { path: PathBuf::from("readonly.txt") };
        /// match err {
        ///     PatchError::PermissionDenied { path } => assert_eq!(path.to_str(), Some("readonly.txt")),
        ///     _ => unreachable!(),
        /// }
        /// ```
        path: PathBuf,
    },
    /// The target path for a patch exists but is a directory, not a file.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::PatchError;
    /// use std::path::PathBuf;
    /// let err = PatchError::TargetIsDirectory { path: PathBuf::from("src/") };
    /// ```
    #[error("Target path is a directory, not a file: {path:?}")]
    TargetIsDirectory {
        /// The path that resolved to a directory.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::PatchError;
        /// use std::path::PathBuf;
        /// let err = PatchError::TargetIsDirectory { path: PathBuf::from("src/") };
        /// match err {
        ///     PatchError::TargetIsDirectory { path } => assert_eq!(path.to_str(), Some("src/")),
        ///     _ => unreachable!(),
        /// }
        /// ```
        path: PathBuf,
    },
    /// An I/O error occurred while reading or writing a file.
    /// This is a "hard" error that stops the entire process.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::PatchError;
    /// use std::path::PathBuf;
    /// use std::io::{Error, ErrorKind};
    /// let err = PatchError::Io { path: PathBuf::from("file.txt"), source: Error::new(ErrorKind::Other, "oh no") };
    /// ```
    #[error("I/O error while processing {path:?}: {source}")]
    Io {
        /// The path associated with the I/O error.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::PatchError;
        /// use std::path::PathBuf;
        /// use std::io::{Error, ErrorKind};
        /// let err = PatchError::Io { path: PathBuf::from("file.txt"), source: Error::new(ErrorKind::Other, "oh no") };
        /// match err {
        ///     PatchError::Io { path, .. } => assert_eq!(path.to_str(), Some("file.txt")),
        ///     _ => unreachable!(),
        /// }
        /// ```
        path: PathBuf,
        /// The underlying I/O error.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::PatchError;
        /// use std::path::PathBuf;
        /// use std::io::{Error, ErrorKind};
        /// let err = PatchError::Io { path: PathBuf::from("file.txt"), source: Error::new(ErrorKind::Other, "oh no") };
        /// match err {
        ///     PatchError::Io { source, .. } => assert_eq!(source.kind(), ErrorKind::Other),
        ///     _ => unreachable!(),
        /// }
        /// ```
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
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{StrictApplyError, PatchError};
    /// use std::path::PathBuf;
    /// let err = StrictApplyError::Patch(PatchError::TargetNotFound(PathBuf::from("file.txt")));
    /// ```
    #[error(transparent)]
    Patch(#[from] PatchError),

    /// The patch was only partially applied, with some hunks failing.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{StrictApplyError, ApplyResult};
    /// let report = ApplyResult { hunk_results: vec![] };
    /// let err = StrictApplyError::PartialApply { report };
    /// ```
    #[error("Patch applied partially. See report for details.")]
    PartialApply {
        /// The detailed report of the operation, including which hunks succeeded/failed.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::{StrictApplyError, ApplyResult};
        /// let report = ApplyResult { hunk_results: vec![] };
        /// let err = StrictApplyError::PartialApply { report };
        /// match err {
        ///     StrictApplyError::PartialApply { report } => assert!(report.all_applied_cleanly()),
        ///     _ => unreachable!(),
        /// }
        /// ```
        report: ApplyResult,
    },
}

/// Represents errors that can occur during strict batch patch operations.
///
/// This enum is returned by functions like [`try_apply_patches_to_dir()`] and
/// [`try_apply_patches_to_dir_atomic()`], which treat partial applications or hard errors
/// across any patch in a batch as an error.
///
/// # Examples
///
/// ```rust
/// # use mpatch::{parse_auto, try_apply_patches_to_dir_atomic, ApplyOptions, StrictBatchApplyError};
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-wrong\n+new\n";
/// let patches = parse_auto(diff)?;
/// let options = ApplyOptions::exact();
///
/// let result = try_apply_patches_to_dir_atomic(&patches, dir.path(), options);
/// assert!(matches!(result, Err(StrictBatchApplyError::Failed { .. })));
/// # Ok(())
/// # }
/// ```
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum StrictBatchApplyError {
    /// One or more patch operations failed or applied partially.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{BatchResult, StrictBatchApplyError};
    /// let batch = BatchResult { results: vec![] };
    /// let err = StrictBatchApplyError::Failed { batch_result: batch };
    /// match err {
    ///     StrictBatchApplyError::Failed { batch_result } => assert!(batch_result.all_succeeded()),
    /// }
    /// ```
    #[error("One or more patch operations failed. See batch result for details.")]
    Failed {
        /// The aggregated results of the batch operations.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use mpatch::{BatchResult, StrictBatchApplyError};
        /// let batch = BatchResult { results: vec![] };
        /// let err = StrictBatchApplyError::Failed { batch_result: batch };
        /// if let StrictBatchApplyError::Failed { batch_result } = err {
        ///     assert_eq!(batch_result.results.len(), 0);
        /// }
        /// ```
        batch_result: BatchResult,
    },
}

/// Represents errors that can occur during the high-level [`patch_content_str()`] operation.
///
/// This enum consolidates all possible failures from the one-shot workflow,
/// including parsing errors, finding the wrong number of patches, or failures
/// during the strict application process.
///
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{OneShotError, ParseError};
    /// let err = OneShotError::Parse(ParseError::MissingFileHeader { line: 1 });
    /// ```
    #[error("Failed to parse diff content")]
    Parse(#[from] ParseError),

    /// An error occurred while applying the patch. This includes partial applications.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{OneShotError, StrictApplyError, PatchError};
    /// use std::path::PathBuf;
    /// let err = OneShotError::Apply(StrictApplyError::Patch(PatchError::TargetNotFound(PathBuf::from("file.txt"))));
    /// ```
    #[error("Failed to apply patch")]
    Apply(#[from] StrictApplyError),

    /// The provided diff content did not contain any valid patches (Markdown blocks,
    /// Unified Diffs, Aider search/replace blocks, or Conflict Markers).
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::OneShotError;
    /// let err = OneShotError::NoPatchesFound;
    /// ```
    #[error("No patches were found in the provided diff content")]
    NoPatchesFound,

    /// The provided diff content contained patches for more than one file, which is not
    /// supported by this simplified function. Use [`parse_diffs()`] and
    /// [`apply_patches_to_dir()`] for multi-file operations.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::OneShotError;
    /// let err = OneShotError::MultiplePatchesFound(2);
    /// ```
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
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::HunkApplyError;
    /// let err = HunkApplyError::ContextNotFound;
    /// ```
    #[error("Context not found")]
    ContextNotFound,
    /// An exact match for the hunk's context was found in multiple locations,
    /// and the ambiguity could not be resolved by the line number hint.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::HunkApplyError;
    /// let err = HunkApplyError::AmbiguousExactMatch(vec![10, 20]);
    /// ```
    #[error("Ambiguous exact match found at lines: {0:?}")]
    AmbiguousExactMatch(Vec<usize>),
    /// A fuzzy match for the hunk's context was found in multiple locations with
    /// the same top score, and the ambiguity could not be resolved.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::HunkApplyError;
    /// let err = HunkApplyError::AmbiguousFuzzyMatch(vec![(5, 3), (15, 3)]);
    /// ```
    #[error("Ambiguous fuzzy match found at locations: {0:?}")]
    AmbiguousFuzzyMatch(Vec<(usize, usize)>),
    /// The best fuzzy match found was below the required similarity threshold.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{HunkApplyError, HunkLocation};
    /// let err = HunkApplyError::FuzzyMatchBelowThreshold { best_score: 0.5, threshold: 0.7, location: HunkLocation { start_index: 0, length: 5 } };
    /// ```
    #[error("Best fuzzy match at {location} (score: {best_score:.3}) was below threshold ({threshold:.3})")]
    FuzzyMatchBelowThreshold {
        /// The similarity score of the best match found.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::{HunkApplyError, HunkLocation};
        /// let err = HunkApplyError::FuzzyMatchBelowThreshold { best_score: 0.5, threshold: 0.7, location: HunkLocation { start_index: 0, length: 5 } };
        /// match err {
        ///     HunkApplyError::FuzzyMatchBelowThreshold { best_score, .. } => assert_eq!(best_score, 0.5),
        ///     _ => unreachable!(),
        /// }
        /// ```
        best_score: f64,
        /// The minimum similarity score required for a successful match.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::{HunkApplyError, HunkLocation};
        /// let err = HunkApplyError::FuzzyMatchBelowThreshold { best_score: 0.5, threshold: 0.7, location: HunkLocation { start_index: 0, length: 5 } };
        /// match err {
        ///     HunkApplyError::FuzzyMatchBelowThreshold { threshold, .. } => assert_eq!(threshold, 0.7),
        ///     _ => unreachable!(),
        /// }
        /// ```
        threshold: f32,
        /// The location of the best-scoring (but rejected) fuzzy match.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::{HunkApplyError, HunkLocation};
        /// let err = HunkApplyError::FuzzyMatchBelowThreshold { best_score: 0.5, threshold: 0.7, location: HunkLocation { start_index: 0, length: 5 } };
        /// match err {
        ///     HunkApplyError::FuzzyMatchBelowThreshold { location, .. } => assert_eq!(location.length, 5),
        ///     _ => unreachable!(),
        /// }
        /// ```
        location: HunkLocation,
    },
}

/// Describes the method used to successfully locate and apply a hunk.
///
/// This enum is included in the [`HunkApplyStatus::Applied`] variant and provides
/// insight into how `mpatch` found the location for a hunk, which is useful for
/// logging and diagnostics.
///
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::MatchType;
    /// let match_type = MatchType::Exact;
    /// ```
    Exact,
    /// An exact match after ignoring trailing whitespace on each line.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::MatchType;
    /// let match_type = MatchType::ExactIgnoringWhitespace;
    /// ```
    ExactIgnoringWhitespace,
    /// A fuzzy match found using a similarity algorithm.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::MatchType;
    /// let match_type = MatchType::Fuzzy { score: 0.85 };
    /// ```
    Fuzzy {
        /// The similarity score of the match (0.0 to 1.0).
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::MatchType;
        /// let match_type = MatchType::Fuzzy { score: 0.85 };
        /// match match_type {
        ///     MatchType::Fuzzy { score } => assert_eq!(score, 0.85),
        ///     _ => unreachable!(),
        /// }
        /// ```
        score: f64,
    },
}

/// The result of applying a single hunk.
///
/// This enum is returned by [`apply_hunk_to_lines()`] and is the item type for the
/// [`HunkApplier`] iterator. It provides a detailed outcome for each individual
/// hunk within a patch.
///
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{HunkApplyStatus, HunkLocation, MatchType};
    /// let status = HunkApplyStatus::Applied {
    ///     location: HunkLocation { start_index: 0, length: 2 },
    ///     match_type: MatchType::Exact,
    ///     replaced_lines: vec!["old line".to_string()],
    /// };
    /// ```
    Applied {
        /// The location where the hunk was applied.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::{HunkApplyStatus, HunkLocation, MatchType};
        /// let status = HunkApplyStatus::Applied {
        ///     location: HunkLocation { start_index: 0, length: 2 },
        ///     match_type: MatchType::Exact,
        ///     replaced_lines: vec!["old line".to_string()],
        /// };
        /// match status {
        ///     HunkApplyStatus::Applied { location, .. } => assert_eq!(location.start_index, 0),
        ///     _ => unreachable!(),
        /// }
        /// ```
        location: HunkLocation,
        /// The type of match that was used to find the location.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::{HunkApplyStatus, HunkLocation, MatchType};
        /// let status = HunkApplyStatus::Applied {
        ///     location: HunkLocation { start_index: 0, length: 2 },
        ///     match_type: MatchType::Exact,
        ///     replaced_lines: vec!["old line".to_string()],
        /// };
        /// match status {
        ///     HunkApplyStatus::Applied { match_type, .. } => assert!(matches!(match_type, MatchType::Exact)),
        ///     _ => unreachable!(),
        /// }
        /// ```
        match_type: MatchType,
        /// The original lines that were replaced by the hunk.
        ///
        /// # Examples
        ///
        /// ```
        /// use mpatch::{HunkApplyStatus, HunkLocation, MatchType};
        /// let status = HunkApplyStatus::Applied {
        ///     location: HunkLocation { start_index: 0, length: 2 },
        ///     match_type: MatchType::Exact,
        ///     replaced_lines: vec!["old line".to_string()],
        /// };
        /// match status {
        ///     HunkApplyStatus::Applied { replaced_lines, .. } => assert_eq!(replaced_lines.len(), 1),
        ///     _ => unreachable!(),
        /// }
        /// ```
        replaced_lines: Vec<String>,
    },
    /// The hunk was skipped because it contained no effective changes.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::HunkApplyStatus;
    /// let status = HunkApplyStatus::SkippedNoChanges;
    /// ```
    SkippedNoChanges,
    /// The hunk failed to apply for the specified reason.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{HunkApplyStatus, HunkApplyError};
    /// let status = HunkApplyStatus::Failed(HunkApplyError::ContextNotFound);
    /// ```
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
/// # Examples
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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ApplyOptions {
    /// If `true`, no files will be modified. Instead, a diff of the proposed
    /// changes will be generated and returned in [`PatchResult`].
    ///
    /// This is the primary way to preview the outcome of a patch operation without
    /// making any changes to the filesystem. When `dry_run` is enabled, functions
    /// like [`apply_patch_to_file()`] will populate the `diff` field of the
    /// returned [`PatchResult`].
    ///
    /// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::ApplyOptions;
    /// let options = ApplyOptions {
    ///     dry_run: false,
    ///     fuzz_factor: 0.85,
    /// };
    /// assert_eq!(options.fuzz_factor, 0.85);
    /// ```
    pub fuzz_factor: f32,
}

impl Default for ApplyOptions {
    /// Creates a new [`ApplyOptions`] instance with default values.
    ///
    /// This is the standard way to get a default configuration, which has `dry_run`
    /// set to `false` and `fuzz_factor` set to `0.7`.
    ///
    /// # Returns
    ///
    /// A new [`ApplyOptions`] instance with default values.
    ///
    /// # Examples
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
    /// This is an alias for [`ApplyOptions::default()`].
    ///
    /// # Returns
    ///
    /// A new [`ApplyOptions`] instance.
    ///
    /// # Examples
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
    /// # Returns
    ///
    /// A new [`ApplyOptions`] instance.
    ///
    /// # Examples
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
    /// # Returns
    ///
    /// A new [`ApplyOptions`] instance.
    ///
    /// # Examples
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
    /// # Arguments
    ///
    /// * `dry_run` - The boolean value to set.
    ///
    /// # Returns
    ///
    /// A new [`ApplyOptions`] instance with the updated setting.
    ///
    /// # Examples
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
    /// # Arguments
    ///
    /// * `fuzz_factor` - The float value to set.
    ///
    /// # Returns
    ///
    /// A new [`ApplyOptions`] instance with the updated setting.
    ///
    /// # Examples
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
    /// # Returns
    ///
    /// A new [`ApplyOptionsBuilder`] instance.
    ///
    /// # Examples
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

/// A builder for constructing an [`ApplyOptions`] configuration.
///
/// This provides a classic builder pattern for constructing an [`ApplyOptions`] struct,
/// which can be useful when the configuration is built conditionally or comes from
/// multiple sources.
///
/// # Examples
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
#[derive(Debug, Clone, Copy)]
pub struct ApplyOptionsBuilder {
    /// Configured dry-run flag, or `None` to fall back to the default setting.
    dry_run: Option<bool>,
    /// Configured fuzzy matching threshold, or `None` to fall back to the default setting.
    fuzz_factor: Option<f32>,
}

impl Default for ApplyOptionsBuilder {
    /// Creates a new, empty `ApplyOptionsBuilder`.
    ///
    /// All options are initially unset and will fall back to the defaults
    /// defined in [`ApplyOptions::default()`] when `build()` is called.
    ///
    /// # Returns
    ///
    /// A new, empty [`ApplyOptionsBuilder`] instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::ApplyOptionsBuilder;
    /// let builder = ApplyOptionsBuilder::default();
    /// let options = builder.build();
    /// assert_eq!(options.dry_run, false);
    /// ```
    fn default() -> Self {
        Self {
            dry_run: None,
            fuzz_factor: None,
        }
    }
}

impl ApplyOptionsBuilder {
    /// Sets the `dry_run` flag for the patch operation.
    ///
    /// If `true`, no files will be modified. Instead, a diff of the proposed
    /// changes will be generated and returned in [`PatchResult`]. This is useful
    /// for previewing changes before they are applied.
    ///
    /// # Arguments
    ///
    /// * `dry_run` - The boolean value to set.
    ///
    /// # Returns
    ///
    /// The updated [`ApplyOptionsBuilder`] instance.
    ///
    /// # Examples
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
    /// # Arguments
    ///
    /// * `fuzz_factor` - The float value to set.
    ///
    /// # Returns
    ///
    /// The updated [`ApplyOptionsBuilder`] instance.
    ///
    /// # Examples
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
    /// # Returns
    ///
    /// The finalized [`ApplyOptions`] instance.
    ///
    /// # Examples
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
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{PatchResult, ApplyResult};
    /// # let result = PatchResult { report: ApplyResult { hunk_results: vec![] }, diff: None };
    /// assert!(result.report.all_applied_cleanly());
    /// ```
    pub report: ApplyResult,
    /// The unified diff of the proposed changes. This is only populated
    /// when `dry_run` was set to `true` in [`ApplyOptions`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{PatchResult, ApplyResult};
    /// # let result = PatchResult { report: ApplyResult { hunk_results: vec![] }, diff: Some("--- a/file\n+++ b/file\n".to_string()) };
    /// if let Some(diff_text) = &result.diff {
    ///     println!("Proposed diff:\n{}", diff_text);
    /// }
    /// ```
    pub diff: Option<String>,
}

/// The result of an in-memory patch operation.
///
/// This struct is returned by functions like [`apply_patch_to_content()`] and
/// [`apply_patch_to_lines()`]. It contains the newly generated content as a string,
/// along with a detailed report of the outcome for each hunk.
///
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{InMemoryResult, ApplyResult};
    /// # let result = InMemoryResult { new_content: "new text\n".to_string(), report: ApplyResult { hunk_results: vec![] } };
    /// assert_eq!(result.new_content, "new text\n");
    /// ```
    pub new_content: String,
    /// Detailed results for each hunk within the patch operation.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{InMemoryResult, ApplyResult};
    /// # let result = InMemoryResult { new_content: String::new(), report: ApplyResult { hunk_results: vec![] } };
    /// assert!(result.report.all_applied_cleanly());
    /// ```
    pub report: ApplyResult,
}

/// Contains detailed results for each hunk within a patch operation.
///
/// This struct provides a granular report on the outcome of a patch application.
/// It is a key component of both [`PatchResult`] and [`InMemoryResult`]. You can
/// use its methods like [`all_applied_cleanly()`](ApplyResult::all_applied_cleanly) for a
/// high-level summary or [`failures()`](ApplyResult::failures) to inspect specific issues.
///
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{ApplyResult, HunkApplyStatus};
    /// # let report = ApplyResult { hunk_results: vec![HunkApplyStatus::SkippedNoChanges] };
    /// assert_eq!(report.hunk_results.len(), 1);
    /// ```
    pub hunk_results: Vec<HunkApplyStatus>,
}

/// Details about a hunk that failed to apply.
///
/// This struct is returned by [`ApplyResult::failures()`] and provides a convenient
/// way to inspect which hunk failed and for what reason.
///
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{HunkFailure, HunkApplyError};
    /// # let failure = HunkFailure { hunk_index: 1, reason: HunkApplyError::ContextNotFound };
    /// assert_eq!(failure.hunk_index, 1);
    /// ```
    pub hunk_index: usize,
    /// The reason for the failure.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{HunkFailure, HunkApplyError};
    /// # let failure = HunkFailure { hunk_index: 1, reason: HunkApplyError::ContextNotFound };
    /// assert!(matches!(failure.reason, HunkApplyError::ContextNotFound));
    /// ```
    pub reason: HunkApplyError,
}

impl ApplyResult {
    /// Checks if all hunks in the patch were applied successfully or skipped.
    ///
    /// Returns `false` if any hunk failed to apply.
    ///
    /// # Returns
    ///
    /// `true` if all hunks applied cleanly, `false` otherwise.
    ///
    /// # Examples
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

    /// Checks if any hunk in the patch failed to apply.
    ///
    /// This is the logical opposite of [`all_applied_cleanly`](ApplyResult::all_applied_cleanly).
    ///
    /// # Returns
    ///
    /// `true` if any hunk failed to apply, `false` otherwise.
    ///
    /// # Examples
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
        self.hunk_results
            .iter()
            .any(|r| matches!(r, HunkApplyStatus::Failed(_)))
    }

    /// Returns the number of hunks that failed to apply.
    ///
    /// This is a convenience method that counts how many hunks in the `hunk_results`
    /// list have a status of [`HunkApplyStatus::Failed`].
    ///
    /// # Returns
    ///
    /// The number of failed hunks.
    ///
    /// # Examples
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
        self.hunk_results
            .iter()
            .filter(|r| matches!(r, HunkApplyStatus::Failed(_)))
            .count()
    }

    /// Returns the number of hunks that were applied successfully or skipped.
    ///
    /// This method counts how many hunks in the `hunk_results` list have a status
    /// of either [`HunkApplyStatus::Applied`] or [`HunkApplyStatus::SkippedNoChanges`].
    ///
    /// # Returns
    ///
    /// The number of successful or skipped hunks.
    ///
    /// # Examples
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

    /// Returns a list of all hunks that failed to apply, along with their index.
    ///
    /// This provides a more convenient way to inspect failures than iterating
    /// through [`hunk_results`](ApplyResult::hunk_results) manually.
    ///
    /// # Returns
    ///
    /// A vector of [`HunkFailure`] objects representing the failed hunks.
    ///
    /// # Examples
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
/// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::BatchResult;
    /// # let batch = BatchResult { results: vec![] };
    /// assert!(batch.results.is_empty());
    /// ```
    pub results: Vec<(PathBuf, Result<PatchResult, PatchError>)>,
}

impl BatchResult {
    /// Checks if all patches in the batch were applied without "hard" errors (like I/O errors).
    /// This does *not* check if all hunks were applied cleanly. For that, you must
    /// inspect the individual `PatchResult` objects.
    ///
    /// # Returns
    ///
    /// `true` if all patches succeeded without hard errors, `false` otherwise.
    ///
    /// # Examples
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
    /// # Returns
    ///
    /// A vector of tuples containing the file path and the associated [`PatchError`].
    ///
    /// # Examples
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

    /// Checks if all patches in the batch succeeded without hard errors AND all hunks in every
    /// patch applied cleanly.
    ///
    /// # Returns
    ///
    /// `true` if every patch succeeded and every hunk in every patch applied cleanly, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{parse_auto, apply_patches_to_dir_atomic, ApplyOptions};
    /// # use tempfile::tempdir;
    /// # use std::fs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let dir = tempdir()?;
    /// let file_path = dir.path().join("file.txt");
    /// fs::write(&file_path, "old\n")?;
    /// let diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
    /// let patches = parse_auto(diff)?;
    ///
    /// let batch = apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::exact());
    /// assert!(batch.all_applied_cleanly());
    /// # Ok(())
    /// # }
    /// ```
    pub fn all_applied_cleanly(&self) -> bool {
        self.results
            .iter()
            .all(|(_, res)| res.as_ref().is_ok_and(|p| p.report.all_applied_cleanly()))
    }

    /// Checks if any patch in the batch had a hard error or any hunk failed to apply.
    ///
    /// # Returns
    ///
    /// `true` if any patch had a hard error or any hunk failed to apply, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{parse_auto, apply_patches_to_dir_atomic, ApplyOptions};
    /// # use tempfile::tempdir;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let dir = tempdir()?;
    /// let diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-missing\n+new\n";
    /// let patches = parse_auto(diff)?;
    ///
    /// let batch = apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::exact());
    /// assert!(batch.has_failures());
    /// # Ok(())
    /// # }
    /// ```
    pub fn has_failures(&self) -> bool {
        !self.all_applied_cleanly()
    }
}

// --- Data Structures ---

/// Represents a single hunk of changes within a patch.
///
/// Structurally, this models a hunk from a Unified Diff (the `@@ ... @@` blocks),
/// storing lines prefixed with `+`, `-`, or space. However, it serves as the
/// universal internal representation for all patch formats in `mpatch`. This
/// abstraction allows the matching engine to operate identically regardless
/// of whether the input was a formal `.patch` file or a snippet of
/// conflict markers.
///
/// - **Unified Diffs:** Parsed directly.
/// - **Conflict Markers:** Converted into a `Hunk` where the "old" block becomes
///   deletions and the "new" block becomes additions.
///
/// You typically get `Hunk` objects as part of a [`Patch`] after parsing a diff.
///
/// # Examples
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
    /// When parsing Conflict Markers, `mpatch` synthesizes these lines: the "before"
    /// block becomes `-` lines, and the "after" block becomes `+` lines.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, Hunk};
    /// # let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n-a\n+b\n```";
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
    pub lines: Vec<String>,
    /// The starting line number in the original file (1-based).
    ///
    /// This corresponds to the `l` in the `@@ -l,s ...` header of a unified diff.
    /// In `mpatch`, this value is primarily used as a **hint** to resolve ambiguity.
    /// If the context matches in multiple places, the location closest to this line
    /// is chosen.
    ///
    /// This may be `None` if the patch source (like Conflict Markers) did not provide line numbers.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{Hunk};
    /// let hunk = Hunk {
    ///     lines: vec!["-old".to_string()],
    ///     old_start_line: Some(10), // Hint: look near line 10
    ///     new_start_line: Some(10),
    /// };
    /// ```
    pub old_start_line: Option<usize>,
    /// The starting line number in the new file (1-based).
    ///
    /// This corresponds to the `l` in the `@@ ... +l,s @@` header of a unified diff.
    /// It represents the intended location in the resulting file.
    ///
    /// This may be `None` if the patch source did not provide line numbers.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{Hunk};
    /// let hunk = Hunk {
    ///     lines: vec!["+new".to_string()],
    ///     old_start_line: Some(10),
    ///     new_start_line: Some(12), // Lines shifted down by 2
    /// };
    /// ```
    pub new_start_line: Option<usize>,
}

impl Hunk {
    /// Creates a new `Hunk` that reverses the changes in this one.
    ///
    /// Additions become deletions, and deletions become additions. Context lines
    /// remain unchanged. The old and new line number hints are swapped.
    ///
    /// # Returns
    ///
    /// A new [`Hunk`] with additions and deletions swapped.
    ///
    /// # Examples
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
        trace!(
            "Hunk::invert: inverting hunk with {} line(s) (old_start={:?}, new_start={:?})",
            self.lines.len(),
            self.old_start_line,
            self.new_start_line
        );
        let inverted_lines = self
            .lines
            .iter()
            .map(|line| {
                if let Some(stripped) = line.strip_prefix('+') {
                    let mut s = String::with_capacity(line.len());
                    s.push('-');
                    s.push_str(stripped);
                    s
                } else if let Some(stripped) = line.strip_prefix('-') {
                    let mut s = String::with_capacity(line.len());
                    s.push('+');
                    s.push_str(stripped);
                    s
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
    /// # Returns
    ///
    /// A vector of string slices representing the match block.
    ///
    /// # Examples
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
        let block: Vec<&str> = self
            .lines
            .iter()
            .filter(|l| !l.starts_with('+'))
            .map(|l| &l[1..])
            .collect();
        trace!(
            "Hunk::get_match_block: extracted {} match line(s)",
            block.len()
        );
        block
    }

    /// Extracts the lines that will replace the matched block in the target file.
    ///
    /// This includes context lines (starting with ' ') and addition lines
    /// (starting with '+'). The leading character is stripped. This is the
    /// content that will be "spliced" into the file.
    ///
    /// # Returns
    ///
    /// A vector of string slices representing the replace block.
    ///
    /// # Examples
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
        let block: Vec<&str> = self
            .lines
            .iter()
            .filter(|l| !l.starts_with('-'))
            .map(|l| &l[1..])
            .collect();
        trace!(
            "Hunk::get_replace_block: extracted {} replacement line(s)",
            block.len()
        );
        block
    }

    /// Extracts the context lines from the hunk.
    ///
    /// These are lines that start with ' ' and are stripped of the prefix.
    ///
    /// # Returns
    ///
    /// A vector of string slices representing the context lines.
    ///
    /// # Examples
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
        let lines: Vec<&str> = self
            .lines
            .iter()
            .filter(|l| l.starts_with(' '))
            .map(|l| &l[1..])
            .collect();
        trace!(
            "Hunk::context_lines: extracted {} context line(s)",
            lines.len()
        );
        lines
    }

    /// Extracts the added lines from the hunk.
    ///
    /// These are lines that start with '+' and are stripped of the prefix.
    ///
    /// # Returns
    ///
    /// A vector of string slices representing the added lines.
    ///
    /// # Examples
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
        let lines: Vec<&str> = self
            .lines
            .iter()
            .filter(|l| l.starts_with('+'))
            .map(|l| &l[1..])
            .collect();
        trace!(
            "Hunk::added_lines: extracted {} addition line(s)",
            lines.len()
        );
        lines
    }

    /// Extracts the removed lines from the hunk.
    ///
    /// These are lines that start with '-' and are stripped of the prefix.
    ///
    /// # Returns
    ///
    /// A vector of string slices representing the removed lines.
    ///
    /// # Examples
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
        let lines: Vec<&str> = self
            .lines
            .iter()
            .filter(|l| l.starts_with('-'))
            .map(|l| &l[1..])
            .collect();
        trace!(
            "Hunk::removed_lines: extracted {} removal line(s)",
            lines.len()
        );
        lines
    }

    /// Checks if the hunk contains any effective changes (additions or deletions).
    ///
    /// A hunk with only context lines has no changes and can be skipped.
    ///
    /// # Returns
    ///
    /// `true` if the hunk has additions or deletions, `false` otherwise.
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
        let changed = self.lines.iter().any(|l| l.starts_with(['+', '-']));
        trace!("Hunk::has_changes: {}", changed);
        changed
    }

    /// Returns the minimum span (in lines of `match_block`) between the first and
    /// last edit site (addition or removal) in this hunk.
    ///
    /// This calculates the distance across the hunk's match block between the earliest
    /// modification (a removed line or the anchor preceding an added line) and the latest
    /// modification. Target file windows shorter than this span cannot possibly contain
    /// all edits in the hunk and are pruned during candidate search.
    ///
    /// # Returns
    ///
    /// The span length in match block lines, or `0` if the hunk contains no changes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk = Hunk {
    ///     lines: vec![
    ///         " ctx0".to_string(),
    ///         "-del0".to_string(),
    ///         " ctx1".to_string(),
    ///         "+add1".to_string(),
    ///         " ctx2".to_string(),
    ///     ],
    ///     old_start_line: Some(1),
    ///     new_start_line: Some(1),
    /// };
    /// assert_eq!(hunk.required_match_span(), 2);
    /// ```
    pub fn required_match_span(&self) -> usize {
        if !self.lines.iter().any(|l| !l.starts_with('+')) {
            return 0;
        }

        let mut first_match_idx = None;
        let mut last_match_idx = None;
        let mut current_match_idx: usize = 0;

        for line in &self.lines {
            if line.starts_with('+') {
                let anchor_idx = current_match_idx.saturating_sub(1);
                first_match_idx.get_or_insert(anchor_idx);
                last_match_idx = Some(anchor_idx);
            } else {
                if line.starts_with('-') {
                    first_match_idx.get_or_insert(current_match_idx);
                    last_match_idx = Some(current_match_idx);
                }
                current_match_idx += 1;
            }
        }

        let span = match (first_match_idx, last_match_idx) {
            (Some(first), Some(last)) => last.saturating_sub(first) + 1,
            _ => 0,
        };
        trace!(
            "Hunk::required_match_span: calculated required match span as {} line(s) (first_match_idx={:?}, last_match_idx={:?})",
            span,
            first_match_idx,
            last_match_idx
        );
        span
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
    /// # Arguments
    ///
    /// * `f` - The formatter to write the output to.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the formatting was successful.
    ///
    /// # Errors
    ///
    /// Returns `Err(std::fmt::Error)` if writing to the formatter fails.
    ///
    /// # Examples
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
        let (old_len, new_len) = self.lines.iter().fold((0, 0), |(old, new), line| {
            if line.starts_with('+') {
                (old, new + 1)
            } else if line.starts_with('-') {
                (old + 1, new)
            } else {
                (old + 1, new + 1)
            }
        });
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
/// # Examples
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
    /// # Examples
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
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::HunkLocation;
    /// let location = HunkLocation { start_index: 0, length: 5 };
    /// assert_eq!(location.length, 5);
    /// ```
    pub length: usize,
}

impl std::fmt::Display for HunkLocation {
    /// Formats the location for display, showing a user-friendly 1-based line number.
    ///
    /// This implementation provides a more intuitive, human-readable representation of the
    /// hunk's location. It converts the internal 0-based `start_index` into a 1-based
    /// line number (e.g., index `9` becomes `"line 10"`), which is the standard
    /// convention in text editors and log messages. This makes it easy to use
    /// `HunkLocation` directly in formatted strings for clear diagnostic output.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write the output to.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the formatting was successful.
    ///
    /// # Errors
    ///
    /// Returns `Err(std::fmt::Error)` if writing to the formatter fails.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::HunkLocation;
    /// let location = HunkLocation { start_index: 9, length: 3 };
    /// assert_eq!(location.to_string(), "line 10");
    /// assert_eq!(format!("Hunk applied at {}", location), "Hunk applied at line 10");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Adding 1 to start_index for a more user-friendly 1-based line number.
        write!(f, "line {}", self.start_index + 1)
    }
}

/// Represents all the changes to be applied to a single file.
///
/// A `Patch` contains a target file path and a list of [`Hunk`]s. It is typically
/// created by parsing a diff string (Unified Diff, Markdown block, or Conflict Markers)
/// using functions like [`parse_auto()`] or [`parse_diffs()`]. It can represent
/// file modifications, creations, or deletions.
///
/// It is the primary unit of work for the `apply` functions.
///
/// # Examples
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
    /// # Examples
    ///
    /// ```
    /// # use mpatch::parse_single_patch;
    /// # let diff = "```diff\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,1 +1,1 @@\n-a\n+b\n```";
    /// let patch = parse_single_patch(diff).unwrap();
    ///
    /// assert_eq!(patch.file_path.to_str(), Some("src/main.rs"));
    /// println!("Patch targets the file: {}", patch.file_path.display());
    /// ```
    pub file_path: PathBuf,
    /// A list of hunks to be applied to the file.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::Patch;
    /// # let patch = Patch { file_path: std::path::PathBuf::from("f"), hunks: vec![], ends_with_newline: true };
    /// assert!(patch.hunks.is_empty());
    /// ```
    pub hunks: Vec<Hunk>,
    /// Indicates whether the file should end with a newline.
    /// This is determined by the presence of `\ No newline at end of file`
    /// in the diff.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::Patch;
    /// # let patch = Patch { file_path: std::path::PathBuf::from("f"), hunks: vec![], ends_with_newline: false };
    /// assert_eq!(patch.ends_with_newline, false);
    /// ```
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
    /// # Returns
    ///
    /// A new [`Patch`] object.
    ///
    /// # Errors
    ///
    /// Returns `Err(`[`ParseError`]`)` if the diff cannot be parsed.
    ///
    /// # Examples
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
        debug!(
            "Patch::from_texts: generating unified diff for '{}' (old len: {} bytes, new len: {} bytes, context: {})",
            path.display(),
            old_text.len(),
            new_text.len(),
            context_len
        );
        let diff = TextDiff::configure()
            .algorithm(similar::Algorithm::Patience)
            .diff_lines(old_text, new_text);
        let mut hunks = Vec::new();
        let push_prefixed =
            |lines: &mut Vec<String>, prefix: char, is_old: bool, start: usize, len: usize| {
                for i in 0..len {
                    let slice = if is_old {
                        diff.old_slice(start + i).unwrap_or("")
                    } else {
                        diff.new_slice(start + i).unwrap_or("")
                    };
                    let line = slice.trim_end_matches(['\r', '\n']);
                    let mut s = String::with_capacity(line.len() + 1);
                    s.push(prefix);
                    s.push_str(line);
                    lines.push(s);
                }
            };

        for group in diff.grouped_ops(context_len) {
            let mut lines = Vec::new();
            let mut old_start = None;
            let mut new_start = None;

            if let Some(first_op) = group.first() {
                old_start = Some(first_op.old_range().start + 1);
                new_start = Some(first_op.new_range().start + 1);
            }

            for op in group {
                match op {
                    similar::DiffOp::Equal { old_index, len, .. } => {
                        push_prefixed(&mut lines, ' ', true, old_index, len);
                    }
                    similar::DiffOp::Delete {
                        old_index, old_len, ..
                    } => {
                        push_prefixed(&mut lines, '-', true, old_index, old_len);
                    }
                    similar::DiffOp::Insert {
                        new_index, new_len, ..
                    } => {
                        push_prefixed(&mut lines, '+', false, new_index, new_len);
                    }
                    similar::DiffOp::Replace {
                        old_index,
                        old_len,
                        new_index,
                        new_len,
                    } => {
                        push_prefixed(&mut lines, '-', true, old_index, old_len);
                        push_prefixed(&mut lines, '+', false, new_index, new_len);
                    }
                }
            }

            hunks.push(Hunk {
                lines,
                old_start_line: old_start,
                new_start_line: new_start,
            });
        }

        debug!(
            "Patch::from_texts: produced {} hunk(s) for '{}' (ends_with_newline={})",
            hunks.len(),
            path.display(),
            new_text.ends_with('\n') || new_text.is_empty()
        );
        Ok(Patch {
            file_path: path,
            hunks,
            ends_with_newline: new_text.ends_with('\n') || new_text.is_empty(),
        })
    }

    /// Creates a new `Patch` that reverses the changes in this one.
    ///
    /// Each hunk in the patch is inverted, swapping additions and deletions.
    /// This is useful for "un-applying" a patch.
    ///
    /// **Note:** The `ends_with_newline` status of the reversed patch is ambiguous
    /// in the unified diff format, so it defaults to `true`.
    ///
    /// # Returns
    ///
    /// A new [`Patch`] object with all hunks inverted.
    ///
    /// # Examples
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
        debug!(
            "Patch::invert: inverting patch for '{}' with {} hunk(s)",
            self.file_path.display(),
            self.hunks.len()
        );
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
    /// # Returns
    ///
    /// `true` if the patch represents a file creation, `false` otherwise.
    ///
    /// # Examples
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
        let is_create = self.hunks.first().is_some_and(|h| {
            h.old_start_line == Some(0)
                || (h.old_start_line.is_none() && h.get_match_block().is_empty())
        });
        trace!(
            "Patch::is_creation for '{}': {} (first hunk old_start_line={:?})",
            self.file_path.display(),
            is_create,
            self.hunks.first().and_then(|h| h.old_start_line)
        );
        is_create
    }

    /// Checks if the patch represents a full file deletion.
    ///
    /// A patch is considered a deletion if it contains at least one hunk, and
    /// all of its hunks result in removing content without adding any new content
    /// (i.e., their "replace blocks" are empty). This is typical for a diff
    /// that empties a file.
    ///
    /// # Returns
    ///
    /// `true` if the patch represents a file deletion, `false` otherwise.
    ///
    /// # Examples
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
        let is_delete = !self.hunks.is_empty()
            && self
                .hunks
                .iter()
                .all(|h| h.new_start_line == Some(0) || h.get_replace_block().is_empty());
        trace!(
            "Patch::is_deletion for '{}': {} (hunk count: {})",
            self.file_path.display(),
            is_delete,
            self.hunks.len()
        );
        is_delete
    }

    /// Applies this patch to a file on disk.
    ///
    /// This is an associated method equivalent to [`apply_patch_to_file()`].
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The base directory where the patch should be applied.
    /// * `options` - Configuration for the patch operation.
    ///
    /// # Returns
    ///
    /// A [`PatchResult`] on success.
    ///
    /// # Errors
    ///
    /// Returns `Err(`[`PatchError`]`)` on hard errors like I/O failures or path traversal.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, ApplyOptions};
    /// # use tempfile::tempdir;
    /// # use std::fs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let dir = tempdir()?;
    /// let file_path = dir.path().join("hello.txt");
    /// fs::write(&file_path, "Hello, world!\n")?;
    ///
    /// let diff = "--- a/hello.txt\n+++ b/hello.txt\n@@ -1 +1 @@\n-Hello, world!\n+Hello, mpatch!\n";
    /// let patch = parse_single_patch(diff)?;
    /// let result = patch.apply_to_file(dir.path(), ApplyOptions::exact())?;
    ///
    /// assert!(result.report.all_applied_cleanly());
    /// assert_eq!(fs::read_to_string(&file_path)?, "Hello, mpatch!\n");
    /// # Ok(())
    /// # }
    /// ```
    pub fn apply_to_file(
        &self,
        target_dir: &Path,
        options: ApplyOptions,
    ) -> Result<PatchResult, PatchError> {
        debug!(
            "Patch::apply_to_file: applying patch for '{}' to directory '{}'",
            self.file_path.display(),
            target_dir.display()
        );
        apply_patch_to_file(self, target_dir, options)
    }

    /// Applies this patch to a file on disk atomically.
    ///
    /// Changes are written to disk if and only if all hunks in this patch apply cleanly.
    /// If any hunk fails or encounters an error, the file on disk remains completely unmodified.
    ///
    /// # Arguments
    ///
    /// * `target_dir` - The base directory where the patch should be applied.
    /// * `options` - Configuration for the patch operation.
    ///
    /// # Returns
    ///
    /// A [`PatchResult`] on success.
    ///
    /// # Errors
    ///
    /// Returns `Err(`[`PatchError`]`)` on hard errors like I/O failures or path traversal.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, ApplyOptions};
    /// # use tempfile::tempdir;
    /// # use std::fs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let dir = tempdir()?;
    /// let file_path = dir.path().join("app.txt");
    /// fs::write(&file_path, "line 1\nline 2\n")?;
    ///
    /// let diff = "--- a/app.txt\n+++ b/app.txt\n@@ -1,2 +1,2 @@\n line 1\n-line 2\n+line two\n";
    /// let patch = parse_single_patch(diff)?;
    /// let result = patch.apply_to_file_atomic(dir.path(), ApplyOptions::exact())?;
    ///
    /// assert!(result.report.all_applied_cleanly());
    /// assert_eq!(fs::read_to_string(&file_path)?, "line 1\nline two\n");
    /// # Ok(())
    /// # }
    /// ```
    pub fn apply_to_file_atomic(
        &self,
        target_dir: &Path,
        options: ApplyOptions,
    ) -> Result<PatchResult, PatchError> {
        debug!(
            "Patch::apply_to_file_atomic: applying patch for '{}' atomically to directory '{}'",
            self.file_path.display(),
            target_dir.display()
        );
        apply_patch_to_file_atomic(self, target_dir, options)
    }

    /// Applies this patch to string content in memory.
    ///
    /// This is an associated method equivalent to [`apply_patch_to_content()`].
    ///
    /// # Arguments
    ///
    /// * `original_content` - Optional string slice of the content to patch.
    /// * `options` - Configuration for the patch operation.
    ///
    /// # Returns
    ///
    /// An [`InMemoryResult`] containing the new content and report.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let diff = "--- a/greeting.txt\n+++ b/greeting.txt\n@@ -1 +1 @@\n-Hello\n+Hi\n";
    /// let patch = parse_single_patch(diff)?;
    /// let options = ApplyOptions::exact();
    /// let result = patch.apply_to_content(Some("Hello\n"), &options);
    ///
    /// assert!(result.report.all_applied_cleanly());
    /// assert_eq!(result.new_content, "Hi\n");
    /// # Ok(())
    /// # }
    /// ```
    pub fn apply_to_content(
        &self,
        original_content: Option<&str>,
        options: &ApplyOptions,
    ) -> InMemoryResult {
        debug!(
            "Patch::apply_to_content: applying patch for '{}' to content in-memory",
            self.file_path.display()
        );
        apply_patch_to_content(self, original_content, options)
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
    /// # Arguments
    ///
    /// * `f` - The formatter to write the output to.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the formatting was successful.
    ///
    /// # Errors
    ///
    /// Returns `Err(std::fmt::Error)` if writing to the formatter fails.
    ///
    /// # Examples
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
///
/// # Examples
///
/// ```
/// use mpatch::{detect_patch, PatchFormat};
///
/// let content = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-a\n+b";
/// assert_eq!(detect_patch(content), PatchFormat::Unified);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PatchFormat {
    /// A standard Unified Diff format.
    ///
    /// This format is characterized by file headers starting with `---` and `+++`,
    /// or hunk headers starting with `@@`.
    ///
    /// # Examples
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
    /// # Examples
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
    /// # Examples
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

    /// An Aider search/replace block format (`<<<<<<< SEARCH` / `ORIGINAL`, `=======`, `>>>>>>> REPLACE` / `UPDATED`).
    ///
    /// This format is used by Aider and LLM coding assistants. It specifies the target
    /// file and pairs an exact search block with a replacement block.
    ///
    /// # Examples
    /// ```text
    /// path/to/file.rs
    /// <<<<<<< SEARCH
    /// fn old() {}
    /// =======
    /// fn new() {}
    /// >>>>>>> REPLACE
    /// ```
    Aider,

    /// The format could not be determined.
    ///
    /// The content did not contain any recognizable signatures (such as diff headers,
    /// markdown fences, or conflict markers).
    ///
    /// # Examples
    ///
    /// ```
    /// use mpatch::{detect_patch, PatchFormat};
    /// let content = "Just some random text.";
    /// assert_eq!(detect_patch(content), PatchFormat::Unknown);
    /// ```
    Unknown,
}

/// Counts the number of leading consecutive backticks (```) in a string slice.
///
/// Used to determine opening and closing Markdown fence lengths when handling
/// variable-length code blocks and nested code fences.
///
/// # Arguments
///
/// * `s` - The string slice whose leading backticks to count.
///
/// # Returns
///
/// The number of consecutive backtick characters at the start of `s`.
#[inline]
fn count_leading_backticks(s: &str) -> usize {
    s.as_bytes().iter().take_while(|&&c| c == b'`').count()
}

/// Checks whether a line represents an Aider search/original fence (e.g. `<<<<<<< SEARCH`, `<<<<<<< ORIGINAL`).
///
/// # Arguments
///
/// * `trimmed` - A string slice with leading whitespace trimmed.
///
/// # Returns
///
/// `true` if `trimmed` begins with `<<<<` and an accepted search keyword, `false` otherwise.
#[inline]
fn is_aider_search_fence(trimmed: &str) -> bool {
    if trimmed.starts_with("<<<<") {
        let after = trimmed.trim_start_matches('<').trim_start();
        let upper = after.to_ascii_uppercase();
        let is_match = upper.starts_with("SEARCH")
            || upper.starts_with("ORIGINAL")
            || upper.starts_with("BEFORE")
            || upper.starts_with("OLD")
            || upper.starts_with("CURRENT")
            || upper.starts_with("SOURCE");
        if is_match {
            trace!("is_aider_search_fence: matched search fence: '{}'", trimmed);
        }
        is_match
    } else {
        false
    }
}

/// Extracts an optional file path from an Aider search fence line (e.g. `<<<<<<< SEARCH path/to/file` or `<<<<<<< ORIGINAL path/to/file`).
///
/// # Arguments
///
/// * `trimmed` - A string slice with leading whitespace trimmed.
///
/// # Returns
///
/// `Some(PathBuf)` containing the normalized path if present on the fence line, or `None`.
fn extract_file_path_from_search_fence(trimmed: &str) -> Option<PathBuf> {
    if trimmed.starts_with("<<<<") {
        let after = trimmed.trim_start_matches('<').trim_start();
        let upper = after.to_ascii_uppercase();
        for keyword in &["SEARCH", "ORIGINAL", "BEFORE", "OLD", "CURRENT", "SOURCE"] {
            if upper.starts_with(keyword) {
                let rest = after[keyword.len()..].trim();
                if !rest.is_empty() {
                    let path = extract_file_path_from_line(rest);
                    debug!(
                        "extract_file_path_from_search_fence: extracted path {:?} from search fence line '{}'",
                        path, trimmed
                    );
                    return path;
                }
                return None;
            }
        }
    }
    None
}

/// Checks whether a line represents an Aider dividing fence (`=======`).
///
/// # Arguments
///
/// * `trimmed` - A string slice with leading whitespace trimmed.
///
/// # Returns
///
/// `true` if `trimmed` begins with `====`, `false` otherwise.
#[inline]
fn is_aider_divide_fence(trimmed: &str) -> bool {
    if trimmed.starts_with("====") {
        let after = trimmed.trim_start_matches('=').trim();
        let is_match = after.is_empty()
            || after.to_ascii_uppercase().starts_with("DIVIDE")
            || after.to_ascii_uppercase().starts_with("SPLIT");
        if is_match {
            trace!("is_aider_divide_fence: matched divide fence: '{}'", trimmed);
        }
        is_match
    } else {
        false
    }
}

/// Checks whether a line represents an Aider replace/updated fence (e.g. `>>>>>>> REPLACE`, `>>>>>>> UPDATED`).
///
/// # Arguments
///
/// * `trimmed` - A string slice with leading whitespace trimmed.
///
/// # Returns
///
/// `true` if `trimmed` begins with `>>>>` and an accepted replace keyword, `false` otherwise.
#[inline]
fn is_aider_replace_fence(trimmed: &str) -> bool {
    if trimmed.starts_with(">>>>") {
        let after = trimmed.trim_start_matches('>').trim_start();
        if after.is_empty() {
            trace!(
                "is_aider_replace_fence: matched bare replace fence: '{}'",
                trimmed
            );
            return true;
        }
        let upper = after.to_ascii_uppercase();
        let is_match = upper.starts_with("REPLACE")
            || upper.starts_with("UPDATED")
            || upper.starts_with("AFTER")
            || upper.starts_with("NEW")
            || upper.starts_with("MODIFIED")
            || upper.starts_with("FINAL")
            || upper.starts_with("PROPOSED");
        if is_match {
            trace!(
                "is_aider_replace_fence: matched replace fence: '{}'",
                trimmed
            );
        }
        is_match
    } else {
        false
    }
}

/// Determines whether a line represents an ellipsis / wildcard indicating omitted code.
///
/// This function identifies omitted code markers across a wide variety of comment and syntax
/// conventions (such as `...`, `…`, `// ... existing code ...`, `# ... rest of function ...`,
/// `<!-- ... -->`, `/* ... */`, `[...]`, etc.) while strictly rejecting valid code constructs
/// that happen to contain dots or variadic syntax (such as JS/TS object spread `{ ...props }`,
/// Python array slicing `tensor[..., 0]`, C variadics `printf(fmt, ...)`, and conversational text).
///
/// # Arguments
///
/// * `line` - A string slice containing the single line of text to evaluate.
///
/// # Returns
/// `true` if the line represents an ellipsis or omitted code wildcard, `false` otherwise.
///
/// # Examples
///
/// ```
/// use mpatch::is_ellipsis_line;
///
/// assert!(is_ellipsis_line("..."));
/// assert!(is_ellipsis_line("    // ... existing code ..."));
/// assert!(is_ellipsis_line("# ... rest of function ..."));
/// assert!(is_ellipsis_line("<!-- ... -->"));
/// assert!(is_ellipsis_line("[...]"));
///
/// // Code constructs using spread, slicing, or variadics are not ellipsis lines:
/// assert!(!is_ellipsis_line("const copy = [...items];"));
/// assert!(!is_ellipsis_line("let x = tensor[..., 0];"));
/// assert!(!is_ellipsis_line("fn log(...args: any[]) {}"));
/// ```
pub fn is_ellipsis_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    // 1. Bare dots or unicode ellipsis
    if trimmed == "..." || trimmed == "…" || trimmed == "...." || trimmed == "....." {
        trace!("is_ellipsis_line: matched bare dots/ellipsis '{}'", trimmed);
        return true;
    }

    // 2. Pure repetition of dots, unicode ellipses, dashes, or tildes
    if (trimmed.len() >= 3 || trimmed.chars().count() >= 2)
        && trimmed
            .chars()
            .all(|c| c == '.' || c == '…' || c == '-' || c == '~')
    {
        trace!(
            "is_ellipsis_line: matched repeated ellipsis characters '{}'",
            trimmed
        );
        return true;
    }

    // Must contain an ellipsis ('...' or '…') to be considered an ellipsis line.
    if !trimmed.contains("...") && !trimmed.contains('…') {
        return false;
    }

    // 3. Bare bracketed ellipses: [...], [ ... ], (...), ( ... ), {...}, { ... }, <...>, < ... >,
    // or unicode variations […], (…), {…}, <…>
    if (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.starts_with('(') && trimmed.ends_with(')'))
        || (trimmed.starts_with('{') && trimmed.ends_with('}') && !trimmed.starts_with("{/*"))
        || (trimmed.starts_with('<') && trimmed.ends_with('>') && !trimmed.starts_with("<!--"))
    {
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if inner == "..." || inner == "…" || inner == "...." || inner == "....." {
            trace!("is_ellipsis_line: matched bracketed ellipsis '{}'", trimmed);
            return true;
        }
        if !inner.is_empty()
            && inner
                .chars()
                .all(|c| c == '.' || c == '…' || c == '-' || c == '~')
        {
            trace!(
                "is_ellipsis_line: matched bracketed repeated ellipsis characters '{}'",
                trimmed
            );
            return true;
        }
    }

    // 4. Extract comment body and detect comment type
    let mut candidate = trimmed;
    let mut is_comment = false;

    if let Some(rest) = candidate
        .strip_prefix("{/*")
        .and_then(|s| s.strip_suffix("*/}"))
    {
        candidate = rest.trim();
        is_comment = true;
    } else if let Some(rest) = candidate
        .strip_prefix("(*")
        .and_then(|s| s.strip_suffix("*)"))
    {
        candidate = rest.trim();
        is_comment = true;
    } else if let Some(rest) = candidate
        .strip_prefix("<!--")
        .and_then(|s| s.strip_suffix("-->"))
    {
        candidate = rest.trim();
        is_comment = true;
    } else if let Some(rest) = candidate
        .strip_prefix("/*")
        .and_then(|s| s.strip_suffix("*/"))
    {
        candidate = rest.trim_matches('*').trim();
        is_comment = true;
    } else if let Some(rest) = candidate
        .strip_prefix("'''")
        .and_then(|s| s.strip_suffix("'''"))
    {
        candidate = rest.trim();
        is_comment = true;
    } else if let Some(rest) = candidate
        .strip_prefix("\"\"\"")
        .and_then(|s| s.strip_suffix("\"\"\""))
    {
        candidate = rest.trim();
        is_comment = true;
    } else if let Some(rest) = candidate.strip_prefix("//") {
        candidate = rest.trim_start_matches('/').trim();
        is_comment = true;
    } else if let Some(rest) = candidate.strip_prefix('#') {
        candidate = rest.trim_start_matches('#').trim();
        is_comment = true;
    } else if let Some(rest) = candidate.strip_prefix("--") {
        candidate = rest.trim_start_matches('-').trim();
        is_comment = true;
    } else if let Some(rest) = candidate.strip_prefix(';') {
        candidate = rest.trim_start_matches(';').trim();
        is_comment = true;
    } else if let Some(rest) = candidate.strip_prefix('%') {
        candidate = rest.trim_start_matches('%').trim();
        is_comment = true;
    } else if candidate.to_ascii_lowercase().starts_with("rem ") {
        candidate = candidate[4..].trim();
        is_comment = true;
    }

    // 5. Code syntax rejection:
    // If not a comment, lines containing code syntax operators/delimiters are code, not ellipses.
    if !is_comment {
        if trimmed.contains([';', '=', '{', '}', '(', ')', '"', '\'']) {
            return false;
        }
        if trimmed.starts_with('<') {
            return false;
        }
    }

    // 6. Strip decorative banner/box characters from both ends
    let inner_unbannered = candidate
        .trim_matches(|c: char| {
            c == '='
                || c == '-'
                || c == '*'
                || c == '~'
                || c == '>'
                || c == '<'
                || c == '#'
                || c == '/'
        })
        .trim();

    if inner_unbannered == "..." || inner_unbannered == "…" || inner_unbannered == "...." {
        trace!(
            "is_ellipsis_line: matched unbannered comment ellipsis '{}'",
            trimmed
        );
        return true;
    }
    if !inner_unbannered.is_empty()
        && inner_unbannered
            .chars()
            .all(|c| c == '.' || c == '…' || c == '-' || c == '~')
    {
        trace!(
            "is_ellipsis_line: matched unbannered repeated ellipsis characters '{}'",
            trimmed
        );
        return true;
    }

    // Strip optional surrounding brackets inside comment, e.g. `(unchanged code)`, `[existing logic]`
    let text_to_check = if (inner_unbannered.starts_with('(') && inner_unbannered.ends_with(')'))
        || (inner_unbannered.starts_with('[') && inner_unbannered.ends_with(']'))
        || (inner_unbannered.starts_with('{') && inner_unbannered.ends_with('}'))
    {
        inner_unbannered[1..inner_unbannered.len() - 1].trim()
    } else {
        inner_unbannered
    };

    // 7. Check for spread/rest code syntax: e.g. `...args`, `...numbers`, `...rest`
    if let Some(pos) = text_to_check.find("...") {
        let after = &text_to_check[pos + 3..];
        if after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$') {
            return false;
        }
    }

    // 8. Word-based ellipsis verification
    let words: Vec<&str> = text_to_check
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return false;
    }

    const PRIMARY_INDICATORS: &[&str] = &[
        "existing",
        "unchanged",
        "rest",
        "remaining",
        "remainder",
        "omitted",
        "snip",
        "snipped",
        "truncated",
        "here",
        "earlier",
        "later",
        "previous",
        "original",
        "same",
        "hidden",
        "skipped",
        "more",
        "etc",
    ];

    const CONTEXT_WORDS: &[&str] = &[
        "code",
        "logic",
        "implementation",
        "lines",
        "line",
        "of",
        "the",
        "function",
        "method",
        "class",
        "file",
        "script",
        "batch",
        "template",
        "query",
        "html",
        "lua",
        "lisp",
        "jsx",
        "component",
        "content",
        "contents",
        "as",
        "before",
        "above",
        "below",
        "other",
        "part",
        "parts",
        "section",
        "sections",
        "detail",
        "details",
    ];

    let mut has_primary = false;
    for &w in &words {
        let lower_w = w.to_ascii_lowercase();
        if PRIMARY_INDICATORS.contains(&lower_w.as_str()) {
            has_primary = true;
        } else if !CONTEXT_WORDS.contains(&lower_w.as_str())
            && !w.chars().all(|c| c.is_ascii_digit())
        {
            return false;
        }
    }

    if has_primary {
        trace!(
            "is_ellipsis_line: matched phrase-based ellipsis '{}'",
            trimmed
        );
    }
    has_primary
}

/// Normalizes and cleans a candidate relative path string.
///
/// Strips enclosing quotes, backticks, brackets, asterisks, leading relative prefixes
/// (`./`, `.\`), diff header prefixes (`a/`, `b/`), and trailing line/column number markers
/// (e.g., `:42`, `:10:5`).
///
/// # Arguments
///
/// * `s` - A string slice containing the candidate path string to clean.
///
/// # Returns
///
/// `Some(PathBuf)` containing the normalized relative path, or `None` if the input is empty or invalid.
///
/// # Examples
///
/// ```
/// use mpatch::normalize_candidate_path;
/// use std::path::PathBuf;
///
/// assert_eq!(
///     normalize_candidate_path("`src/main.rs`"),
///     Some(PathBuf::from("src/main.rs"))
/// );
/// assert_eq!(
///     normalize_candidate_path("b/src/components/Button.tsx:42:10"),
///     Some(PathBuf::from("src/components/Button.tsx"))
/// );
/// assert_eq!(
///     normalize_candidate_path("./config/.env.local"),
///     Some(PathBuf::from("config/.env.local"))
/// );
/// assert_eq!(normalize_candidate_path(""), None);
/// ```
pub fn normalize_candidate_path(s: &str) -> Option<PathBuf> {
    let mut s = s.trim();
    if s.is_empty() {
        return None;
    }

    s = s.trim_matches(|c: char| {
        c == '`'
            || c == '"'
            || c == '\''
            || c == '*'
            || c == '('
            || c == ')'
            || c == '['
            || c == ']'
    });

    if let Some(rest) = s.strip_prefix("./").or_else(|| s.strip_prefix(".\\")) {
        s = rest.trim();
    }

    let stripped = if let Some(rest) = s.strip_prefix("a/") {
        rest
    } else if let Some(rest) = s.strip_prefix("b/") {
        rest
    } else if let Some(rest) = s.strip_prefix("a\\") {
        rest
    } else if let Some(rest) = s.strip_prefix("b\\") {
        rest
    } else {
        s
    };

    let mut final_str = stripped.trim_end_matches(':').trim();
    while final_str.len() > 2 {
        let check_str =
            if final_str.as_bytes()[1] == b':' && final_str.as_bytes()[0].is_ascii_alphabetic() {
                &final_str[2..]
            } else {
                final_str
            };
        if let Some(rel_colon) = check_str.rfind(':') {
            let after = &check_str[rel_colon + 1..];
            if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
                let colon_idx = final_str.len() - check_str.len() + rel_colon;
                final_str = final_str[..colon_idx].trim();
                continue;
            }
        }
        break;
    }

    if final_str.is_empty() {
        None
    } else {
        trace!(
            "normalize_candidate_path: candidate normalized from '{}' to '{}'",
            s,
            final_str
        );
        Some(PathBuf::from(final_str))
    }
}

/// Determines whether a string looks like a plausible file path rather than conversational prose.
///
/// Evaluates candidate strings by checking for path separators (`/`, `\`), standard file
/// extensions, known special filenames (such as `Dockerfile`, `Makefile`, `.gitignore`),
/// and rejecting conversational English stop words, URLs, sentences, mathematical expressions,
/// and illegal filename characters.
///
/// # Arguments
///
/// * `s` - A string slice containing the candidate text to inspect.
///
/// # Returns
///
/// `true` if `s` is a plausible file path, `false` otherwise.
///
/// # Examples
///
/// ```
/// use mpatch::is_plausible_file_path;
///
/// assert!(is_plausible_file_path("src/main.rs"));
/// assert!(is_plausible_file_path("components/Button.test.tsx"));
/// assert!(is_plausible_file_path(".gitignore"));
/// assert!(is_plausible_file_path("Dockerfile"));
/// assert!(is_plausible_file_path("config/.env.production"));
///
/// // Conversational English, web URLs, and sentences are rejected:
/// assert!(!is_plausible_file_path("Here is the code in main.rs:"));
/// assert!(!is_plausible_file_path("https://example.com/file.rs"));
/// assert!(!is_plausible_file_path("Please check the following file"));
/// ```
pub fn is_plausible_file_path(s: &str) -> bool {
    let mut s = s.trim();
    while (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
        || (s.starts_with('`') && s.ends_with('`'))
        || (s.starts_with('(') && s.ends_with(')'))
        || (s.starts_with('[') && s.ends_with(']'))
        || (s.starts_with('*') && s.ends_with('*'))
    {
        if s.len() <= 2 {
            trace!(
                "is_plausible_file_path: candidate '{}' too short after trimming delimiters",
                s
            );
            return false;
        }
        s = s[1..s.len() - 1].trim();
    }

    while s.len() > 2 {
        let check_str = if s.as_bytes()[1] == b':' && s.as_bytes()[0].is_ascii_alphabetic() {
            &s[2..]
        } else {
            s
        };
        if let Some(rel_colon) = check_str.rfind(':') {
            let after = &check_str[rel_colon + 1..];
            if !after.is_empty() && after.chars().all(|c| c.is_ascii_digit()) {
                let colon_idx = s.len() - check_str.len() + rel_colon;
                s = s[..colon_idx].trim();
                continue;
            }
        }
        break;
    }

    if let Some(rest) = s.strip_prefix("./").or_else(|| s.strip_prefix(".\\")) {
        s = rest.trim();
    }

    if s.is_empty() || s.len() > 260 || s.ends_with('.') {
        trace!(
            "is_plausible_file_path: '{}' rejected (empty, length > 260, or ends with '.')",
            s
        );
        return false;
    }

    if (s.contains(":\\") || s.contains(":/"))
        && !(s.len() >= 3 && s.as_bytes()[1] == b':' && s.as_bytes()[0].is_ascii_alphabetic())
    {
        trace!(
            "is_plausible_file_path: '{}' rejected (invalid colon position)",
            s
        );
        return false;
    }

    if s.contains(['\0', '<', '>', '|', '?', '"', '{', '}', ';', '=', '*']) {
        trace!(
            "is_plausible_file_path: '{}' rejected (contains forbidden characters)",
            s
        );
        return false;
    }

    if s.contains("://")
        || s.contains("->")
        || s.contains("=>")
        || s.contains("()")
        || s.contains("!=")
        || s.contains("==")
        || s.contains("+=")
        || s.contains("-=")
        || s.contains("::")
    {
        trace!(
            "is_plausible_file_path: '{}' rejected (contains code syntax tokens)",
            s
        );
        return false;
    }

    if s.contains(char::is_whitespace) {
        if s.chars().filter(|c| c.is_whitespace()).count() > 3 {
            return false;
        }
        let lower = s.to_ascii_lowercase();
        const STOPWORDS: &[&str] = &[
            " the ",
            " is ",
            " to ",
            " in ",
            " for ",
            " and ",
            " or ",
            " we ",
            " this ",
            " that ",
            " with ",
            " here ",
            " update ",
            " change ",
            " code ",
            " file ",
            " replace ",
            " search ",
            " below ",
            " following ",
            " should ",
            " would ",
            " could ",
            " can ",
            " please ",
            " you ",
            " my ",
            " your ",
            " our ",
            " have ",
            " has ",
            " had ",
            " will ",
            " make ",
            " need ",
            " want ",
            " like ",
            " see ",
            " use ",
            " using ",
            " from ",
            " into ",
            " about ",
            " edit ",
            " modify ",
            " at ",
            " error ",
            " warning ",
            " modified ",
            " by ",
            " on ",
            " version ",
            " release ",
        ];
        let padded = format!(" {} ", lower);
        if STOPWORDS.iter().any(|&w| padded.contains(w)) {
            trace!(
                "is_plausible_file_path: '{}' rejected (contains English stop word)",
                s
            );
            return false;
        }
        if !s.contains('.') && !s.contains('/') && !s.contains('\\') {
            return false;
        }
    }

    let has_slash = s.contains('/') || s.contains('\\');
    if s.starts_with('.') && !has_slash {
        let rest = &s[1..];
        if !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
        {
            return !rest.chars().all(|c| c == '.');
        }
    }

    let has_valid_extension = if let Some(dot_pos) = s.rfind('.') {
        if dot_pos > 0 || has_slash {
            let ext = &s[dot_pos + 1..];
            !ext.is_empty()
                && ext.len() <= 16
                && ext.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                && ext.chars().any(|c| c.is_ascii_alphabetic())
        } else {
            false
        }
    } else {
        false
    };

    let filename = if let Some(pos) = s.rfind(['/', '\\']) {
        &s[pos + 1..]
    } else {
        s
    };
    let lower_filename = filename.to_ascii_lowercase();

    let is_known_filename = matches!(
        lower_filename.as_str(),
        "dockerfile"
            | "containerfile"
            | "makefile"
            | "gnumakefile"
            | "cmakelists.txt"
            | "gemfile"
            | "rakefile"
            | "procfile"
            | "vagrantfile"
            | "brewfile"
            | "justfile"
            | "tiltfile"
            | "jenkinsfile"
            | "pipfile"
            | "capfile"
            | "doxyfile"
            | "snakefile"
            | "podfile"
            | "cartfile"
            | "license"
            | "licence"
            | "copying"
            | "notice"
            | "readme"
            | "changelog"
            | "contributing"
            | "authors"
            | ".gitignore"
            | ".gitattributes"
            | ".gitmodules"
            | ".mailmap"
            | ".env"
            | ".dockerignore"
            | ".editorconfig"
            | ".prettierrc"
            | ".eslintrc"
            | ".babelrc"
            | ".npmrc"
            | ".nvmrc"
    ) || lower_filename.starts_with(".env.");

    let res = if s.contains(char::is_whitespace) {
        has_valid_extension || is_known_filename
    } else {
        has_slash || has_valid_extension || is_known_filename
    };
    trace!(
        "is_plausible_file_path: evaluated '{}': has_slash={}, has_valid_extension={}, is_known_filename={}, result={}",
        s,
        has_slash,
        has_valid_extension,
        is_known_filename,
        res
    );
    res
}

/// Extracts a plausible file path from a line preceding or introducing an Aider block.
///
/// Searches for target file paths in common AI and developer conversational conventions:
/// - Diff header lines (`diff --git a/path b/path`, `--- a/path`, `+++ b/path`)
/// - Quoted or backtick-enclosed paths (`` `src/file.rs` ``, `"src/file.rs"`, `'src/file.rs'`)
/// - Markdown links (`[Label](path/to/file.rs)`)
/// - Markdown headings or list items (`### src/models/user.rs`, `1. config.toml:`)
/// - Code comments or conversational prompts (`// filepath: internal/auth.go`, `In src/app.py:`)
///
/// # Arguments
///
/// * `line` - A string slice containing the candidate line preceding or introducing a diff block.
///
/// # Returns
///
/// `Some(PathBuf)` containing the extracted file path if a plausible path is found, or `None`.
///
/// # Examples
///
/// ```
/// use mpatch::extract_file_path_from_line;
/// use std::path::PathBuf;
///
/// assert_eq!(
///     extract_file_path_from_line("Update `src/server.ts` with the new handler:"),
///     Some(PathBuf::from("src/server.ts"))
/// );
/// assert_eq!(
///     extract_file_path_from_line("// filepath: internal/auth/token.go"),
///     Some(PathBuf::from("internal/auth/token.go"))
/// );
/// assert_eq!(
///     extract_file_path_from_line("### src/models/user.py:42"),
///     Some(PathBuf::from("src/models/user.py"))
/// );
/// assert_eq!(
///     extract_file_path_from_line("Here is the updated implementation:"),
///     None
/// );
/// ```
pub fn extract_file_path_from_line(line: &str) -> Option<PathBuf> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    trace!("extract_file_path_from_line: analyzing line '{}'", trimmed);

    if trimmed.starts_with("```")
        || trimmed.starts_with("<<<<")
        || trimmed.starts_with("====")
        || trimmed.starts_with(">>>>")
    {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("diff --git ") {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Some(path) = normalize_candidate_path(parts[1]) {
                if is_plausible_file_path(&path.to_string_lossy()) {
                    return Some(path);
                }
            }
            if let Some(path) = normalize_candidate_path(parts[0]) {
                if is_plausible_file_path(&path.to_string_lossy()) {
                    return Some(path);
                }
            }
        }
    }

    if trimmed.starts_with("--- ") || trimmed.starts_with("+++ ") {
        let candidate = trimmed[4..].trim();
        if candidate != "/dev/null" && candidate != "a/dev/null" && candidate != "b/dev/null" {
            if let Some(path) = normalize_candidate_path(candidate) {
                if is_plausible_file_path(&path.to_string_lossy()) {
                    return Some(path);
                }
            }
        }
        return None;
    }

    for quote_char in ['`', '"', '\''] {
        let mut chars = trimmed.char_indices();
        while let Some((start, ch)) = chars.next() {
            if ch == quote_char {
                for (end, end_ch) in chars.by_ref() {
                    if end_ch == quote_char {
                        let candidate = trimmed[start + 1..end].trim();
                        if is_plausible_file_path(candidate) {
                            let path = normalize_candidate_path(candidate);
                            trace!("extract_file_path_from_line: detected quoted path {:?} in line '{}'", path, trimmed);
                            return path;
                        }
                        break;
                    }
                }
            }
        }
    }

    let mut search_from = 0;
    while let Some(open_rel) = trimmed[search_from..].find('[') {
        let open = search_from + open_rel;
        if let Some(close_rel) = trimmed[open + 1..].find(']') {
            let close = open + 1 + close_rel;
            let label = trimmed[open + 1..close].trim();
            let rest = &trimmed[close + 1..];
            if rest.starts_with('(') {
                if let Some(paren_close) = rest.find(')') {
                    let target = rest[1..paren_close].trim();
                    if is_plausible_file_path(target) {
                        return normalize_candidate_path(target);
                    }
                }
            }
            if is_plausible_file_path(label) {
                return normalize_candidate_path(label);
            }
            search_from = close + 1;
        } else {
            break;
        }
    }

    let mut candidate = trimmed;
    while candidate.starts_with('#')
        || candidate.starts_with('*')
        || candidate.starts_with('-')
        || candidate.starts_with('>')
        || candidate.starts_with('/')
    {
        candidate = candidate
            .trim_start_matches(['#', '*', '-', '>', '/'])
            .trim_start();
    }

    if let Some(dot_pos) = candidate.find(". ") {
        let num_part = &candidate[..dot_pos];
        if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
            candidate = candidate[dot_pos + 2..].trim_start();
        }
    }

    const PREFIXES: &[&str] = &[
        "file:",
        "path:",
        "filename:",
        "in file:",
        "in:",
        "for file:",
        "for:",
        "update file:",
        "update:",
        "edit file:",
        "edit:",
        "modify file:",
        "modify:",
        "changes in:",
        "changes to:",
        "patch for:",
        "filepath:",
        "file path:",
    ];

    let mut changed = true;
    while changed {
        changed = false;
        let lower = candidate.to_ascii_lowercase();
        for &prefix in PREFIXES {
            if lower.starts_with(prefix) {
                candidate = candidate[prefix.len()..].trim_start();
                changed = true;
                break;
            }
        }
    }

    let cleaned = candidate.trim_matches(|c: char| {
        c == '`'
            || c == '*'
            || c == '"'
            || c == '\''
            || c == ':'
            || c == '('
            || c == ')'
            || c == '['
            || c == ']'
            || c == '{'
            || c == '}'
            || c.is_whitespace()
    });
    if is_plausible_file_path(cleaned) {
        let path = normalize_candidate_path(cleaned);
        trace!(
            "extract_file_path_from_line: extracted prefix path {:?} from line '{}'",
            path,
            trimmed
        );
        return path;
    }

    for raw_token in candidate.split_whitespace() {
        let token = raw_token.trim_matches(|c: char| {
            c == '`'
                || c == '*'
                || c == '"'
                || c == '\''
                || c == ':'
                || c == ','
                || c == ';'
                || c == '('
                || c == ')'
                || c == '['
                || c == ']'
        });
        if is_plausible_file_path(token) {
            let path = normalize_candidate_path(token);
            trace!(
                "extract_file_path_from_line: extracted token path {:?} from line '{}'",
                path,
                trimmed
            );
            return path;
        }
    }

    trace!(
        "extract_file_path_from_line: no plausible path found in line '{}'",
        trimmed
    );
    None
}

/// Automatically detects the patch format of the provided content.
///
/// This function scans the content efficiently (without parsing the full structure)
/// to determine if it contains Markdown code blocks, standard unified diff headers,
/// Aider search/replace blocks, or conflict markers.
///
/// ## Behavior
///
/// The detection follows this priority:
/// 1. **Markdown**: If code fences (3+ backticks) are found containing diff signatures, it is treated as Markdown.
/// 2. **Unified**: If `--- a/` or `diff --git` headers are found, it is treated as a Unified Diff.
/// 3. **Aider**: If `<<<<<<< SEARCH` / `ORIGINAL` markers are found, it is treated as Aider search/replace blocks.
/// 4. **Conflict**: If `<<<<` markers are found, it is treated as Conflict Markers.
///
/// # Arguments
///
/// * `content` - A string slice containing the patch data to analyze.
///
/// # Returns
///
/// The detected [`PatchFormat`].
///
/// # Examples
/// ```rust
/// use mpatch::{detect_patch, PatchFormat};
///
/// let md = "```diff\n--- a/f\n+++ b/f\n```";
/// assert_eq!(detect_patch(md), PatchFormat::Markdown);
///
/// let raw = "--- a/f\n+++ b/f\n@@ -1 +1 @@";
/// assert_eq!(detect_patch(raw), PatchFormat::Unified);
///
/// let aider = "app.py\n<<<<<<< SEARCH\nold\n=======\nnew\n>>>>>>> REPLACE";
/// assert_eq!(detect_patch(aider), PatchFormat::Aider);
/// ```
pub fn detect_patch(content: &str) -> PatchFormat {
    trace!(
        "detect_patch: analyzing content ({} bytes, ~{} lines)",
        content.len(),
        content.lines().count()
    );
    let mut lines = content.lines().peekable();
    let mut in_code_block = false;
    let mut current_fence_len = 0;
    let mut has_unified_headers = false;
    let mut has_conflict_start = false;
    let mut has_conflict_middle_or_end = false;
    let mut has_conflict_markers = false;
    let mut has_aider_start = false;
    let mut has_aider_middle_or_end = false;
    let mut has_aider_markers = false;

    while let Some(line) = lines.next() {
        // Check for Markdown code blocks
        let trimmed = line.trim_start();
        if trimmed.as_bytes().starts_with(b"```") {
            let fence_len = count_leading_backticks(trimmed);
            if fence_len >= 3 {
                if !in_code_block {
                    in_code_block = true;
                    current_fence_len = fence_len;
                    let info = &trimmed[fence_len..];
                    if info.contains("diff") || info.contains("patch") {
                        debug!(
                            "detect_patch: recognized Markdown code block with '{}'",
                            info.trim()
                        );
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
            trace!(
                "detect_patch: found unified diff signature in line: '{}' (in_code_block={})",
                line,
                line
            );
            if in_code_block {
                debug!("detect_patch: recognized Unified diff headers inside Markdown code block");
                return PatchFormat::Markdown;
            }
            has_unified_headers = true;
        }

        // Check for Aider Search/Replace Markers
        if is_aider_search_fence(trimmed) {
            trace!("detect_patch: found Aider search fence: '{}'", trimmed);
            has_aider_start = true;
        } else if (trimmed.starts_with("====") || is_aider_replace_fence(trimmed))
            && has_aider_start
        {
            trace!(
                "detect_patch: found Aider divide or replace fence: '{}'",
                trimmed
            );
            has_aider_middle_or_end = true;
        }
        if has_aider_start && has_aider_middle_or_end {
            if in_code_block {
                debug!("detect_patch: recognized Aider search/replace markers inside Markdown code block");
                return PatchFormat::Markdown;
            }
            has_aider_markers = true;
        }

        // Check for Conflict Markers
        if trimmed.starts_with("<<<<") {
            trace!("detect_patch: found conflict start marker: '{}'", trimmed);
            has_conflict_start = true;
        } else if (trimmed.starts_with("====") || trimmed.starts_with(">>>>")) && has_conflict_start
        {
            trace!(
                "detect_patch: found conflict middle/end marker: '{}'",
                trimmed
            );
            has_conflict_middle_or_end = true;
        }

        if has_conflict_start && has_conflict_middle_or_end {
            if in_code_block {
                debug!("detect_patch: recognized conflict markers inside Markdown code block");
                return PatchFormat::Markdown;
            }
            has_conflict_markers = true;
        }
    }

    let detected = if has_unified_headers {
        PatchFormat::Unified
    } else if has_aider_markers {
        PatchFormat::Aider
    } else if has_conflict_markers {
        PatchFormat::Conflict
    } else {
        PatchFormat::Unknown
    };
    debug!("detect_patch: detected format {:?}", detected);
    detected
}

/// Automatically detects the format of the input text and parses it into a list of patches.
///
/// This is the recommended entry point for most use cases, as it robustly handles
/// the various ways diffs are commonly presented (e.g., inside Markdown code blocks
/// from LLMs, as raw output from `git diff`, or as conflict markers in source files).
///
/// ## Supported Formats
///
/// 1.  **Markdown:** Code blocks fenced with backticks (e.g., ` ```diff `) containing
///     diff content. This is the standard output format for AI coding assistants.
/// 2.  **Unified Diff:** Standard diffs containing `--- a/path` and `+++ b/path` headers.
/// 3.  **Aider Search/Replace:** Blocks delimited by `<<<<<<< SEARCH` (or `ORIGINAL`), `=======`,
///     and `>>>>>>> REPLACE` (or `UPDATED`). File paths are detected automatically.
/// 4.  **Conflict Markers:** Blocks delimited by `<<<<`, `====`, and `>>>>`. These are
///     parsed into patches where the "old" content is removed and the "new" content is added.
///
/// ## Behavior
///
/// The function first attempts to detect the format using lightweight heuristics
/// (see [`detect_patch`]).
///
/// - If **Markdown** is detected, it extracts patches from all valid code blocks.
/// - If **Unified Diff** headers are detected, it parses the entire string as a raw diff.
/// - If **Aider Search/Replace** blocks are detected, it parses them into patches.
/// - If **Conflict Markers** are detected, it parses the blocks into patches targeting a generic file path.
/// - If the format is **Unknown**, it attempts to parse the content as a raw diff
///   as a fallback. This allows parsing fragments that might lack full file headers
///   but contain valid hunks.
///
/// # Arguments
///
/// * `content` - A string slice containing the patch data to analyze.
///
/// # Returns
///
/// A `Result` containing a vector of [`Patch`] objects on success.
///
/// # Errors
///
/// Returns `Err(`[`ParseError`]`)` if the content is detected as a format but fails to parse
/// (e.g., a unified diff missing file headers).
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
/// **Parsing Aider Search/Replace Blocks:**
/// ````
/// use mpatch::parse_auto;
///
/// let aider = r#"
/// src/app.py
/// <<<<<<< SEARCH
/// def run():
///     old_logic()
/// =======
/// def run():
///     new_logic()
/// >>>>>>> REPLACE
/// "#;
///
/// let patches = parse_auto(aider).unwrap();
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].file_path.to_str(), Some("src/app.py"));
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
    debug!("parse_auto: analyzing input ({} bytes)", content.len());
    let format = detect_patch(content);
    debug!("Auto-detected patch format: {:?}", format);
    match format {
        PatchFormat::Markdown => {
            let patches = parse_diffs(content)?;
            debug!(
                "parse_auto: parsed {} patch(es) from Markdown blocks",
                patches.len()
            );
            Ok(patches)
        }
        PatchFormat::Unified => {
            let patches = parse_patches(content)?;
            debug!(
                "parse_auto: parsed {} patch(es) from Unified diff",
                patches.len()
            );
            Ok(patches)
        }
        PatchFormat::Aider => {
            let patches = parse_aider(content);
            debug!(
                "Parsed {} patches from Aider search/replace blocks.",
                patches.len()
            );
            Ok(patches)
        }
        PatchFormat::Conflict => {
            let patches = parse_conflict_markers(content);
            debug!("Parsed {} patches from conflict markers.", patches.len());
            Ok(patches)
        }
        PatchFormat::Unknown => {
            // If unknown, we try parsing as raw patches as a fallback,
            // as it might be a fragment without headers.
            debug!("Patch format unknown. Falling back to raw unified diff parsing.");
            let patches = parse_patches(content)?;
            if !patches.is_empty() {
                debug!(
                    "Fallback parsing successful, found {} patch(es).",
                    patches.len()
                );
                Ok(patches)
            } else {
                debug!(
                    "Fallback unified diff parsing found no patches. Trying Aider search/replace."
                );
                let aider_patches = parse_aider(content);
                if !aider_patches.is_empty() {
                    debug!(
                        "Fallback Aider parsing found {} patch(es).",
                        aider_patches.len()
                    );
                    return Ok(aider_patches);
                }
                // If that yields nothing, return empty.
                debug!("Fallback parsing found no patches.");
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
/// It supports the following formats within the blocks:
/// 1. **Unified Diff:** Standard `--- a/file`, `+++ b/file`, `@@ ... @@` format.
/// 2. **Aider Search/Replace:** `<<<<<<< SEARCH`, `=======`, `>>>>>>> REPLACE` blocks.
/// 3. **Conflict Markers:** `<<<<`, `====`, `>>>>` blocks. Since these lack file headers,
///    patches will be assigned a generic file path (`patch_target`) unless a preceding path is found.
///
/// For automatic format detection (supporting raw diffs and conflict markers outside of markdown),
/// use [`parse_auto()`].
///
/// # Arguments
///
/// * `content` - A string slice containing the markdown content to parse.
///
/// # Returns
///
/// A `Result` containing a vector of [`Patch`] objects on success.
///
/// # Errors
///
/// Returns `Err(`[`ParseError`]`)` if a block looks like a patch (e.g. has `--- a/file`) but fails
/// to parse correctly. Blocks that simply lack headers are ignored.
///
/// # Examples
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
    let mut preceding_lines: Vec<&str> = Vec::new();

    while let Some((line_index, line_text)) = lines.next() {
        let trimmed = line_text.trim_start();
        if trimmed.starts_with("```") && count_leading_backticks(trimmed) >= 3 {
            let fence_len = count_leading_backticks(trimmed);
            let opening_indent = line_text.len() - trimmed.len();
            let block_info = trimmed[fence_len..].trim();

            trace!(
                "parse_diffs: found code block start at line {}: fence_len={}, indent={}, info='{}'",
                line_index + 1,
                fence_len,
                opening_indent,
                block_info
            );
            let diff_block_start_line = line_index + 1;

            let mut block_lines = Vec::new();

            // Consume lines until end of block
            while let Some((_, line)) = lines.peek() {
                let inner_trimmed = line.trim_start();
                let current_indent = line.len() - inner_trimmed.len();
                if inner_trimmed.starts_with("```")
                    && count_leading_backticks(inner_trimmed) >= fence_len
                    && current_indent <= opening_indent
                {
                    trace!(
                        "parse_diffs: closing code block for start line {} at line {}",
                        diff_block_start_line,
                        line_index + 1
                    );
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
                let default_file_path = preceding_lines
                    .iter()
                    .rev()
                    .find_map(|l| extract_file_path_from_line(l));
                if let Some(ref path) = default_file_path {
                    trace!(
                        "  Inferred target file path for block from preceding text: '{}'",
                        path.display()
                    );
                } else {
                    trace!("  No preceding target file path found for block");
                }
                let block_patches = parse_generic_block_lines(
                    &block_lines,
                    diff_block_start_line,
                    default_file_path,
                )?;
                debug!(
                    "  Extracted {} patch(es) from block starting on line {}.",
                    block_patches.len(),
                    diff_block_start_line
                );
                all_patches.extend(block_patches);
            } else {
                trace!(
                    "Skipping code block starting on line {} (no patch markers found).",
                    diff_block_start_line
                );
            }
            preceding_lines.clear();
        } else {
            if preceding_lines.len() >= 5 {
                preceding_lines.remove(0);
            }
            preceding_lines.push(line_text);
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
///
/// # Arguments
///
/// * `lines` - A slice of lines from within a code block.
///
/// # Returns
///
/// `true` if un-nested patch headers (`--- `, `diff --git`, or conflict markers) are found.
fn has_patch_signature_at_level_1<S: AsRef<str>>(lines: &[S]) -> bool {
    let mut in_nested_block = false;
    let mut current_fence_len = 0;

    for line in lines {
        let line = line.as_ref();
        let trimmed = line.trim_start();

        // Check for nested block boundaries
        if trimmed.starts_with("```") {
            let fence_len = count_leading_backticks(trimmed);
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
                || is_aider_search_fence(trimmed)
                || trimmed.starts_with("<<<<")
                || trimmed.starts_with("====")
                || trimmed.starts_with(">>>>"))
        {
            trace!(
                "has_patch_signature_at_level_1: matched top-level patch signature: '{}'",
                trimmed
            );
            return true;
        }
    }
    false
}

/// Helper function to parse a block of lines that could be Unified, Aider, or Conflict.
///
/// This consolidates the fallback logic inside [`parse_diffs`]. It first
/// attempts standard unified diff parsing; if no patches or headers are found,
/// it attempts Aider search/replace parsing, and finally falls back to parsing conflict markers.
///
/// # Arguments
///
/// * `lines` - A slice of line string slices forming the block body.
/// * `start_line` - 1-based line number where the code block began, for reporting errors.
///
/// # Returns
///
/// A vector of parsed [`Patch`] objects on success.
///
/// # Errors
///
/// Returns [`ParseError`] if the block contains patch signatures but is syntactically invalid.
fn parse_generic_block_lines(
    lines: &[&str],
    start_line: usize,
    default_file_path: Option<PathBuf>,
) -> Result<Vec<Patch>, ParseError> {
    trace!(
        "  Attempting to parse generic block starting at line {} ({} line(s)) as standard unified diff.",
        start_line,
        lines.len()
    );
    // 1. Try parsing as standard unified diff
    let standard_result = parse_patches_from_lines(lines.iter().copied());

    match standard_result {
        Ok(patches) => {
            if !patches.is_empty() {
                debug!("  Successfully parsed block starting at line {} as standard unified diff ({} patch(es)).", start_line, patches.len());
                Ok(patches)
            } else {
                trace!(
                    "  Standard parser found no patches. Attempting Aider search/replace blocks."
                );
                let aider_patches =
                    parse_aider_from_lines(lines.iter().copied(), default_file_path.clone());
                if !aider_patches.is_empty() {
                    debug!("  Successfully parsed block starting at line {} as Aider search/replace blocks ({} patch(es)).", start_line, aider_patches.len());
                    Ok(aider_patches)
                } else {
                    trace!("  No Aider blocks found. Attempting conflict markers.");
                    // 3. If standard and Aider parsing found nothing, try conflict markers
                    let conflict_patches = parse_conflict_markers_from_lines(lines.iter().copied());
                    if !conflict_patches.is_empty() {
                        debug!("  Successfully parsed block starting at line {} as conflict markers ({} patch(es)).", start_line, conflict_patches.len());
                    } else {
                        trace!("  No conflict markers found either.");
                    }
                    Ok(conflict_patches)
                }
            }
        }
        Err(e) => {
            trace!(
                "  Standard parsing failed ({}). Attempting Aider search/replace blocks.",
                e
            );
            let aider_patches = parse_aider_from_lines(lines.iter().copied(), default_file_path);
            if !aider_patches.is_empty() {
                debug!("  Successfully parsed block starting at line {} as Aider search/replace blocks ({} patch(es)).", start_line, aider_patches.len());
                Ok(aider_patches)
            } else {
                trace!("  Aider parsing found nothing. Attempting conflict markers.");
                let conflict_patches = parse_conflict_markers_from_lines(lines.iter().copied());
                if !conflict_patches.is_empty() {
                    debug!("  Successfully parsed block starting at line {} as conflict markers ({} patch(es)).", start_line, conflict_patches.len());
                    Ok(conflict_patches)
                } else {
                    trace!("  Conflict marker parsing also failed. Returning original error.");
                    match e {
                        ParseError::MissingFileHeader { .. } => {
                            Err(ParseError::MissingFileHeader { line: start_line })
                        }
                    }
                }
            }
        }
    }
}

/// Constructs a structured [`Hunk`] from Aider `SEARCH` and `REPLACE` blocks.
///
/// Aligns the search and replace lines using [`similar::TextDiff`], normalizing wildcard
/// ellipsis lines and generating corresponding context (` `), addition (`+`), and deletion (`-`)
/// hunk lines.
///
/// # Arguments
///
/// * `search_lines` - Lines from the `SEARCH` / `ORIGINAL` block.
/// * `replace_lines` - Lines from the `REPLACE` / `UPDATED` block.
///
/// # Returns
///
/// A structured [`Hunk`] representing the diff between the two blocks.
fn create_hunk_from_search_replace(search_lines: &[String], replace_lines: &[String]) -> Hunk {
    trace!(
        "create_hunk_from_search_replace: aligning {} search line(s) and {} replace line(s)",
        search_lines.len(),
        replace_lines.len()
    );
    let mut search_norm = Vec::with_capacity(search_lines.len());
    for s in search_lines {
        if is_ellipsis_line(s) {
            let mut norm = String::with_capacity(s.len() + 3);
            norm.push_str(get_indent(s));
            norm.push_str("...");
            search_norm.push(norm);
        } else {
            search_norm.push(s.clone());
        }
    }
    let mut replace_norm = Vec::with_capacity(replace_lines.len());
    for r in replace_lines {
        if is_ellipsis_line(r) {
            let mut norm = String::with_capacity(r.len() + 3);
            norm.push_str(get_indent(r));
            norm.push_str("...");
            replace_norm.push(norm);
        } else {
            replace_norm.push(r.clone());
        }
    }

    let search_refs: Vec<&str> = search_norm.iter().map(|s| s.as_str()).collect();
    let replace_refs: Vec<&str> = replace_norm.iter().map(|s| s.as_str()).collect();

    let diff = similar::TextDiff::configure()
        .algorithm(similar::Algorithm::Patience)
        .diff_slices(&search_refs, &replace_refs);
    let mut hunk_lines = Vec::new();

    let push_line = |lines: &mut Vec<String>, prefix: char, s: &str| {
        if prefix == '+' && is_ellipsis_line(s) {
            return;
        }
        let mut out = String::with_capacity(s.len() + 1);
        out.push(prefix);
        out.push_str(s);
        lines.push(out);
    };

    for op in diff.ops() {
        trace!("  create_hunk_from_search_replace: diff op {:?}", op);
        match *op {
            similar::DiffOp::Equal { old_index, len, .. } => {
                for i in 0..len {
                    push_line(&mut hunk_lines, ' ', search_refs[old_index + i]);
                }
            }
            similar::DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for i in 0..old_len {
                    push_line(&mut hunk_lines, '-', search_refs[old_index + i]);
                }
            }
            similar::DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for i in 0..new_len {
                    push_line(&mut hunk_lines, '+', replace_refs[new_index + i]);
                }
            }
            similar::DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for i in 0..old_len {
                    push_line(&mut hunk_lines, '-', search_refs[old_index + i]);
                }
                for i in 0..new_len {
                    push_line(&mut hunk_lines, '+', replace_refs[new_index + i]);
                }
            }
        }
    }

    debug!(
        "create_hunk_from_search_replace: produced hunk with {} lines (has_changes={})",
        hunk_lines.len(),
        hunk_lines.iter().any(|l| l.starts_with(['+', '-']))
    );
    Hunk {
        lines: hunk_lines,
        old_start_line: None,
        new_start_line: None,
    }
}

/// Parses a string containing Aider search/replace blocks (`<<<<<<< SEARCH`, `=======`, `>>>>>>> REPLACE`).
///
/// This format is widely used by [Aider](https://aider.chat) and LLM-based coding agents.
/// It consists of one or more search/replace blocks targeting specific files:
///
/// ```text
/// path/to/file.ext
/// <<<<<<< SEARCH
/// original lines to find
/// =======
/// new lines to replace with
/// >>>>>>> REPLACE
/// ```
///
/// If a file path precedes the search block (e.g. on the previous line, as a header,
/// or inside backticks), it is associated with that patch. Consecutive blocks without an
/// intervening file path are merged into the same [`Patch`]. If no file path is found,
/// it defaults to `patch_target`.
///
/// The `SEARCH` and `REPLACE` blocks are converted into structured [`Hunk`] objects
/// where identical lines are preserved as context lines, enabling `mpatch`'s fuzzy matching,
/// indentation adjustment, and candidate backtracking to operate with full fidelity.
///
/// # Arguments
///
/// * `content` - A string slice containing the Aider search/replace content.
///
/// # Returns
///
/// A vector of [`Patch`] objects.
///
/// # Examples
///
/// ```rust
/// use mpatch::parse_aider;
///
/// let diff = r#"
/// src/greeting.py
/// <<<<<<< SEARCH
/// def greet():
///     print("hello")
/// =======
/// def greet():
///     print("hello, world!")
/// >>>>>>> REPLACE
/// "#;
///
/// let patches = parse_aider(diff);
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].file_path.to_str(), Some("src/greeting.py"));
/// assert_eq!(patches[0].hunks[0].removed_lines(), vec!["    print(\"hello\")"]);
/// assert_eq!(patches[0].hunks[0].added_lines(), vec!["    print(\"hello, world!\")"]);
/// ```
pub fn parse_aider(content: &str) -> Vec<Patch> {
    debug!(
        "Starting to parse Aider search/replace content ({} bytes).",
        content.len()
    );
    let patches = parse_aider_from_lines(content.lines(), None);
    debug!(
        "Finished parsing Aider content. Found {} patch(es).",
        patches.len()
    );
    patches
}

/// Parses an iterator of lines containing "Aider" style search/replace blocks.
///
/// This is the line-iterator counterpart to [`parse_aider`]. It processes lines sequentially,
/// extracting search and replace fences (`<<<<<<< SEARCH` / `ORIGINAL`, `=======`,
/// `>>>>>>> REPLACE` / `UPDATED`), reconstructing context and changes into structured [`Hunk`]
/// objects, and grouping consecutive hunks for the same file into unified [`Patch`] instances.
///
/// # Arguments
///
/// * `lines` - An iterator yielding string slices for each line.
/// * `default_file_path` - An optional fallback [`PathBuf`] to use if a block does not specify a file.
///
/// # Returns
///
/// A vector of [`Patch`] objects parsed from the lines.
///
/// # Examples
///
/// ```
/// use mpatch::parse_aider_from_lines;
///
/// let lines = vec![
///     "src/lib.rs",
///     "<<<<<<< SEARCH",
///     "fn old() {}",
///     "=======",
///     "fn new() {}",
///     ">>>>>>> REPLACE",
/// ];
///
/// let patches = parse_aider_from_lines(lines.into_iter(), None);
/// assert_eq!(patches.len(), 1);
/// assert_eq!(patches[0].file_path.to_str(), Some("src/lib.rs"));
/// assert_eq!(patches[0].hunks[0].added_lines(), vec!["fn new() {}"]);
/// ```
pub fn parse_aider_from_lines<'a, I>(lines: I, default_file_path: Option<PathBuf>) -> Vec<Patch>
where
    I: Iterator<Item = &'a str>,
{
    debug!(
        "parse_aider_from_lines: scanning lines (default_file_path={:?})",
        default_file_path
    );
    let mut unmerged_patches: Vec<Patch> = Vec::new();
    let mut active_file_path: Option<PathBuf> = default_file_path;
    let mut pending_file_path: Option<PathBuf> = None;

    enum State {
        Outside,
        InSearch,
        InReplace,
    }
    let mut state = State::Outside;
    let mut search_lines: Vec<String> = Vec::new();
    let mut replace_lines: Vec<String> = Vec::new();

    for line in lines {
        let trimmed = line.trim_start();

        if is_aider_search_fence(trimmed) {
            let path_on_fence = extract_file_path_from_search_fence(trimmed);
            let target_file = path_on_fence
                .or_else(|| pending_file_path.take())
                .or_else(|| active_file_path.clone())
                .unwrap_or_else(|| PathBuf::from("patch_target"));

            debug!(
                "  Aider parser: starting SEARCH block for '{}' (fence: '{}')",
                target_file.display(),
                trimmed
            );
            active_file_path = Some(target_file.clone());

            search_lines.clear();
            replace_lines.clear();
            state = State::InSearch;
            continue;
        }

        if is_aider_divide_fence(trimmed) && matches!(state, State::InSearch) {
            debug!(
                "  Aider parser: transitioned to REPLACE block ({} search lines collected)",
                search_lines.len()
            );
            state = State::InReplace;
            continue;
        }

        if is_aider_replace_fence(trimmed) && matches!(state, State::InReplace) {
            debug!(
                "  Aider parser: ending REPLACE block ({} replace lines collected) for '{}'",
                replace_lines.len(),
                active_file_path
                    .as_deref()
                    .unwrap_or(Path::new("patch_target"))
                    .display()
            );
            state = State::Outside;

            let file_path = active_file_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("patch_target"));

            while search_lines.first().is_some_and(|l| is_ellipsis_line(l))
                && replace_lines.first().is_some_and(|l| is_ellipsis_line(l))
            {
                search_lines.remove(0);
                replace_lines.remove(0);
            }
            while search_lines.last().is_some_and(|l| is_ellipsis_line(l))
                && replace_lines.last().is_some_and(|l| is_ellipsis_line(l))
            {
                search_lines.pop();
                replace_lines.pop();
            }

            let s_has = search_lines.iter().any(|l| is_ellipsis_line(l));
            let r_has = replace_lines.iter().any(|l| is_ellipsis_line(l));
            if s_has && r_has {
                let s_segs = split_lines_by_ellipsis(&search_lines);
                let r_segs = split_lines_by_ellipsis(&replace_lines);
                let changed_indices: Vec<usize> = s_segs
                    .iter()
                    .zip(r_segs.iter())
                    .enumerate()
                    .filter_map(|(i, (s, r))| if s != r { Some(i) } else { None })
                    .collect();

                if s_segs.len() == r_segs.len() && changed_indices.len() > 1 {
                    debug!(
                        "  Aider search/replace has {} wildcard segment(s) with {} modified site(s) for '{}'",
                        s_segs.len(),
                        changed_indices.len(),
                        file_path.display()
                    );
                    for &idx in &changed_indices {
                        let mut s_sub = s_segs[idx].clone();
                        let mut r_sub = r_segs[idx].clone();

                        if idx > 0
                            && !changed_indices.contains(&(idx - 1))
                            && !s_segs[idx - 1].is_empty()
                        {
                            let mut new_s = s_segs[idx - 1].clone();
                            new_s.push("...".to_string());
                            new_s.append(&mut s_sub);
                            s_sub = new_s;

                            let mut new_r = r_segs[idx - 1].clone();
                            new_r.push("...".to_string());
                            new_r.append(&mut r_sub);
                            r_sub = new_r;
                        }

                        let hunk = create_hunk_from_search_replace(&s_sub, &r_sub);
                        if hunk.has_changes() {
                            unmerged_patches.push(Patch {
                                file_path: file_path.clone(),
                                hunks: vec![hunk],
                                ends_with_newline: true,
                            });
                        }
                    }
                    search_lines.clear();
                    replace_lines.clear();
                    continue;
                }
            }

            let hunk = create_hunk_from_search_replace(&search_lines, &replace_lines);
            if hunk.has_changes() || !search_lines.is_empty() || !replace_lines.is_empty() {
                debug!(
                    "  Adding Aider patch hunk with {} line(s) for '{}'",
                    hunk.lines.len(),
                    file_path.display()
                );
                unmerged_patches.push(Patch {
                    file_path,
                    hunks: vec![hunk],
                    ends_with_newline: true,
                });
            }
            search_lines.clear();
            replace_lines.clear();
            continue;
        }

        match state {
            State::Outside => {
                if let Some(path) = extract_file_path_from_line(line) {
                    pending_file_path = Some(path);
                }
            }
            State::InSearch => {
                search_lines.push(line.to_string());
            }
            State::InReplace => {
                replace_lines.push(line.to_string());
            }
        }
    }

    if matches!(state, State::InReplace) && (!search_lines.is_empty() || !replace_lines.is_empty())
    {
        let file_path = active_file_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("patch_target"));
        let hunk = create_hunk_from_search_replace(&search_lines, &replace_lines);
        unmerged_patches.push(Patch {
            file_path,
            hunks: vec![hunk],
            ends_with_newline: true,
        });
    }

    if unmerged_patches.is_empty() {
        return Vec::new();
    }

    let mut merged_patches: Vec<Patch> = Vec::new();
    for patch in unmerged_patches {
        if let Some(existing) = merged_patches
            .iter_mut()
            .find(|p| p.file_path == patch.file_path)
        {
            existing.hunks.extend(patch.hunks);
        } else {
            merged_patches.push(patch);
        }
    }

    debug!(
        "parse_aider_from_lines: completed. Merged into {} patch(es).",
        merged_patches.len()
    );
    merged_patches
}

/// Splits a slice of lines into segments separated by ellipsis wildcard lines.
///
/// # Arguments
///
/// * `lines` - Slice of lines to split around ellipsis boundaries.
///
/// # Returns
///
/// A vector of line segments excluding the ellipsis separator lines.
fn split_lines_by_ellipsis(lines: &[String]) -> Vec<Vec<String>> {
    trace!("split_lines_by_ellipsis: input {} line(s)", lines.len());
    let mut segs = Vec::new();
    let mut current = Vec::new();
    for line in lines {
        if is_ellipsis_line(line) {
            segs.push(std::mem::take(&mut current));
        } else {
            current.push(line.clone());
        }
    }
    segs.push(current);
    trace!(
        "split_lines_by_ellipsis: partitioned into {} segment(s)",
        segs.len()
    );
    segs
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
/// # Returns
///
/// A single [`Patch`] object on success.
///
/// # Errors
///
/// Returns a [`SingleParseError`] if:
/// - The underlying parsing fails (e.g., a diff block is missing a file header).
/// - No patches are found in the content.
/// - More than one patch is found in the content.
///
/// # Examples
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
    debug!(
        "parse_single_patch: parsing single patch from content ({} bytes)",
        content.len()
    );
    let mut patches = parse_auto(content)?;

    if patches.len() > 1 {
        warn!(
            "parse_single_patch: expected exactly 1 patch, but found {}",
            patches.len()
        );
        Err(SingleParseError::MultiplePatchesFound(patches.len()))
    } else if patches.is_empty() {
        warn!("parse_single_patch: no patches found in input");
        Err(SingleParseError::NoPatchesFound)
    } else {
        let p = patches.remove(0);
        debug!(
            "parse_single_patch: successfully parsed patch for '{}' ({} hunks)",
            p.file_path.display(),
            p.hunks.len()
        );
        Ok(p)
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
/// * `content` - A string slice containing the raw unified diff content.
///
/// # Returns
///
/// A `Result` containing a vector of [`Patch`] objects on success.
///
/// # Errors
///
/// Returns `Err(`[`ParseError::MissingFileHeader`]`)` if the content contains patch
/// hunks but no `--- a/path/to/file` header.
///
/// # Examples
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
    debug!(
        "Starting to parse raw diff content ({} bytes).",
        content.len()
    );
    parse_patches_from_lines(content.lines())
}

/// Parses a string containing "Conflict Marker" style diffs (<<<<, ====, >>>>).
///
/// This format is common in Git merge conflicts or AI-generated code suggestions.
/// Since this format typically lacks file headers, the resulting [`Patch`] objects
/// will have a generic file path (`patch_target`).
///
/// **Warning:** Because this format lacks target file information, it is
/// generally unsuitable for batch-applying patches to a directory unless
/// the target file is manually specified or renamed.
///
/// This function treats text outside the markers as context lines, text between
/// `<<<<` and `====` as deletions, and text between `====` and `>>>>` as additions.
///
/// For automatic format detection, use [`parse_auto()`].
///
/// # Arguments
///
/// * `content` - A string slice containing the conflict marker content.
///
/// # Returns
///
/// A vector of [`Patch`] objects parsed from the conflict markers.
///
/// # Examples
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
    debug!(
        "Starting to parse conflict marker content ({} bytes).",
        content.len()
    );
    let patches = parse_conflict_markers_from_lines(content.lines());
    debug!(
        "Finished parsing conflict markers. Found {} patch(es).",
        patches.len()
    );
    patches
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
/// # Returns
///
/// A `Result` containing a vector of [`Patch`] objects on success.
///
/// # Errors
///
/// Returns `Err(`[`ParseError::MissingFileHeader`]`)` if the content contains patch
/// hunks but no `--- a/path/to/file` header. The `line` number in the error will
/// correspond to the first hunk header (e.g., `@@ ... @@`) found.
///
/// # Examples
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
    debug!("parse_patches_from_lines: reading diff lines...");
    let mut unmerged_patches: Vec<Patch> = Vec::new();
    const HUNK_BUFFER_CAPACITY: usize = 32;

    // State variables for the parser as it moves through the diff block.
    let mut first_hunk_header_line: Option<usize> = None;
    let mut current_file: Option<PathBuf> = None;
    let mut current_hunks: Vec<Hunk> = Vec::new();
    let mut current_hunk_lines: Vec<String> = Vec::with_capacity(HUNK_BUFFER_CAPACITY);
    let mut current_hunk_old_start_line: Option<usize> = None;
    let mut current_hunk_new_start_line: Option<usize> = None;
    let mut ends_with_newline_for_section = true;

    macro_rules! finalize_hunk {
        () => {
            if current_hunk_old_start_line.is_some() {
                trace!(
                    "    Finalizing previous hunk with {} lines.",
                    current_hunk_lines.len()
                );
                // Strip trailing empty context lines (often artifacts of spacing between diffs)
                while let Some(last) = current_hunk_lines.last() {
                    if last.trim().is_empty() {
                        current_hunk_lines.pop();
                    } else {
                        break;
                    }
                }
                current_hunks.push(Hunk {
                    lines: std::mem::replace(
                        &mut current_hunk_lines,
                        Vec::with_capacity(HUNK_BUFFER_CAPACITY),
                    ),
                    old_start_line: current_hunk_old_start_line,
                    new_start_line: current_hunk_new_start_line,
                });
            }
        };
    }

    for (line_idx, line) in lines.enumerate() {
        if let Some(stripped_line) = line.strip_prefix("--- ") {
            trace!("  Found file header line: '{}'", line);
            // A `---` line always signals a new file section.
            // Finalize the previous file's patch section if it exists.
            if let Some(existing_file) = &current_file {
                finalize_hunk!();
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
            finalize_hunk!();
            if first_hunk_header_line.is_none() {
                first_hunk_header_line = Some(line_idx + 1);
            }
            let (old, new) = parse_hunk_header(line);
            debug!(
                "  Hunk header at line {}: '{}' -> old_start={:?}, new_start={:?}",
                line_idx + 1,
                line,
                old,
                new
            );
            current_hunk_old_start_line = old;
            current_hunk_new_start_line = new;
        } else if line.starts_with(['+', '-', ' ']) {
            // Only treat this as a hunk line if we're actually inside a hunk.
            if current_hunk_old_start_line.is_some() {
                trace!(
                    "    Hunk line [{}]: '{}'",
                    current_hunk_lines.len() + 1,
                    line
                );
                current_hunk_lines.push(line.to_string());
            } else {
                trace!("    Ignored diff line outside hunk: '{}'", line);
            }
        } else if line.starts_with('\\') {
            // This line only makes sense inside a hunk.
            if current_hunk_old_start_line.is_some() {
                trace!(
                    "  Found '\\ No newline at end of file' marker on line {}.",
                    line_idx + 1
                );
                if let Some(last_line) = current_hunk_lines.last() {
                    if last_line.starts_with('+') || last_line.starts_with(' ') {
                        ends_with_newline_for_section = false;
                        trace!("    Recorded ends_with_newline=false for section");
                    } else {
                        trace!("    Marker '\\ No newline' ignored because preceding line was not '+' or ' '");
                    }
                }
            } else {
                trace!("    Ignored '\\' line outside hunk: '{}'", line);
            }
        } else if is_git_header_line(line) {
            trace!("  Ignoring Git header line: '{}'", line.trim_end());
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
    finalize_hunk!();

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
        debug!("parse_patches_from_lines: completed. 0 patch sections found.");
        return Ok(vec![]);
    }
    if unmerged_patches.len() == 1 {
        debug!(
            "parse_patches_from_lines: completed. Single patch section for '{}' with {} hunk(s).",
            unmerged_patches[0].file_path.display(),
            unmerged_patches[0].hunks.len()
        );
        return Ok(unmerged_patches);
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

    debug!(
        "parse_patches_from_lines: completed with {} merged patch(es).",
        merged_patches.len()
    );
    Ok(merged_patches)
}

/// Checks if a line is a standard Git diff header that should be ignored when parsing hunks.
///
/// Git extended diff output frequently includes metadata headers (such as `index`,
/// `old mode`, `new file mode`, etc.) between file diffs. This function checks whether
/// `line` matches any known Git header prefix so it is not incorrectly absorbed as
/// hunk context.
///
/// # Arguments
///
/// * `line` - The line to inspect.
///
/// # Returns
///
/// `true` if the line starts with a recognized Git header prefix, `false` otherwise.
fn is_git_header_line(line: &str) -> bool {
    const GIT_PREFIXES: &[&str] = &[
        "diff --git",
        "index ",
        "old mode ",
        "new mode ",
        "new file mode ",
        "deleted file mode ",
        "similarity index ",
        "copy from ",
        "copy to ",
        "rename from ",
        "rename to ",
    ];
    GIT_PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// Parses an iterator of lines containing "Conflict Marker" style diffs.
///
/// Scans through lines looking for `<<<<`, `====`, and `>>>>` delimiters.
/// Content before `====` becomes deletions (`-`), and content after `====`
/// becomes additions (`+`). Lines outside the markers become context (` `).
///
/// See [`parse_conflict_markers`] for details.
///
/// # Arguments
///
/// * `lines` - An iterator yielding string slices for each line.
///
/// # Returns
///
/// A vector containing a single [`Patch`] targeting `patch_target` if valid conflict
/// markers were found, or an empty vector otherwise.
fn parse_conflict_markers_from_lines<'a, I>(lines: I) -> Vec<Patch>
where
    I: Iterator<Item = &'a str>,
{
    trace!("parse_conflict_markers_from_lines: scanning lines for conflict markers");
    let mut hunk_lines = Vec::new();
    let mut has_start = false;
    let mut has_middle_or_end = false;

    enum State {
        Context,
        Old,
        New,
    }
    let mut state = State::Context;

    for line in lines {
        if line.trim_start().starts_with("<<<<") {
            trace!("  Conflict marker start: '{}'", line.trim());
            state = State::Old;
            has_start = true;
            continue;
        } else if line.trim_start().starts_with("====") {
            trace!("  Conflict marker separator: '{}'", line.trim());
            state = State::New;
            if has_start {
                has_middle_or_end = true;
            }
            continue;
        } else if line.trim_start().starts_with(">>>>") {
            trace!("  Conflict marker end: '{}'", line.trim());
            state = State::Context;
            if has_start {
                has_middle_or_end = true;
            }
            continue;
        }

        let prefix = match state {
            State::Context => ' ',
            State::Old => '-',
            State::New => '+',
        };
        let mut s = String::with_capacity(line.len() + 1);
        s.push(prefix);
        s.push_str(line);
        hunk_lines.push(s);
    }

    if !(has_start && has_middle_or_end) {
        trace!(
            "  Conflict markers incomplete or missing (has_start={}, has_middle_or_end={}). No patches created.",
            has_start,
            has_middle_or_end
        );
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
    debug!(
        "  Created conflict marker patch with {} line(s)",
        hunk.lines.len()
    );
    // The user can override this or use `patch_content_str` where it doesn't matter.
    vec![Patch {
        file_path: PathBuf::from("patch_target"),
        hunks: vec![hunk],
        ends_with_newline: true, // Assumption
    }]
}

/// Converts a [`std::io::Error`] into a more specific [`PatchError`].
///
/// Distinguishes between permission errors, directory target errors, and general I/O errors,
/// attaching the relevant file path to the error variant.
///
/// # Arguments
///
/// * `path` - The path that was being accessed when the error occurred.
/// * `e` - The underlying standard I/O error.
///
/// # Returns
///
/// The mapped [`PatchError`] instance.
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
/// The safe, canonicalized, absolute path of the target if validation succeeds.
///
/// # Errors
///
/// - Returns `Err(`[`PatchError::PathTraversal`]`)` if the path resolves outside the `base_dir`.
/// - Returns `Err(`[`PatchError::Io`]`)` if an I/O error occurs during path canonicalization (e.g., `base_dir` does not exist).
///
/// # Examples
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
    trace!(
        "  ensure_path_is_safe: canonicalized base directory '{}'",
        base_path.display()
    );

    // Lexical check to prevent arbitrary directory creation outside base_dir
    let mut virtual_path = base_path.clone();
    for component in relative_path.components() {
        trace!(
            "  ensure_path_is_safe: processing component '{:?}' on virtual path '{}'",
            component,
            virtual_path.display()
        );
        match component {
            std::path::Component::ParentDir => {
                if !virtual_path.pop() || !virtual_path.starts_with(&base_path) {
                    warn!(
                        "Path safety violation: relative path '{}' escapes base directory '{}'",
                        relative_path.display(),
                        base_dir.display()
                    );
                    return Err(PatchError::PathTraversal(relative_path.to_path_buf()));
                }
            }
            std::path::Component::Normal(c) => {
                virtual_path.push(c);
                // Resolve symlinks for existing components to prevent traversal via symlink.
                // If it doesn't exist yet, we just append it lexically (safe because
                // non-existent paths cannot be malicious symlinks).
                if fs::symlink_metadata(&virtual_path).is_ok() {
                    virtual_path = fs::canonicalize(&virtual_path)
                        .map_err(|e| map_io_error(virtual_path.clone(), e))?;
                    if !virtual_path.starts_with(&base_path) {
                        warn!(
                            "Path safety violation: symlink in '{}' resolves outside base directory '{}'",
                            relative_path.display(),
                            base_dir.display()
                        );
                        return Err(PatchError::PathTraversal(relative_path.to_path_buf()));
                    }
                }
            }
            std::path::Component::CurDir => {
                trace!("    ensure_path_is_safe: CurDir (.) ignored");
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                warn!(
                    "Path safety violation: path '{}' contains root or prefix component outside base directory '{}'",
                    relative_path.display(),
                    base_dir.display()
                );
                return Err(PatchError::PathTraversal(relative_path.to_path_buf()));
            }
        }
    }

    trace!(
        "  Path safety verified: '{}' safely resolves to '{}'",
        relative_path.display(),
        virtual_path.display()
    );
    Ok(virtual_path)
}

/// A convenience function that applies a slice of [`Patch`] objects to a target directory.
///
/// This is a high-level convenience function that iterates through a list of
/// patches and applies each one to the filesystem using [`apply_patch_to_file()`].
/// It aggregates the results, including both successful applications and any
/// "hard" errors encountered (like I/O errors). Files resulting in empty content
/// will be deleted.
///
/// This function will continue applying patches even if some fail.
///
/// # Arguments
///
/// * `patches` - A slice of [`Patch`] objects to apply.
/// * `target_dir` - The base directory where the patches should be applied.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// A [`BatchResult`] containing the results of each individual patch operation.
///
/// # Examples
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
    debug!(
        "apply_patches_to_dir: applying {} patch(es) to '{}' (dry_run={}, fuzz={:.2})",
        patches.len(),
        target_dir.display(),
        options.dry_run,
        options.fuzz_factor
    );
    let results: Vec<(PathBuf, Result<PatchResult, PatchError>)> = patches
        .iter()
        .enumerate()
        .map(|(idx, patch)| {
            debug!(
                "  [{}/{}] Applying patch for '{}' ({} hunk(s))",
                idx + 1,
                patches.len(),
                patch.file_path.display(),
                patch.hunks.len()
            );
            let result = apply_patch_to_file(patch, target_dir, options);
            (patch.file_path.clone(), result)
        })
        .collect();

    let batch = BatchResult { results };
    debug!(
        "apply_patches_to_dir: completed {} patch(es). all_succeeded={}, all_applied_cleanly={}",
        batch.results.len(),
        batch.all_succeeded(),
        batch.all_applied_cleanly()
    );
    batch
}

/// Internal representation of applied file actions used during atomic disk commit rollback.
#[derive(Debug)]
enum AppliedCommitAction {
    Created(PathBuf),
    Overwritten { path: PathBuf, prev_content: String },
    Deleted { path: PathBuf, prev_content: String },
}

/// Rolls back committed filesystem actions in reverse order upon encountering an unexpected I/O error.
fn rollback_committed_actions(actions: Vec<AppliedCommitAction>) {
    warn!(
        "Rolling back {} committed filesystem action(s)...",
        actions.len()
    );
    for action in actions.into_iter().rev() {
        match action {
            AppliedCommitAction::Created(path) => {
                trace!("Rollback: removing created file '{}'", path.display());
                if let Err(e) = fs::remove_file(&path) {
                    warn!(
                        "Rollback failed to remove created file '{}': {}",
                        path.display(),
                        e
                    );
                }
            }
            AppliedCommitAction::Overwritten { path, prev_content }
            | AppliedCommitAction::Deleted { path, prev_content } => {
                trace!(
                    "Rollback: restoring previous content to '{}' ({} bytes)",
                    path.display(),
                    prev_content.len()
                );
                if let Err(e) = fs::write(&path, prev_content) {
                    warn!(
                        "Rollback failed to restore file '{}': {}",
                        path.display(),
                        e
                    );
                }
            }
        }
    }
}

/// Applies a slice of [`Patch`] objects to a target directory atomically.
///
/// Unlike [`apply_patches_to_dir()`], this function stages all file modifications in-memory
/// first. Changes are committed to disk **if and only if all hunks across all patches apply
/// cleanly without errors**.
///
/// If any hunk in any patch fails, or if any hard error (such as a missing target file or path
/// traversal attempt) occurs:
/// - **Zero files on disk are modified, created, or deleted.**
/// - The filesystem remains in its exact, original state.
/// - The returned [`BatchResult`] details the status of every hunk and patch.
///
/// # Arguments
///
/// * `patches` - A slice of [`Patch`] objects to apply.
/// * `target_dir` - The base directory where patches should be applied.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// A [`BatchResult`] containing the outcome of each patch operation.
///
/// # Examples
///
/// ```rust
/// # use mpatch::{parse_auto, apply_patches_to_dir_atomic, ApplyOptions};
/// # use std::fs;
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let file1 = dir.path().join("file1.txt");
/// let file2 = dir.path().join("file2.txt");
/// fs::write(&file1, "foo\n")?;
/// fs::write(&file2, "bar\n")?;
///
/// // Second patch has invalid context and will fail.
/// let diff = r#"
/// --- a/file1.txt
/// +++ b/file1.txt
/// @@ -1 +1 @@
/// -foo
/// +foo_updated
/// --- a/file2.txt
/// +++ b/file2.txt
/// @@ -1 +1 @@
/// -WRONG_CONTEXT
/// +bar_updated
/// "#;
/// let patches = parse_auto(diff)?;
/// let options = ApplyOptions::exact();
///
/// let batch = apply_patches_to_dir_atomic(&patches, dir.path(), options);
///
/// // Because patch 2 failed, neither file was modified on disk!
/// assert!(!batch.all_applied_cleanly());
/// assert_eq!(fs::read_to_string(&file1)?, "foo\n");
/// assert_eq!(fs::read_to_string(&file2)?, "bar\n");
/// # Ok(())
/// # }
/// ```
pub fn apply_patches_to_dir_atomic(
    patches: &[Patch],
    target_dir: &Path,
    options: ApplyOptions,
) -> BatchResult {
    debug!(
        "Applying {} patch(es) to '{}' in atomic mode (dry_run={}, fuzz={:.2}).",
        patches.len(),
        target_dir.display(),
        options.dry_run,
        options.fuzz_factor
    );

    struct StagedFile {
        safe_path: PathBuf,
        existed_on_disk: bool,
        original_disk_content: Option<String>,
        current_content: Option<String>,
    }

    let mut staged_files: HashMap<PathBuf, StagedFile> = HashMap::new();
    let mut results: Vec<(PathBuf, Result<PatchResult, PatchError>)> =
        Vec::with_capacity(patches.len());

    for (idx, patch) in patches.iter().enumerate() {
        debug!(
            "  [Atomic Stage {}/{}] Staging '{}' ({} hunks)...",
            idx + 1,
            patches.len(),
            patch.file_path.display(),
            patch.hunks.len()
        );
        let safe_target_path = match ensure_path_is_safe(target_dir, &patch.file_path) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "  Atomic apply: path safety check failed for '{}': {}",
                    patch.file_path.display(),
                    e
                );
                results.push((patch.file_path.clone(), Err(e)));
                continue;
            }
        };

        if safe_target_path.is_dir() {
            warn!(
                "  Atomic apply: target path '{}' is a directory",
                safe_target_path.display()
            );
            results.push((
                patch.file_path.clone(),
                Err(PatchError::TargetIsDirectory {
                    path: safe_target_path,
                }),
            ));
            continue;
        }

        // Retrieve existing staged state or read from disk
        let staged = match staged_files.entry(safe_target_path.clone()) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                let (existed, content) = if safe_target_path.is_file() {
                    match fs::read_to_string(&safe_target_path) {
                        Ok(c) => {
                            trace!(
                                "    Read {} bytes from '{}'",
                                c.len(),
                                safe_target_path.display()
                            );
                            (true, Some(c))
                        }
                        Err(e) => {
                            warn!("    Failed to read '{}': {}", safe_target_path.display(), e);
                            results.push((
                                patch.file_path.clone(),
                                Err(map_io_error(safe_target_path.clone(), e)),
                            ));
                            continue;
                        }
                    }
                } else {
                    (false, None)
                };
                let curr = content.clone();
                entry.insert(StagedFile {
                    safe_path: safe_target_path.clone(),
                    existed_on_disk: existed,
                    original_disk_content: content,
                    current_content: curr,
                })
            }
        };

        if staged.current_content.is_none() && !patch.is_creation() {
            warn!(
                "  Atomic apply: target file '{}' not found and patch is not file creation",
                patch.file_path.display()
            );
            results.push((
                patch.file_path.clone(),
                Err(PatchError::TargetNotFound(
                    target_dir.join(&patch.file_path),
                )),
            ));
            continue;
        }

        let original_before_patch = staged.current_content.clone();
        trace!(
            "  Atomic apply: applying patch to staged content for '{}' (original len: {} bytes)",
            patch.file_path.display(),
            original_before_patch.as_deref().map_or(0, |s| s.len())
        );
        let in_memory_res =
            apply_patch_to_content(patch, original_before_patch.as_deref(), &options);
        if in_memory_res.report.all_applied_cleanly() {
            debug!(
                "  Atomic apply: staged changes for '{}' cleanly ({} hunks applied)",
                patch.file_path.display(),
                in_memory_res.report.success_count()
            );
        } else {
            warn!(
                "  Atomic apply: staged changes for '{}' had {} failure(s)",
                patch.file_path.display(),
                in_memory_res.report.failure_count()
            );
        }

        let mut diff = None;
        if options.dry_run {
            let a_path = format!("a/{}", patch.file_path.display());
            let b_path = format!("b/{}", patch.file_path.display());
            let diff_text = TextDiff::from_lines(
                original_before_patch.as_deref().unwrap_or(""),
                &in_memory_res.new_content,
            )
            .unified_diff()
            .context_radius(3)
            .header(&a_path, &b_path)
            .to_string();
            diff = Some(diff_text);
        }

        staged.current_content = Some(in_memory_res.new_content);

        results.push((
            patch.file_path.clone(),
            Ok(PatchResult {
                report: in_memory_res.report,
                diff,
            }),
        ));
    }

    // Check if every patch succeeded without hard error and all hunks applied cleanly
    let all_applied_cleanly = results
        .iter()
        .all(|(_, res)| res.as_ref().is_ok_and(|r| r.report.all_applied_cleanly()));

    debug!(
        "apply_patches_to_dir_atomic: evaluated all patches. all_applied_cleanly={}, dry_run={}, staged files count={}",
        all_applied_cleanly,
        options.dry_run,
        staged_files.len()
    );

    if !all_applied_cleanly || options.dry_run || staged_files.is_empty() {
        if options.dry_run {
            info!(
                "  DRY RUN (atomic): Evaluated {} patch(es). No files modified on disk.",
                patches.len()
            );
        } else if !all_applied_cleanly {
            info!("  ATOMIC APPLY ABORTED: Not all patches or hunks applied cleanly. Zero files modified on disk.");
        }
        return BatchResult { results };
    }

    // Commit phase: Write all staged files to disk in deterministic order
    info!(
        "  Atomic check passed! Committing changes for {} file(s) to disk...",
        staged_files.len()
    );

    let mut staged_list: Vec<_> = staged_files.into_values().collect();
    staged_list.sort_by(|a, b| a.safe_path.cmp(&b.safe_path));

    let mut applied_actions: Vec<AppliedCommitAction> = Vec::new();

    for staged in staged_list {
        let new_content = staged.current_content.unwrap_or_default();
        if new_content.is_empty() {
            if staged.existed_on_disk && staged.safe_path.exists() {
                debug!(
                    "  Atomic commit: removing empty file '{}'",
                    staged.safe_path.display()
                );
                if let Err(e) = fs::remove_file(&staged.safe_path) {
                    warn!(
                        "  I/O error removing file '{}' during atomic commit: {}. Rolling back...",
                        staged.safe_path.display(),
                        e
                    );
                    rollback_committed_actions(applied_actions);
                    let err_kind = e.kind();
                    let err_str = e.to_string();
                    for (path, res) in &mut results {
                        if ensure_path_is_safe(target_dir, path).ok().as_ref()
                            == Some(&staged.safe_path)
                        {
                            *res = Err(map_io_error(
                                staged.safe_path.clone(),
                                std::io::Error::new(err_kind, err_str.clone()),
                            ));
                        }
                    }
                    return BatchResult { results };
                }
                applied_actions.push(AppliedCommitAction::Deleted {
                    path: staged.safe_path,
                    prev_content: staged.original_disk_content.unwrap_or_default(),
                });
            }
        } else {
            if let Some(parent) = staged.safe_path.parent() {
                trace!(
                    "  Atomic commit: ensuring parent dir '{}'",
                    parent.display()
                );
                if let Err(e) = fs::create_dir_all(parent) {
                    warn!("  I/O error creating parent directory '{}' during atomic commit: {}. Rolling back...", parent.display(), e);
                    rollback_committed_actions(applied_actions);
                    let err_kind = e.kind();
                    let err_str = e.to_string();
                    for (path, res) in &mut results {
                        if ensure_path_is_safe(target_dir, path).ok().as_ref()
                            == Some(&staged.safe_path)
                        {
                            *res = Err(map_io_error(
                                parent.to_path_buf(),
                                std::io::Error::new(err_kind, err_str.clone()),
                            ));
                        }
                    }
                    return BatchResult { results };
                }
            }

            let was_file = staged.existed_on_disk;
            let prev = staged.original_disk_content;

            debug!(
                "  Atomic commit: writing {} bytes to '{}'",
                new_content.len(),
                staged.safe_path.display()
            );
            if let Err(e) = fs::write(&staged.safe_path, &new_content) {
                warn!(
                    "  I/O error writing file '{}' during atomic commit: {}. Rolling back...",
                    staged.safe_path.display(),
                    e
                );
                rollback_committed_actions(applied_actions);
                let err_kind = e.kind();
                let err_str = e.to_string();
                for (path, res) in &mut results {
                    if ensure_path_is_safe(target_dir, path).ok().as_ref()
                        == Some(&staged.safe_path)
                    {
                        *res = Err(map_io_error(
                            staged.safe_path.clone(),
                            std::io::Error::new(err_kind, err_str.clone()),
                        ));
                    }
                }
                return BatchResult { results };
            }

            if was_file {
                debug!(
                    "  Atomic commit: overwritten existing file '{}' (previous len: {} bytes)",
                    staged.safe_path.display(),
                    staged.safe_path.display()
                );
                applied_actions.push(AppliedCommitAction::Overwritten {
                    path: staged.safe_path,
                    prev_content: prev.unwrap_or_default(),
                });
            } else {
                debug!(
                    "  Atomic commit: created new file '{}'",
                    staged.safe_path.display()
                );
                applied_actions.push(AppliedCommitAction::Created(staged.safe_path));
            }
        }
    }

    info!("  Successfully committed all atomic changes to disk.");
    BatchResult { results }
}

/// A strict variant of [`apply_patches_to_dir_atomic()`] that treats partial applications as an error.
///
/// # Arguments
///
/// * `patches` - A slice of [`Patch`] objects to apply.
/// * `target_dir` - The base directory where patches should be applied.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// `Ok(`[`BatchResult`]`)` if all patches and hunks applied cleanly.
///
/// # Errors
///
/// Returns `Err(`[`StrictBatchApplyError::Failed`]`)` if any patch had a hard error or any hunk failed.
///
/// # Examples
///
/// ```rust
/// # use mpatch::{parse_auto, try_apply_patches_to_dir_atomic, ApplyOptions};
/// # use tempfile::tempdir;
/// # use std::fs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let file_path = dir.path().join("data.txt");
/// fs::write(&file_path, "value = 1\n")?;
///
/// let diff = "--- a/data.txt\n+++ b/data.txt\n@@ -1 +1 @@\n-value = 1\n+value = 2\n";
/// let patches = parse_auto(diff)?;
/// let batch = try_apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::exact())?;
///
/// assert!(batch.all_applied_cleanly());
/// assert_eq!(fs::read_to_string(&file_path)?, "value = 2\n");
/// # Ok(())
/// # }
/// ```
pub fn try_apply_patches_to_dir_atomic(
    patches: &[Patch],
    target_dir: &Path,
    options: ApplyOptions,
) -> Result<BatchResult, StrictBatchApplyError> {
    debug!(
        "try_apply_patches_to_dir_atomic: applying {} patch(es) strictly and atomically",
        patches.len()
    );
    let result = apply_patches_to_dir_atomic(patches, target_dir, options);
    if result.all_applied_cleanly() {
        debug!("try_apply_patches_to_dir_atomic: all patches and hunks applied cleanly");
        Ok(result)
    } else {
        warn!("try_apply_patches_to_dir_atomic: strict atomic batch application failed");
        Err(StrictBatchApplyError::Failed {
            batch_result: result,
        })
    }
}

/// A strict variant of [`apply_patches_to_dir()`] that treats partial applications as an error.
///
/// # Arguments
///
/// * `patches` - A slice of [`Patch`] objects to apply.
/// * `target_dir` - The base directory where patches should be applied.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// `Ok(`[`BatchResult`]`)` if all patches and hunks applied cleanly.
///
/// # Errors
///
/// Returns `Err(`[`StrictBatchApplyError::Failed`]`)` if any patch had a hard error or any hunk failed.
///
/// # Examples
///
/// ```rust
/// # use mpatch::{parse_auto, try_apply_patches_to_dir, ApplyOptions};
/// # use tempfile::tempdir;
/// # use std::fs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let file_path = dir.path().join("server.txt");
/// fs::write(&file_path, "port = 8080\n")?;
///
/// let diff = "--- a/server.txt\n+++ b/server.txt\n@@ -1 +1 @@\n-port = 8080\n+port = 9090\n";
/// let patches = parse_auto(diff)?;
/// let batch = try_apply_patches_to_dir(&patches, dir.path(), ApplyOptions::exact())?;
///
/// assert!(batch.all_applied_cleanly());
/// assert_eq!(fs::read_to_string(&file_path)?, "port = 9090\n");
/// # Ok(())
/// # }
/// ```
pub fn try_apply_patches_to_dir(
    patches: &[Patch],
    target_dir: &Path,
    options: ApplyOptions,
) -> Result<BatchResult, StrictBatchApplyError> {
    debug!(
        "try_apply_patches_to_dir: applying {} patch(es) strictly",
        patches.len()
    );
    let result = apply_patches_to_dir(patches, target_dir, options);
    if result.all_applied_cleanly() {
        debug!("try_apply_patches_to_dir: all patches applied cleanly");
        Ok(result)
    } else {
        warn!("try_apply_patches_to_dir: strict batch application failed");
        Err(StrictBatchApplyError::Failed {
            batch_result: result,
        })
    }
}

/// Inverts a list of patches.
///
/// This is a convenience function that calls [`Patch::invert()`] on every patch
/// in the provided slice. It is useful when you want to reverse the effect of
/// a multi-file diff (e.g., "un-applying" a set of changes).
///
/// # Arguments
///
/// * `patches` - A slice of [`Patch`] objects to invert.
///
/// # Returns
///
/// A new vector of [`Patch`] objects with their changes reversed.
///
/// # Examples
///
/// ```
/// # use mpatch::{parse_auto, invert_patches};
/// let diff = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new";
/// let patches = parse_auto(diff).unwrap();
///
/// let reversed = invert_patches(&patches);
/// let hunk = &reversed[0].hunks[0];
///
/// assert_eq!(hunk.removed_lines(), vec!["new"]);
/// assert_eq!(hunk.added_lines(), vec!["old"]);
/// ```
pub fn invert_patches(patches: &[Patch]) -> Vec<Patch> {
    debug!("invert_patches: inverting {} patch(es)", patches.len());
    patches.iter().map(|p| p.invert()).collect()
}

/// A convenience function that applies a single [`Patch`] to the filesystem.
///
/// This function orchestrates the patching process for a single file. It handles
/// filesystem interactions like reading the original file and writing the new
/// content, while delegating the core patching logic to [`apply_patch_to_content()`].
/// If the patch results in empty content, the target file is deleted.
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
/// A [`PatchResult`] on success. The `PatchResult` contains a detailed report
/// for each hunk and, if `dry_run` was enabled, a diff of the proposed changes.
/// If some hunks failed, the file may be in a partially patched state (unless
/// in dry-run mode).
///
/// # Errors
///
/// Returns `Err(`[`PatchError`]`)` for "hard" errors like I/O problems, path traversal violations,
/// or a missing target file.
///
/// # Examples
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
    debug!(
        "  apply_patch_to_file: target_dir='{}', hunks={}, dry_run={}, fuzz={:.2}",
        target_dir.display(),
        patch.hunks.len(),
        options.dry_run,
        options.fuzz_factor
    );

    // --- Path Safety Check ---
    // This is a critical security measure. `ensure_path_is_safe` returns a
    // canonicalized, absolute path that is confirmed to be inside the target_dir.
    let safe_target_path = ensure_path_is_safe(target_dir, &patch.file_path)?;
    debug!(
        "  Resolved safe target path: '{}'",
        safe_target_path.display()
    );

    // --- Read Original File ---
    // All subsequent operations use the verified `safe_target_path`.
    if safe_target_path.is_dir() {
        warn!(
            "  Target path '{}' is a directory, not a file.",
            safe_target_path.display()
        );
        return Err(PatchError::TargetIsDirectory {
            path: safe_target_path,
        });
    }

    let (original_content, is_new_file) = if safe_target_path.is_file() {
        debug!(
            "  Target file exists: '{}'. Reading content...",
            safe_target_path.display()
        );
        let content = fs::read_to_string(&safe_target_path)
            .map_err(|e| map_io_error(safe_target_path.clone(), e))?;
        trace!(
            "    Read {} bytes ({} lines) from target file.",
            content.len(),
            content.lines().count()
        );
        (content, false)
    } else {
        // File doesn't exist. This is only okay if it's a file creation patch.
        if !patch.is_creation() {
            debug!(
                "  Target file '{}' does not exist, and patch is not a creation patch. Aborting.",
                safe_target_path.display()
            );
            // For user-facing errors, show the original path, not the canonicalized one.
            return Err(PatchError::TargetNotFound(
                target_dir.join(&patch.file_path),
            ));
        }
        debug!(
            "  Target file '{}' does not exist. Assuming file creation.",
            safe_target_path.display()
        );
        (String::new(), true)
    };

    // --- Apply Patch to Content ---
    debug!("  Applying patch logic to content in-memory...");
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
            "  DRY RUN: Evaluated changes for '{}' ({} hunks, clean={})",
            patch.file_path.display(),
            patch.hunks.len(),
            apply_result.all_applied_cleanly()
        );
        trace!("  Generating diff for dry run...");

        let a_path = format!("a/{}", patch.file_path.display());
        let b_path = format!("b/{}", patch.file_path.display());
        let diff_text = TextDiff::from_lines(&original_content, &new_content)
            .unified_diff()
            .context_radius(3)
            .header(&a_path, &b_path)
            .to_string();
        diff = Some(diff_text);
    } else {
        // Write the modified content to the file system.
        // The parent directory might have been created by `ensure_path_is_safe`
        // for a new file, but we ensure it again just in case.
        if new_content.is_empty() {
            if safe_target_path.exists() {
                info!(
                    "  Resulting content is empty. Removing file '{}'",
                    patch.file_path.display()
                );
                fs::remove_file(&safe_target_path)
                    .map_err(|e| map_io_error(safe_target_path.clone(), e))?;
                debug!(
                    "  Successfully deleted empty file '{}'",
                    safe_target_path.display()
                );
            } else {
                info!(
                    "  Resulting content is empty. Skipping creation of '{}'",
                    patch.file_path.display()
                );
            }

            if apply_result.all_applied_cleanly() {
                info!(
                    "  Successfully processed deletion/empty-result for '{}'",
                    patch.file_path.display()
                );
            } else {
                warn!(
                    "  Partial application resulted in empty content for '{}'",
                    patch.file_path.display()
                );
            }
        } else {
            if let Some(parent) = safe_target_path.parent() {
                trace!("  Ensuring parent directory exists: '{}'", parent.display());
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
/// # Arguments
///
/// * `patch` - The [`Patch`] object to apply.
/// * `target_dir` - The base directory where the patch should be applied.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// A [`PatchResult`] if all hunks were applied successfully.
///
/// # Errors
///
/// - Returns `Err(`[`StrictApplyError::PartialApply`]`)` if some hunks failed to apply. The file may
///   be in a partially patched state (unless in dry-run mode). The `report` within
///   the error contains the detailed results.
/// - Returns `Err(`[`StrictApplyError::Patch`]`)` for "hard" errors like I/O problems or a missing target file.
///
/// # Examples
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
    debug!(
        "try_apply_patch_to_file: strictly applying patch for '{}'",
        patch.file_path.display()
    );
    let result = apply_patch_to_file(patch, target_dir, options)?;
    if result.report.all_applied_cleanly() {
        debug!(
            "try_apply_patch_to_file: all hunks applied cleanly for '{}'",
            patch.file_path.display()
        );
        Ok(result)
    } else {
        warn!(
            "try_apply_patch_to_file: partial apply for '{}' ({} failures)",
            patch.file_path.display(),
            result.report.failure_count()
        );
        Err(StrictApplyError::PartialApply {
            report: result.report,
        })
    }
}

/// Applies a single [`Patch`] to the filesystem atomically.
///
/// The target file is modified on disk **if and only if all hunks in the patch apply cleanly**.
/// If any hunk fails or encounters an error, the file on disk remains completely untouched.
///
/// # Arguments
///
/// * `patch` - The [`Patch`] object to apply.
/// * `target_dir` - The base directory where the patch should be applied.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// A [`PatchResult`] on success.
///
/// # Errors
///
/// Returns `Err(`[`PatchError`]`)` on hard errors like I/O problems or a missing target file.
///
/// # Examples
///
/// ````rust
/// # use mpatch::{parse_single_patch, apply_patch_to_file_atomic, ApplyOptions};
/// # use std::fs;
/// # use tempfile::tempdir;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let file_path = dir.path().join("partial.txt");
/// fs::write(&file_path, "line 1\nline 2\n")?;
///
/// // Second hunk will fail due to wrong context.
/// let diff = r#"
/// ```diff
/// --- a/partial.txt
/// +++ b/partial.txt
/// @@ -1,1 +1,1 @@
/// -line 1
/// +line one
/// @@ -2,1 +2,1 @@
/// -WRONG
/// +line two
/// ```
/// "#;
/// let patch = parse_single_patch(diff)?;
/// let options = ApplyOptions::exact();
///
/// let res = apply_patch_to_file_atomic(&patch, dir.path(), options)?;
/// assert!(!res.report.all_applied_cleanly());
///
/// // In atomic mode, partial changes were discarded; file remains untouched!
/// assert_eq!(fs::read_to_string(&file_path)?, "line 1\nline 2\n");
/// # Ok(())
/// # }
/// ````
pub fn apply_patch_to_file_atomic(
    patch: &Patch,
    target_dir: &Path,
    options: ApplyOptions,
) -> Result<PatchResult, PatchError> {
    debug!(
        "apply_patch_to_file_atomic: dispatching atomic patch for '{}' to directory '{}'",
        patch.file_path.display(),
        target_dir.display()
    );
    let mut batch = apply_patches_to_dir_atomic(std::slice::from_ref(patch), target_dir, options);
    let (_, res) = batch.results.remove(0);
    res
}

/// A strict variant of [`apply_patch_to_file_atomic()`] that treats partial applications as an error.
///
/// If any hunk fails to apply, the file on disk remains completely unmodified and an
/// `Err(`[`StrictApplyError::PartialApply`]`)` is returned.
///
/// # Arguments
///
/// * `patch` - The [`Patch`] object to apply.
/// * `target_dir` - The base directory where the patch should be applied.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// A [`PatchResult`] if all hunks applied cleanly.
///
/// # Errors
///
/// - Returns `Err(`[`StrictApplyError::PartialApply`]`)` if any hunk failed to apply. The file on disk
///   remains completely unmodified.
/// - Returns `Err(`[`StrictApplyError::Patch`]`)` for hard errors like I/O problems or a missing target file.
///
/// # Examples
///
/// ```rust
/// # use mpatch::{parse_single_patch, try_apply_patch_to_file_atomic, ApplyOptions};
/// # use tempfile::tempdir;
/// # use std::fs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let file_path = dir.path().join("config.toml");
/// fs::write(&file_path, "timeout = 30\n")?;
///
/// let diff = "--- a/config.toml\n+++ b/config.toml\n@@ -1 +1 @@\n-timeout = 30\n+timeout = 60\n";
/// let patch = parse_single_patch(diff)?;
/// let result = try_apply_patch_to_file_atomic(&patch, dir.path(), ApplyOptions::exact())?;
///
/// assert!(result.report.all_applied_cleanly());
/// assert_eq!(fs::read_to_string(&file_path)?, "timeout = 60\n");
/// # Ok(())
/// # }
/// ```
pub fn try_apply_patch_to_file_atomic(
    patch: &Patch,
    target_dir: &Path,
    options: ApplyOptions,
) -> Result<PatchResult, StrictApplyError> {
    debug!(
        "try_apply_patch_to_file_atomic: strictly applying atomic patch for '{}'",
        patch.file_path.display()
    );
    let result = apply_patch_to_file_atomic(patch, target_dir, options)?;
    if result.report.all_applied_cleanly() {
        debug!(
            "try_apply_patch_to_file_atomic: patch for '{}' applied cleanly",
            patch.file_path.display()
        );
        Ok(result)
    } else {
        warn!(
            "try_apply_patch_to_file_atomic: patch for '{}' failed ({} hunk failures)",
            patch.file_path.display(),
            result.report.failure_count()
        );
        Err(StrictApplyError::PartialApply {
            report: result.report,
        })
    }
}

/// Recursively searches for all strictly non-overlapping, monotonically increasing
/// assignment chains of candidate match indices across a sequence of unanchored hunks.
fn find_valid_chains(
    j: usize,
    min_pos: usize,
    hunk_lens: &[usize],
    cand_lists: &[Vec<usize>],
    current: &mut Vec<usize>,
    chains: &mut Vec<Vec<usize>>,
) {
    if chains.len() > 100 {
        trace!("  find_valid_chains: reached limit of 100 chains, truncating search");
        return;
    }
    if j == cand_lists.len() {
        trace!("  find_valid_chains: found valid chain: {:?}", current);
        chains.push(current.clone());
        return;
    }
    for &cand in &cand_lists[j] {
        if cand >= min_pos {
            current.push(cand);
            find_valid_chains(
                j + 1,
                cand + hunk_lens[j],
                hunk_lens,
                cand_lists,
                current,
                chains,
            );
            current.pop();
        }
    }
}

/// Soundly resolves line hints for hunks in a patch that lack `old_start_line` (such as Aider
/// search/replace blocks or conflict markers) using topological anchor interval bounding.
///
/// When multiple hunks target the same file, unambiguous hunks act as spatial anchors. An
/// intermediate hunk with identical/repetitive code is soundly resolved if and only if exactly
/// one match exists within the interval between its bounding anchors.
fn resolve_hunk_line_hints<T: AsRef<str>>(hunks: &[Hunk], lines: &[T]) -> Vec<Hunk> {
    let mut resolved = hunks.to_vec();
    if resolved.is_empty() || lines.is_empty() {
        trace!("resolve_hunk_line_hints: empty hunks or lines, returning immediately");
        return resolved;
    }

    let n = resolved.len();
    debug!(
        "resolve_hunk_line_hints: evaluating {} hunk(s) across {} target lines",
        n,
        lines.len()
    );
    let mut matches_per_hunk: Vec<Vec<usize>> = Vec::with_capacity(n);
    let mut anchors: Vec<Option<usize>> = vec![None; n];
    let match_lens: Vec<usize> = resolved
        .iter()
        .map(|h| h.lines.iter().filter(|l| !l.starts_with('+')).count())
        .collect();

    for (i, hunk) in resolved.iter_mut().enumerate() {
        let match_block = hunk.get_match_block();
        if match_block.is_empty() || is_low_entropy_segment(&match_block) {
            trace!(
                "  Hunk {}: empty or low-entropy match block, skipping initial anchor",
                i + 1
            );
            matches_per_hunk.push(Vec::new());
            continue;
        }

        if let Some(explicit_line) = hunk.old_start_line {
            trace!(
                "  Hunk {}: already has explicit line hint {}",
                i + 1,
                explicit_line
            );
            matches_per_hunk.push(vec![explicit_line.saturating_sub(1)]);
            anchors[i] = Some(explicit_line.saturating_sub(1));
            continue;
        }

        let mut exact_matches = Vec::new();
        if match_block.len() <= lines.len() {
            for (idx, window) in lines.windows(match_block.len()).enumerate() {
                if window
                    .iter()
                    .map(|s| s.as_ref())
                    .eq(match_block.iter().copied())
                {
                    exact_matches.push(idx);
                }
            }
            if exact_matches.is_empty() {
                let match_trimmed: Vec<&str> = match_block.iter().map(|s| s.trim_end()).collect();
                for (idx, window) in lines.windows(match_block.len()).enumerate() {
                    if window
                        .iter()
                        .map(|s| s.as_ref().trim_end())
                        .eq(match_trimmed.iter().copied())
                    {
                        exact_matches.push(idx);
                    }
                }
            }
        }

        trace!(
            "  Hunk {}: found {} exact/trimmed match(es)",
            i + 1,
            exact_matches.len()
        );
        if exact_matches.len() == 1 {
            debug!(
                "  Hunk {}: uniquely anchored at line {} (0-based: {})",
                i + 1,
                exact_matches[0] + 1,
                exact_matches[0]
            );
            anchors[i] = Some(exact_matches[0]);
            hunk.old_start_line = Some(exact_matches[0] + 1);
        } else if exact_matches.is_empty() {
            trace!(
                "  Hunk {}: 0 exact matches found during initial anchor scan",
                i + 1
            );
        } else {
            trace!(
                "  Hunk {}: {} candidate exact matches found (ambiguous without bounding)",
                i + 1,
                exact_matches.len()
            );
        }
        matches_per_hunk.push(exact_matches);
    }

    // Relaxation: Soundly bound intermediate ambiguous hunks between established anchors
    let mut changed = true;
    let mut iteration = 0;
    while changed {
        iteration += 1;
        trace!(
            "resolve_hunk_line_hints: beginning relaxation pass {}",
            iteration
        );
        changed = false;

        // Pass 1: Single unique bounded matches and contiguous anchor attachments
        for i in 0..n {
            if anchors[i].is_some() {
                continue;
            }
            let candidates = &matches_per_hunk[i];
            if candidates.is_empty() {
                continue;
            }

            let prev_anchor = (0..i).rev().find_map(|p| anchors[p].map(|pos| (p, pos)));
            let next_anchor = ((i + 1)..n).find_map(|s| anchors[s].map(|pos| (s, pos)));

            let (min_bound, max_bound) = match (prev_anchor, next_anchor) {
                (Some((p, p_pos)), Some((_s, s_pos))) => {
                    let p_len = match_lens[p];
                    (p_pos + p_len, s_pos)
                }
                (Some((p, p_pos)), None) => {
                    let p_len = match_lens[p];
                    (p_pos + p_len, lines.len())
                }
                (None, Some((_s, s_pos))) => (0, s_pos),
                (None, None) => (0, lines.len()),
            };

            if min_bound <= max_bound {
                let hunk_len = match_lens[i];
                let bounded_matches: Vec<usize> = candidates
                    .iter()
                    .copied()
                    .filter(|&m| m >= min_bound && m + hunk_len <= max_bound)
                    .collect();

                if bounded_matches.len() == 1 {
                    let unique_match = bounded_matches[0];
                    debug!(
                        "  Hunk {}: uniquely resolved between anchors to line {} (0-based: {}) in interval [{}..{}]",
                        i + 1,
                        unique_match + 1,
                        unique_match,
                        min_bound,
                        max_bound
                    );
                    anchors[i] = Some(unique_match);
                    resolved[i].old_start_line = Some(unique_match + 1);
                    changed = true;
                    continue;
                } else {
                    trace!(
                        "  Hunk {}: {} match(es) within interval [{}..{}]",
                        i + 1,
                        bounded_matches.len(),
                        min_bound,
                        max_bound
                    );
                }

                // Contiguity check: If hunk `i` is immediately adjacent to prev_anchor in the patch (`i == p + 1`),
                // and a bounded candidate starts right where prev_anchor ended (`m == min_bound`), anchor it.
                if let Some((p, _)) = prev_anchor {
                    if i == p + 1 && bounded_matches.contains(&min_bound) {
                        debug!(
                            "  Hunk {}: resolved via contiguity to prev anchor {} at line {}",
                            i + 1,
                            p + 1,
                            min_bound + 1
                        );
                        anchors[i] = Some(min_bound);
                        resolved[i].old_start_line = Some(min_bound + 1);
                        changed = true;
                        continue;
                    }
                }

                // Similarly, if hunk `i` immediately precedes next_anchor in the patch (`i + 1 == s`),
                // and a bounded candidate ends right where next_anchor begins (`m + hunk_len == max_bound`), anchor it.
                if let Some((s, _)) = next_anchor {
                    if i + 1 == s
                        && max_bound >= hunk_len
                        && bounded_matches.contains(&(max_bound - hunk_len))
                    {
                        let m = max_bound - hunk_len;
                        debug!(
                            "  Hunk {}: resolved via contiguity to next anchor {} at line {}",
                            i + 1,
                            s + 1,
                            m + 1
                        );
                        anchors[i] = Some(m);
                        resolved[i].old_start_line = Some(m + 1);
                        changed = true;
                        continue;
                    }
                }
            }
        }

        if changed {
            continue;
        }

        // Pass 2: Topological chain resolution across contiguous sequences of unanchored hunks
        let mut idx = 0;
        while idx < n {
            if anchors[idx].is_some() {
                idx += 1;
                continue;
            }
            let start = idx;
            while idx < n && anchors[idx].is_none() {
                idx += 1;
            }
            let end = idx;
            let unanchored_indices: Vec<usize> = (start..end).collect();
            if unanchored_indices.len() >= 2 {
                let prev_anchor = if start > 0 {
                    anchors[start - 1].map(|pos| (start - 1, pos))
                } else {
                    None
                };
                let next_anchor = if end < n {
                    anchors[end].map(|pos| (end, pos))
                } else {
                    None
                };

                let (min_bound, max_bound) = match (prev_anchor, next_anchor) {
                    (Some((p, p_pos)), Some((_s, s_pos))) => {
                        let p_len = match_lens[p];
                        (p_pos + p_len, s_pos)
                    }
                    (Some((p, p_pos)), None) => {
                        let p_len = match_lens[p];
                        (p_pos + p_len, lines.len())
                    }
                    (None, Some((_s, s_pos))) => (0, s_pos),
                    (None, None) => (0, lines.len()),
                };

                if min_bound <= max_bound {
                    let mut hunk_lens = Vec::with_capacity(unanchored_indices.len());
                    let mut cand_lists = Vec::with_capacity(unanchored_indices.len());
                    let mut all_have_candidates = true;

                    for &h in &unanchored_indices {
                        let h_len = match_lens[h];
                        let cands: Vec<usize> = matches_per_hunk[h]
                            .iter()
                            .copied()
                            .filter(|&m| m >= min_bound && m + h_len <= max_bound)
                            .collect();
                        if cands.is_empty() {
                            all_have_candidates = false;
                            break;
                        }
                        hunk_lens.push(h_len);
                        cand_lists.push(cands);
                    }

                    if all_have_candidates {
                        let mut chains = Vec::new();
                        let mut cur = Vec::with_capacity(unanchored_indices.len());
                        find_valid_chains(
                            0,
                            min_bound,
                            &hunk_lens,
                            &cand_lists,
                            &mut cur,
                            &mut chains,
                        );

                        if !chains.is_empty() {
                            trace!(
                                "  Found {} monotonic chain(s) for hunks {:?}",
                                chains.len(),
                                unanchored_indices
                            );
                            for (j, &h) in unanchored_indices.iter().enumerate() {
                                let pos0 = chains[0][j];
                                if chains.iter().all(|c| c[j] == pos0) {
                                    debug!(
                                        "  Hunk {}: invariant chain position resolved to line {}",
                                        h + 1,
                                        pos0 + 1
                                    );
                                    anchors[h] = Some(pos0);
                                    resolved[h].old_start_line = Some(pos0 + 1);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let anchored_count = anchors.iter().filter(|a| a.is_some()).count();
    debug!(
        "resolve_hunk_line_hints: completed hint resolution. {}/{} hunk(s) have anchors.",
        anchored_count, n
    );
    resolved
}

/// An iterator that applies hunks from a patch one by one.
///
/// This struct provides fine-grained control over the patch application process.
/// It allows you to apply hunks sequentially, inspect the intermediate state of
/// the content, and handle results on a per-hunk basis.
///
/// The iterator yields a [`HunkApplyStatus`] for each hunk in the patch.
///
/// # Examples
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
    /// An iterator over remaining hunks in the patch.
    hunks: Vec<Hunk>,
    current_idx: usize,
    /// Accumulated lines of the file content as hunks are applied.
    current_lines: Vec<String>,
    /// Patch application options governing fuzzy thresholds and behavior.
    options: &'a ApplyOptions,
    /// Whether the patch specifies that the file should end with a newline.
    patch_ends_with_newline: bool,
    /// Whether the original target content ended with a newline.
    original_ends_with_newline: bool,
    /// Tracks whether any applied hunk touched or modified the end of the file.
    touched_eof: bool,
    completed_edits: Vec<(usize, isize)>,
    /// 1-based line number in current_lines where the last hunk finished applying.
    last_applied_line: Option<usize>,
}

impl<'a> HunkApplier<'a> {
    /// Creates a new `HunkApplier` to begin a step-by-step patch operation.
    ///
    /// This constructor initializes the applier with the patch to be applied and the
    /// original content. The content is provided as an optional slice of lines,
    /// allowing for both file modifications (`Some(lines)`) and file creations (`None`).
    /// The applier can then be used as an iterator to apply hunks one by one.
    ///
    /// # Arguments
    ///
    /// * `patch` - The [`Patch`] to apply.
    /// * `original_lines` - An optional slice of strings representing the original content.
    /// * `options` - Configuration for the patch operation.
    ///
    /// # Returns
    ///
    /// A new `HunkApplier` instance.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-line 2\n+line two\n```";
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

        let hunks = if let Some(lines) = original_lines {
            resolve_hunk_line_hints(&patch.hunks, lines)
        } else {
            patch.hunks.clone()
        };

        debug!(
            "HunkApplier: initialized with {} hunk(s) across {} line(s) of target content (fuzz_factor={:.2}, dry_run={})",
            hunks.len(),
            current_lines.len(),
            options.fuzz_factor,
            options.dry_run
        );
        Self {
            hunks,
            current_idx: 0,
            current_lines,
            options,
            patch_ends_with_newline: patch.ends_with_newline,
            original_ends_with_newline: true,
            touched_eof: false,
            completed_edits: Vec::new(),
            last_applied_line: None,
        }
    }

    /// Returns a slice of the current lines, reflecting all hunks applied so far.
    ///
    /// This method provides read-only access to the intermediate state of the
    /// content being patched. It is useful for inspecting the content between
    /// applying hunks with the `HunkApplier` iterator.
    ///
    /// # Returns
    ///
    /// A slice of strings representing the current state of the content.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-line 2\n+line two\n```";
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

    /// Sets whether the original content ended with a newline.
    ///
    /// When working with a slice of lines (e.g., `Vec<String>`), the information about
    /// whether the original file ended with a newline character is typically lost.
    /// This method allows you to restore that context.
    ///
    /// `mpatch` uses this information to determine the newline status of the final output:
    /// 1. If a patch modifies the end of the file (i.e., the last hunk applies to the
    ///    very end), the patch's `ends_with_newline` setting takes precedence.
    /// 2. If the patch only modifies the middle of the file, the original newline
    ///    status is preserved.
    ///
    /// By default, `HunkApplier` assumes the original content ended with a newline (`true`).
    ///
    /// # Arguments
    ///
    /// * `ends_with_newline` - `true` if the original content had a trailing newline, `false` otherwise.
    ///
    /// # Returns
    ///
    /// None.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Original content: "line 1\nline 2" (No trailing newline)
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,1 +1,1\n-line 1\n+line one\n```";
    /// let patch = parse_single_patch(diff)?;
    /// let options = ApplyOptions::new();
    ///
    /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
    ///
    /// // Crucial step: Tell the applier the original file didn't have a newline.
    /// applier.set_original_newline_status(false);
    ///
    /// applier.next(); // Apply the hunk (modifies line 1)
    ///
    /// // The result should preserve the "no newline" status because the patch didn't touch EOF.
    /// let result = applier.into_content();
    /// assert_eq!(result, "line one\nline 2");
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_original_newline_status(&mut self, ends_with_newline: bool) {
        trace!(
            "HunkApplier::set_original_newline_status: original_ends_with_newline={}",
            ends_with_newline
        );
        self.original_ends_with_newline = ends_with_newline;
    }

    /// Consumes the applier and returns the final vector of lines.
    ///
    /// After iterating through the `HunkApplier` and applying all desired hunks,
    /// this method can be called to take ownership of the final, modified vector
    /// of strings.
    ///
    /// # Returns
    ///
    /// A vector of strings representing the final content.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-line 2\n+line two\n```";
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
        trace!(
            "HunkApplier::into_lines: returning {} line(s)",
            self.current_lines.len()
        );
        self.current_lines
    }

    /// Consumes the applier and returns the final content as a single string.
    ///
    /// This method joins the final lines with newlines and ensures the content
    /// has a trailing newline if required by the patch's `ends_with_newline`
    /// property. It is the most common way to get the final result from a
    /// `HunkApplier`.
    ///
    /// # Returns
    ///
    /// A single string representing the final content.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-line 1\n+line one\n```";
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
        debug!(
            "HunkApplier::into_content: assembling final content from {} line(s) (touched_eof={}, patch_ends_with_newline={}, original_ends_with_newline={})",
            self.current_lines.len(),
            self.touched_eof,
            self.patch_ends_with_newline,
            self.original_ends_with_newline
        );
        let mut new_content = self.current_lines.join("\n");

        let should_have_newline = if self.touched_eof {
            self.patch_ends_with_newline
        } else {
            self.original_ends_with_newline
        };

        if should_have_newline && !self.current_lines.is_empty() {
            new_content.push('\n');
        }
        trace!(
            "HunkApplier::into_content: resulting content has {} bytes ({} lines, ends_with_newline={})",
            new_content.len(),
            self.current_lines.len(),
            should_have_newline
        );
        new_content
    }
}

impl<'a> Iterator for HunkApplier<'a> {
    type Item = HunkApplyStatus;

    /// Applies the next hunk in the patch and returns its status.
    ///
    /// This method advances the iterator, applying one hunk to the internal state
    /// of the `HunkApplier`. It returns `Some(`[`HunkApplyStatus`]`)` for each hunk in
    /// the patch, and `None` when all hunks have been processed.
    ///
    /// # Returns
    ///
    /// An `Option` containing the [`HunkApplyStatus`] of the applied hunk, or `None` if there are no more hunks.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, HunkApplier, ApplyOptions, HunkApplyStatus};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let original_lines = vec!["line 1", "line 2"];
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-line 2\n+line two\n```";
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
        if self.current_idx >= self.hunks.len() {
            return None;
        }
        let hunk = &self.hunks[self.current_idx];
        self.current_idx += 1;

        let old_len = self.current_lines.len();

        let mut adjusted_hunk;
        let hunk_to_apply = if let Some(old_start) = hunk.old_start_line {
            let applicable_delta: isize = self
                .completed_edits
                .iter()
                .filter(|(orig_pos, _)| *orig_pos <= old_start)
                .map(|(_, d)| *d)
                .sum();

            if applicable_delta != 0 {
                adjusted_hunk = hunk.clone();
                let shifted = (old_start as isize + applicable_delta).max(1) as usize;
                trace!(
                    "  HunkApplier: shifting hunk {} line hint from {} to {} (cumulative delta={})",
                    self.current_idx,
                    old_start,
                    shifted,
                    applicable_delta
                );
                adjusted_hunk.old_start_line = Some(shifted);
                &adjusted_hunk
            } else {
                hunk
            }
        } else {
            hunk
        };

        let status = apply_hunk_to_lines(hunk_to_apply, &mut self.current_lines, self.options);
        debug!(
            "  HunkApplier: hunk {} application outcome: {:?}",
            self.current_idx, status
        );

        if let HunkApplyStatus::Applied { location, .. } = &status {
            let new_len = self.current_lines.len();
            let delta = (new_len as isize) - (old_len as isize);
            let orig_pos = hunk.old_start_line.unwrap_or(location.start_index + 1);
            trace!(
                "  HunkApplier: hunk {} applied at line {} (len={}), delta={}, target lines now={}",
                self.current_idx,
                location.start_index + 1,
                location.length,
                delta,
                new_len
            );
            self.completed_edits.push((orig_pos, delta));
            let inserted_len = (location.length as isize + delta) as usize;
            if location.start_index + inserted_len >= new_len {
                trace!(
                    "  HunkApplier: hunk touched EOF (start={}, inserted={}, total={})",
                    location.start_index,
                    inserted_len,
                    new_len
                );
                self.touched_eof = true;
            }
            self.last_applied_line = Some(location.start_index + inserted_len + 1);
        }
        Some(status)
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
/// # Examples
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
    debug!(
        "apply_patch_to_lines: patch for '{}' ({} hunks), original line count={:?}",
        patch.file_path.display(),
        patch.hunks.len(),
        original_lines.map(|l| l.len())
    );
    apply_patch_to_lines_internal(patch, original_lines, options, true)
}

/// Internal helper for applying a patch to a slice of lines while preserving EOF newline behavior.
///
/// Drives a [`HunkApplier`] across all hunks, handles per-hunk progress logging,
/// and constructs the resulting [`InMemoryResult`].
///
/// # Arguments
///
/// * `patch` - The [`Patch`] to apply.
/// * `original_lines` - Optional slice of lines representing the original file content.
/// * `options` - Configuration options controlling fuzziness and dry-run mode.
/// * `original_ends_with_newline` - Whether the original file content had a trailing newline.
///
/// # Returns
///
/// An [`InMemoryResult`] containing the new patched content string and per-hunk reports.
fn apply_patch_to_lines_internal<T: AsRef<str>>(
    patch: &Patch,
    original_lines: Option<&[T]>,
    options: &ApplyOptions,
    original_ends_with_newline: bool,
) -> InMemoryResult {
    debug!(
        "  apply_patch_to_lines called with {} lines of original content.",
        original_lines.as_ref().map_or(0, |l| l.len())
    );

    let mut applier = HunkApplier::new(patch, original_lines, options);
    applier.set_original_newline_status(original_ends_with_newline);
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
                    if log::log_enabled!(log::Level::Trace) {
                        trace!("    Replaced lines:");
                        for line in replaced_lines {
                            trace!("      - {}", line);
                        }
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
/// # Arguments
///
/// * `patch` - The [`Patch`] object to apply.
/// * `original_lines` - An `Option` containing a slice of strings representing the file's content.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// An [`InMemoryResult`] if all hunks were applied successfully.
///
/// # Errors
///
/// Returns `Err(`[`StrictApplyError::PartialApply`]`)` if some hunks failed to apply. The returned
/// `report` within the error contains the detailed results.
///
/// # Examples
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
    debug!(
        "try_apply_patch_to_lines: strictly applying patch for '{}'",
        patch.file_path.display()
    );
    let result = apply_patch_to_lines(patch, original_lines, options);
    if result.report.all_applied_cleanly() {
        debug!(
            "try_apply_patch_to_lines: all hunks applied cleanly for '{}'",
            patch.file_path.display()
        );
        Ok(result)
    } else {
        warn!(
            "try_apply_patch_to_lines: partial apply for '{}' ({} failures)",
            patch.file_path.display(),
            result.report.failure_count()
        );
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
/// For file creation patches (where [`Patch::is_creation`] returns `true`), pass `None`
/// for `original_content`. For file modification patches, pass `Some(content)`.
/// If the patch removes all lines, the resulting `new_content` string will be empty.
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
/// # Examples
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
    debug!(
        "apply_patch_to_content: patch for '{}' ({} hunks), original content: {} bytes",
        patch.file_path.display(),
        patch.hunks.len(),
        original_content.map_or(0, |s| s.len())
    );
    let original_lines: Option<Vec<String>> =
        original_content.map(|c| c.lines().map(String::from).collect());
    let original_ends_with_newline = original_content.is_none_or(|s| {
        if s.is_empty() {
            false
        } else {
            s.ends_with('\n')
        }
    });
    apply_patch_to_lines_internal(
        patch,
        original_lines.as_deref(),
        options,
        original_ends_with_newline,
    )
}

/// A strict variant of [`apply_patch_to_content()`] that treats partial applications as an error.
///
/// This function provides a simpler error handling model for workflows where any
/// failed hunk should be considered a failure for the entire operation.
///
/// For file creation patches (where [`Patch::is_creation`] returns `true`), pass `None`
/// for `original_content`. For file modification patches, pass `Some(content)`.
/// If the patch removes all lines, the resulting `new_content` string will be empty.
///
/// # Arguments
///
/// * `patch` - The [`Patch`] object to apply.
/// * `original_content` - An `Option<&str>` representing the file's content.
/// * `options` - Configuration for the patch operation.
///
/// # Returns
///
/// An [`InMemoryResult`] if all hunks were applied successfully.
///
/// # Errors
///
/// Returns `Err(`[`StrictApplyError::PartialApply`]`)` if some hunks failed to apply. The returned
/// `report` within the error contains the detailed results.
///
/// # Examples
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
    debug!(
        "try_apply_patch_to_content: strictly applying patch for '{}' in-memory",
        patch.file_path.display()
    );
    let result = apply_patch_to_content(patch, original_content, options);
    if result.report.all_applied_cleanly() {
        debug!(
            "try_apply_patch_to_content: all hunks applied cleanly for '{}'",
            patch.file_path.display()
        );
        Ok(result)
    } else {
        warn!(
            "try_apply_patch_to_content: partial apply for '{}' ({} failures)",
            patch.file_path.display(),
            result.report.failure_count()
        );
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
///     Unified Diffs, Aider search/replace blocks, and Conflict Markers).
/// 2.  Ensures that exactly one `Patch` is found. If zero or more than one are
///     found, it returns an error.
/// 3.  Applies the single patch to `original_content` using the strict logic of
///     [`try_apply_patch_to_content()`].
///
/// # Arguments
///
/// * `diff_content` - A string slice containing the diff. This can be a Markdown
///   code block, a raw Unified Diff, Aider search/replace blocks, or Conflict Markers.
/// * `original_content` - An `Option<&str>` representing the content to be patched.
///   Use `Some(content)` for an existing file, or `None` for a file creation patch.
/// * `options` - Configuration for the patch operation, such as `fuzz_factor`.
///
/// # Returns
///
/// The new, patched content as a `String` if the patch applied cleanly.
///
/// # Errors
///
/// Returns `Err(`[`OneShotError`]`)` if any step fails, including parsing errors, finding
/// the wrong number of patches, or if the patch does not apply cleanly (i.e.,
/// any hunk fails).
///
/// # Examples
///
/// **Unified Diff Example:**
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
///
/// **Aider Search/Replace Example:**
/// ````rust
/// # use mpatch::{patch_content_str, ApplyOptions, OneShotError};
/// # fn main() -> Result<(), OneShotError> {
/// let original = "def greet():\n    print('hello')\n";
/// let aider_diff = "app.py\n<<<<<<< SEARCH\n    print('hello')\n=======\n    print('hello, world!')\n>>>>>>> REPLACE\n";
/// let options = ApplyOptions::new();
/// let patched = patch_content_str(aider_diff, Some(original), &options)?;
/// assert_eq!(patched, "def greet():\n    print('hello, world!')\n");
///
/// Ok(())
/// # }
/// ````
pub fn patch_content_str(
    diff_content: &str,
    original_content: Option<&str>,
    options: &ApplyOptions,
) -> Result<String, OneShotError> {
    debug!(
        "patch_content_str: parsing diff ({} bytes) and applying to content ({:?} bytes)",
        diff_content.len(),
        original_content.map(|s| s.len())
    );
    let mut patches = parse_auto(diff_content)?;
    if patches.is_empty() {
        warn!("patch_content_str: no patches found in input");
        return Err(OneShotError::NoPatchesFound);
    }
    if patches.len() > 1 {
        warn!(
            "patch_content_str: expected exactly 1 patch, but found {}",
            patches.len()
        );
        return Err(OneShotError::MultiplePatchesFound(patches.len()));
    }
    let patch = patches.remove(0);
    debug!(
        "patch_content_str: applying single patch for '{}' ({} hunk(s))",
        patch.file_path.display(),
        patch.hunks.len()
    );
    let result = try_apply_patch_to_content(&patch, original_content, options)?;
    debug!(
        "patch_content_str: application succeeded. Result content length: {} bytes",
        result.new_content.len()
    );
    Ok(result.new_content)
}

/// Helper to adjust the indentation of a line based on a detected offset.
///
/// If `target_indent` is shorter than `hunk_indent`, we strip the difference from `line`.
/// If `target_indent` is longer, we prepend the difference. It also intelligently translates
/// indentation between spaces and tabs when appropriate.
///
/// # Arguments
///
/// * `line` - The line whose leading indentation should be adjusted.
/// * `hunk_indent` - The indentation style/level detected in the patch hunk.
/// * `target_indent` - The indentation style/level detected in the target file.
///
/// # Returns
///
/// A `String` with the adjusted leading indentation prefix.
fn adjust_indentation(line: &str, hunk_indent: &str, target_indent: &str) -> String {
    if line.trim().is_empty() {
        return String::new();
    }
    if hunk_indent == target_indent {
        return line.to_string();
    }

    let line_indent = get_indent(line);
    trace!(
        "adjust_indentation: line='{}', hunk_indent='{}', target_indent='{}', line_indent='{}'",
        line.escape_debug(),
        hunk_indent.escape_debug(),
        target_indent.escape_debug(),
        line_indent.escape_debug()
    );

    // Check for pure spaces to pure tabs translation (or vice versa)
    if !hunk_indent.is_empty() && !target_indent.is_empty() {
        let hunk_is_spaces = hunk_indent.chars().all(|c| c == ' ');
        let target_is_tabs = target_indent.chars().all(|c| c == '\t');

        if hunk_is_spaces && target_is_tabs {
            let spaces_per_tab = if hunk_indent.len().is_multiple_of(target_indent.len())
                && hunk_indent.len() / target_indent.len() <= 4
            {
                hunk_indent.len() / target_indent.len()
            } else {
                4
            };

            if line_indent.chars().all(|c| c == ' ') {
                let hunk_tabs = hunk_indent.len() / spaces_per_tab;
                let line_tabs = line_indent.len() / spaces_per_tab;
                let line_spaces = line_indent.len() % spaces_per_tab;

                let target_tabs = target_indent.len();

                let res = if line_tabs >= hunk_tabs {
                    let new_tabs = target_tabs + (line_tabs - hunk_tabs);
                    let new_indent =
                        format!("{}{}", "\t".repeat(new_tabs), " ".repeat(line_spaces));
                    format!("{}{}", new_indent, &line[line_indent.len()..])
                } else {
                    let outdent = hunk_tabs - line_tabs;
                    let new_tabs = target_tabs.saturating_sub(outdent);
                    let new_indent =
                        format!("{}{}", "\t".repeat(new_tabs), " ".repeat(line_spaces));
                    format!("{}{}", new_indent, &line[line_indent.len()..])
                };
                trace!(
                    "  adjust_indentation: translated spaces to tabs (spaces_per_tab={}): '{}'",
                    spaces_per_tab,
                    res.escape_debug()
                );
                return res;
            }
        }

        let hunk_is_tabs = hunk_indent.chars().all(|c| c == '\t');
        let target_is_spaces = target_indent.chars().all(|c| c == ' ');

        if hunk_is_tabs && target_is_spaces {
            let spaces_per_tab = if target_indent.len().is_multiple_of(hunk_indent.len())
                && target_indent.len() / hunk_indent.len() <= 4
            {
                target_indent.len() / hunk_indent.len()
            } else {
                4
            };

            if line_indent.chars().all(|c| c == '\t') {
                let hunk_tabs = hunk_indent.len();
                let line_tabs = line_indent.len();

                let target_spaces = target_indent.len();

                let res = if line_tabs >= hunk_tabs {
                    let new_spaces = target_spaces + (line_tabs - hunk_tabs) * spaces_per_tab;
                    let new_indent = " ".repeat(new_spaces);
                    format!("{}{}", new_indent, &line[line_indent.len()..])
                } else {
                    let outdent_spaces = (hunk_tabs - line_tabs) * spaces_per_tab;
                    let new_spaces = target_spaces.saturating_sub(outdent_spaces);
                    let new_indent = " ".repeat(new_spaces);
                    format!("{}{}", new_indent, &line[line_indent.len()..])
                };
                trace!(
                    "  adjust_indentation: translated tabs to spaces (spaces_per_tab={}): '{}'",
                    spaces_per_tab,
                    res.escape_debug()
                );
                return res;
            }
        }
    }

    // If the line starts with the hunk's indentation, we can simply replace it with the target's indentation.
    if let Some(stripped) = line.strip_prefix(hunk_indent) {
        let res = format!("{}{}", target_indent, stripped);
        trace!(
            "  adjust_indentation: replaced prefix matching hunk indent: '{}'",
            res.escape_debug()
        );
        return res;
    }

    // Fallback for lines that are outdented relative to the hunk's context.
    if let Some(diff) = hunk_indent.strip_prefix(target_indent) {
        // Hunk is more indented than target. Try to strip the difference from the end of line_indent.
        let mut new_indent = line_indent.to_string();
        for c in diff.chars().rev() {
            if new_indent.ends_with(c) {
                new_indent.pop();
            } else {
                break;
            }
        }
        let res = format!("{}{}", new_indent, &line[line_indent.len()..]);
        trace!(
            "  adjust_indentation: outdented fallback adjustment: '{}'",
            res.escape_debug()
        );
        res
    } else if let Some(diff) = target_indent.strip_prefix(hunk_indent) {
        // Target is more indented than hunk.
        // We need to add `diff` to the start of `line`.
        let res = format!("{}{}", diff, line);
        trace!(
            "  adjust_indentation: indented fallback adjustment: '{}'",
            res.escape_debug()
        );
        res
    } else {
        trace!(
            "  adjust_indentation: no adjustment applied: '{}'",
            line.escape_debug()
        );
        line.to_string()
    }
}

/// Helper to extract the whitespace prefix from a line.
///
/// # Arguments
///
/// * `line` - The input line.
///
/// # Returns
///
/// A string slice representing all leading whitespace characters in `line`.
fn get_indent(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

/// Extracts the primary definition or invocation identifier from a line of code.
///
/// Recognizes function, method, struct, class, enum, trait, interface, and type definitions
/// across Rust, Python, Go, JavaScript/TypeScript, C/C++, Java, C#, and other languages,
/// as well as function and method call expressions preceding parentheses.
///
/// Strictly ignores comments, string literals, and control-flow keywords (`if`, `while`, etc.).
fn extract_primary_identifier(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Strictly ignore comments and documentation
    if trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('#')
        || trimmed.starts_with("<!--")
        || trimmed.starts_with("--")
        || trimmed.starts_with(';')
        || trimmed.starts_with('%')
    {
        return None;
    }

    // 2. Definition patterns across languages
    // Strip leading decorators/attributes like `@decorator`
    let mut text = trimmed;
    while text.starts_with('@') {
        if let Some(pos) = text.find(char::is_whitespace) {
            text = text[pos..].trim_start();
        } else {
            return None;
        }
    }

    let mut words = text.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');

        // Skip language visibility modifiers, async/const qualifiers, and pub(...) scopes
        if clean_word.starts_with("pub")
            || matches!(
                clean_word,
                "public"
                    | "private"
                    | "protected"
                    | "internal"
                    | "static"
                    | "final"
                    | "abstract"
                    | "virtual"
                    | "override"
                    | "sealed"
                    | "async"
                    | "unsafe"
                    | "extern"
                    | "default"
                    | "inline"
                    | "explicit"
                    | "constexpr"
                    | "consteval"
                    | "friend"
                    | "export"
                    | "suspend"
                    | "readonly"
                    | "mut"
            )
        {
            // If modifier has parentheses (e.g. pub(crate) or extern "C"), skip consumed tokens
            if word.contains('(') && !word.contains(')') {
                for next_tok in words.by_ref() {
                    if next_tok.contains(')') {
                        break;
                    }
                }
            }
            continue;
        }

        // Definition keywords
        if matches!(
            clean_word,
            "fn" | "def"
                | "function"
                | "func"
                | "fun"
                | "class"
                | "struct"
                | "enum"
                | "trait"
                | "interface"
                | "union"
                | "type"
        ) {
            // Check for Go receiver: func (s *Server) Name(...)
            if clean_word == "func" {
                if let Some(&next_tok) = words.peek() {
                    if next_tok.starts_with('(') {
                        words.next();
                        if !next_tok.contains(')') {
                            for tok in words.by_ref() {
                                if tok.contains(')') {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if let Some(name_word) = words.next() {
                let name = name_word
                    .split(['(', '<', ':', '{', ' ', '[', ';', '='])
                    .next()
                    .unwrap_or(name_word);
                let clean_name = name.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                if !clean_name.is_empty()
                    && clean_name
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                {
                    trace!(
                        "extract_primary_identifier: found definition identifier '{}' in '{}'",
                        clean_name,
                        trimmed
                    );
                    return Some(clean_name);
                }
            }
        }
        break;
    }

    // 3. Call patterns and method invocations: identifier immediately preceding '('
    let mut search_start = 0;
    while let Some(rel_paren) = trimmed[search_start..].find('(') {
        let paren_pos = search_start + rel_paren;
        let before_paren = &trimmed[..paren_pos];
        let ident = before_paren
            .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");

        if !ident.is_empty()
            && ident
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            if matches!(
                ident,
                "if" | "while"
                    | "for"
                    | "switch"
                    | "match"
                    | "catch"
                    | "return"
                    | "typeof"
                    | "sizeof"
                    | "alignof"
                    | "decltype"
                    | "new"
                    | "delete"
                    | "throw"
                    | "assert"
                    | "let"
                    | "var"
                    | "const"
                    | "pub"
            ) {
                search_start = paren_pos + 1;
                continue;
            }
            trace!(
                "extract_primary_identifier: found call identifier '{}' in '{}'",
                ident,
                trimmed
            );
            return Some(ident);
        }
        search_start = paren_pos + 1;
    }

    None
}

/// Returns whether a line represents a declaration/definition header.
fn is_definition(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    // Ignore comments
    if trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('#')
        || trimmed.starts_with("<!--")
        || trimmed.starts_with("--")
        || trimmed.starts_with(';')
        || trimmed.starts_with('%')
    {
        return false;
    }

    let mut words = trimmed.split_whitespace();
    while let Some(word) = words.next() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if clean.starts_with("pub")
            || matches!(
                clean,
                "public"
                    | "private"
                    | "protected"
                    | "internal"
                    | "static"
                    | "final"
                    | "abstract"
                    | "virtual"
                    | "override"
                    | "sealed"
                    | "async"
                    | "unsafe"
                    | "extern"
                    | "default"
                    | "inline"
                    | "explicit"
                    | "constexpr"
                    | "export"
                    | "suspend"
                    | "readonly"
                    | "mut"
            )
        {
            if word.contains('(') && !word.contains(')') {
                for next_tok in words.by_ref() {
                    if next_tok.contains(')') {
                        break;
                    }
                }
            }
            continue;
        }

        let res = matches!(
            clean,
            "fn" | "def"
                | "function"
                | "func"
                | "fun"
                | "class"
                | "struct"
                | "enum"
                | "trait"
                | "interface"
                | "union"
                | "type"
        );
        if res {
            trace!(
                "is_definition: '{}' is a definition header ({})",
                trimmed,
                clean
            );
        }
        return res;
    }
    false
}

/// Searches for a target line in `new_slice` that represents the same semantic statement
/// as `old_slice` across line-break or formatting variations.
///
/// Computes word-level and non-whitespace character similarity between the joined text of
/// `old_slice` and individual candidate lines in `new_slice`.
///
/// # Arguments
///
/// * `old_slice` - The sequence of hunk match lines representing the statement.
/// * `new_slice` - Candidate target lines in the target file window.
/// * `_has_context` - Unused flag maintained for interface compatibility.
///
/// # Returns
///
/// `Some(index)` of the best matching line in `new_slice`, or `None` if no line meets the similarity threshold.
fn find_statement_match_in_block(
    old_slice: &[&str],
    new_slice: &[String],
    _has_context: bool,
) -> Option<usize> {
    if old_slice.is_empty() || new_slice.is_empty() {
        return None;
    }
    if old_slice.iter().all(|l| is_ellipsis_line(l)) {
        return None;
    }
    trace!(
        "find_statement_match_in_block: attempting statement alignment for {} line(s) against {} target line(s)",
        old_slice.len(),
        new_slice.len()
    );

    // If old_slice has multiple lines, ensure line 0 is an incomplete statement
    // (e.g., an unclosed parameter list or wrapped expression) rather than a complete
    // statement or block header.
    if old_slice.len() > 1 {
        let first_trimmed = old_slice[0].trim();
        if first_trimmed.is_empty()
            || first_trimmed.starts_with("//")
            || first_trimmed.starts_with('#')
            || first_trimmed.starts_with("/*")
            || first_trimmed.starts_with('*')
            || first_trimmed.starts_with("<!--")
            || first_trimmed.ends_with('{')
            || first_trimmed.ends_with(';')
            || first_trimmed.ends_with('}')
        {
            return None;
        }
        if first_trimmed.ends_with(':')
            && (first_trimmed.starts_with("def ")
                || first_trimmed.starts_with("async def ")
                || first_trimmed.starts_with("class ")
                || first_trimmed.starts_with("if ")
                || first_trimmed.starts_with("while ")
                || first_trimmed.starts_with("for ")
                || first_trimmed.starts_with("with "))
        {
            return None;
        }
    }

    let old_text = old_slice.join(" ");
    let old_trimmed = old_text.trim();
    if old_trimmed.is_empty() {
        return None;
    }

    let id_old = extract_primary_identifier(old_trimmed);

    let mut best_match = None;
    let mut best_ratio: f32 = 0.0;
    // Require a strong match (>= 0.80) to avoid false-positive statement hijacking.
    let threshold: f32 = 0.80;

    let old_no_ws: String = old_trimmed.chars().filter(|c| !c.is_whitespace()).collect();

    for (idx, line) in new_slice.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // If both lines contain primary identifiers (function/call name), they must match.
        if let (Some(o_id), Some(n_id)) = (id_old, extract_primary_identifier(trimmed)) {
            if o_id != n_id {
                continue;
            }
        }

        let diff = similar::TextDiff::configure()
            .algorithm(similar::Algorithm::Histogram)
            .diff_words(old_trimmed, trimmed);
        let ratio = diff.ratio();

        let new_no_ws: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
        let ratio_no_ws = similar::TextDiff::from_chars(&old_no_ws, &new_no_ws).ratio();
        let combined_ratio = ratio.max(ratio_no_ws);

        trace!(
            "  find_statement_match_in_block: comparing target line {} ('{}'): word_ratio={:.3}, char_ratio={:.3}, combined={:.3}",
            idx + 1,
            trimmed,
            ratio,
            ratio_no_ws,
            combined_ratio
        );

        if combined_ratio > best_ratio && combined_ratio >= threshold {
            trace!(
                "  Candidate statement line {} matched with ratio {:.3}: '{}'",
                idx + 1,
                combined_ratio,
                trimmed
            );
            best_ratio = combined_ratio;
            best_match = Some(idx);
        }
    }
    if let Some(idx) = best_match {
        debug!(
            "find_statement_match_in_block: aligned statement with target line {} (ratio={:.3})",
            idx + 1,
            best_ratio
        );
    }
    best_match
}

/// Checks whether an entire slice of lines consists solely of low-entropy syntax tokens.
///
/// # Arguments
///
/// * `seg` - Slice of string slices to check.
///
/// # Returns
///
/// `true` if every line in `seg` is low-entropy, `false` otherwise.
fn is_low_entropy_segment(seg: &[&str]) -> bool {
    let res = seg.iter().all(|l| is_low_entropy_line(l));
    if res {
        trace!(
            "is_low_entropy_segment: segment with {} line(s) is low-entropy",
            seg.len()
        );
    }
    res
}

/// Checks whether a line is trivial / low-entropy syntax (e.g. closing braces, blank lines).
///
/// Low-entropy lines (such as alone `}`, `];`, or empty lines) should not be used as the
/// sole criterion for resolving ambiguous match locations using line number hints.
///
/// # Arguments
///
/// * `line` - The line to test.
///
/// # Returns
///
/// `true` if the line consists solely of low-entropy syntax tokens, `false` otherwise.
fn is_low_entropy_line(line: &str) -> bool {
    let trimmed = line.trim();
    let res = matches!(trimmed, "" | "}" | "};" | "]" | "];" | ")" | ");" | "{");
    if res {
        trace!("is_low_entropy_line: line '{}' is low-entropy", trimmed);
    }
    res
}

/// Normalizes trailing delimiters (such as semicolons and commas) and trailing inline comments
/// for line-by-line reconciliation when strict context diffing fails.
fn normalize_line_delimiters(s: &str) -> String {
    let mut trimmed = s.trim();
    if let Some(pos) = trimmed.rfind("//") {
        if pos > 0 && trimmed.as_bytes()[pos - 1].is_ascii_whitespace() {
            let before = trimmed[..pos].trim_end();
            if !before.is_empty() {
                trimmed = before;
            }
        }
    } else if let Some(pos) = trimmed.rfind('#') {
        if pos > 0 && trimmed.as_bytes()[pos - 1].is_ascii_whitespace() {
            let before = trimmed[..pos].trim_end();
            if !before.is_empty() {
                trimmed = before;
            }
        }
    }
    let res = trimmed.trim_end_matches([';', ',']).trim_end().to_string();
    trace!("normalize_line_delimiters: '{}' -> '{}'", s, res);
    res
}

/// Applies a single hunk to a mutable vector of lines in-place.
///
/// This function provides granular control over the patching process, allowing library
/// users to apply changes hunk-by-hunk. It modifies the `target_lines` vector
/// directly based on the changes defined in the `hunk`.
///
/// If an initial candidate match location fails during reconstruction (for example, if
/// added lines cannot be cleanly anchored without context corruption), the function
/// automatically backtracks to evaluate alternative candidate locations before reporting
/// failure.
///
/// If application fails, `target_lines` is guaranteed to remain in its original, unmodified
/// state.
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
/// # Examples
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
    debug!(
        "apply_hunk_to_lines: applying hunk with {} line(s) against target with {} line(s)",
        hunk.lines.len(),
        target_lines.len()
    );
    if log::log_enabled!(log::Level::Trace) {
        trace!("  Match block: {:?}", hunk.get_match_block());
        trace!("  Replace block: {:?}", hunk.get_replace_block());
    }
    if !hunk.has_changes() {
        debug!("  Hunk has no changes (only context lines), skipping.");
        return HunkApplyStatus::SkippedNoChanges;
    }

    let finder = DefaultHunkFinder::new(options);
    let candidates = match finder.find_candidate_locations(hunk, target_lines) {
        Ok(c) => c,
        Err(error) => {
            warn!("  Hunk location search failed: {}", error);
            return HunkApplyStatus::Failed(error);
        }
    };

    let candidate_count = candidates.len();
    debug!(
        "  Found {} candidate location(s) for hunk. Testing sequentially...",
        candidate_count
    );

    let mut last_error = HunkApplyError::ContextNotFound;

    // --- Pass 1: Strict application across candidates ---
    for (cand_idx, (location, match_type)) in candidates.iter().cloned().enumerate() {
        trace!(
            "  Evaluating candidate {}/{} at location {:?} (match_type: {:?})",
            cand_idx + 1,
            candidate_count,
            location,
            match_type
        );
        match try_apply_hunk_at_location(hunk, target_lines, location, match_type, options, false) {
            Ok(status) => {
                debug!(
                    "  Candidate {}/{} at {:?} succeeded! Hunk applied cleanly.",
                    cand_idx + 1,
                    candidate_count,
                    location
                );
                return status;
            }
            Err(e) => {
                debug!(
                    "  Candidate {}/{} at {:?} failed with {:?}. Backtracking...",
                    cand_idx + 1,
                    candidate_count,
                    location,
                    e
                );
                last_error = e;
            }
        }
    }

    // --- Pass 2: Fallback lenient reconciliation only if Pass 1 (strict) failed on all candidates ---
    debug!(
        "  Strict application failed for all {} candidate(s). Retrying with fallback context reconciliation...",
        candidate_count
    );
    for (cand_idx, (location, match_type)) in candidates.into_iter().enumerate() {
        trace!(
            "  Evaluating candidate {}/{} with lenient reconciliation at location {:?}",
            cand_idx + 1,
            candidate_count,
            location
        );
        match try_apply_hunk_at_location(hunk, target_lines, location, match_type, options, true) {
            Ok(status) => {
                info!(
                    "  Hunk applied cleanly at location {:?} via fallback context reconciliation.",
                    location
                );
                return status;
            }
            Err(e) => {
                last_error = e;
            }
        }
    }

    warn!(
        "  All {} candidate location(s) exhausted. Hunk application failed with: {}",
        candidate_count, last_error
    );
    HunkApplyStatus::Failed(last_error)
}

/// Attempts to apply a hunk at a specific candidate location within `target_lines`.
///
/// For exact matches ([`MatchType::Exact`]), lines are spliced in directly using the hunk's
/// replacement block.
///
/// For fuzzy and whitespace-insensitive matches ([`MatchType::Fuzzy`], [`MatchType::ExactIgnoringWhitespace`]),
/// a granular reconstruction pass is performed:
/// - Context lines and local file modifications are preserved.
/// - Dynamic indentation is established and tracked line by line.
/// - Multi-line signature and statement re-alignments are resolved via [`find_statement_match_in_block`].
/// - If additions cannot be anchored safely (e.g., attached context line was deleted or unaligned),
///   the candidate is rejected with [`HunkApplyError::ContextNotFound`].
///
/// # Arguments
///
/// * `hunk` - The [`Hunk`] being applied.
/// * `target_lines` - The mutable vector of target file lines, modified in-place on success.
/// * `location` - The candidate [`HunkLocation`] where changes should be spliced.
/// * `match_type` - The method by which this location was matched.
///
/// # Returns
///
/// Returns [`Ok(HunkApplyStatus::Applied)`] on successful reconstruction and splicing, or
/// [`Err(HunkApplyError)`] if the candidate location cannot safely accommodate the hunk.
///
/// # Errors
///
/// Returns [`Err(HunkApplyError)`] if the candidate location cannot safely accommodate the hunk.
fn try_apply_hunk_at_location(
    hunk: &Hunk,
    target_lines: &mut Vec<String>,
    location: HunkLocation,
    match_type: MatchType,
    options: &ApplyOptions,
    lenient: bool,
) -> Result<HunkApplyStatus, HunkApplyError> {
    debug!(
        "  Found location {:?} with match type {:?}. Applying changes.",
        location, match_type
    );
    debug!(
        "  try_apply_hunk_at_location: target line {} (length {}), match_type={:?}, total target lines={}.",
        location.start_index + 1,
        location.length,
        match_type,
        target_lines.len()
    );

    let match_block = hunk.get_match_block();
    let has_ellipsis = hunk
        .lines
        .iter()
        .any(|l| is_ellipsis_line(l.strip_prefix([' ', '+', '-']).unwrap_or(l)));
    let target_slice = if location.start_index + location.length <= target_lines.len() {
        &target_lines[location.start_index..location.start_index + location.length]
    } else {
        &[]
    };
    let is_literal_match = has_ellipsis
        && location.length == match_block.len()
        && target_slice
            .iter()
            .zip(match_block.iter())
            .all(|(t, m)| t.trim() == m.trim());
    let is_wildcard_gap_match = has_ellipsis && !is_literal_match;

    trace!(
        "    Match block lines: {} | Total hunk lines: {}",
        match_block.len(),
        hunk.lines.len()
    );
    trace!("    Target slice to replace: {:?}", target_slice);
    trace!(
        "    Ellipsis flags: has_ellipsis={}, is_literal_match={}, is_wildcard_gap_match={}",
        has_ellipsis,
        is_literal_match,
        is_wildcard_gap_match
    );

    let final_replace_block: Vec<String> = if is_wildcard_gap_match {
        debug!(
            "    Applying hunk via wildcard gap reconstruction (target slice has {} lines).",
            location.length
        );
        // Sound N-segment wildcard reconstruction:
        // 1. Separate hunk lines into segments and identify ellipsis removal/context markers
        struct HunkSegment {
            lines: Vec<String>,
            match_lines: Vec<String>,
        }
        let mut hunk_segs: Vec<HunkSegment> = Vec::new();
        let mut ellipsis_is_removal: Vec<bool> = Vec::new();
        let mut current_lines = Vec::new();
        let mut current_match = Vec::new();

        for line in &hunk.lines {
            let raw = line.strip_prefix([' ', '+', '-']).unwrap_or(line);
            if is_ellipsis_line(raw) {
                if current_lines.is_empty() && !hunk_segs.is_empty() {
                    if let Some(last_removal) = ellipsis_is_removal.last_mut() {
                        *last_removal |= line.starts_with('-');
                    }
                    continue;
                }
                ellipsis_is_removal.push(line.starts_with('-'));
                hunk_segs.push(HunkSegment {
                    lines: std::mem::take(&mut current_lines),
                    match_lines: std::mem::take(&mut current_match),
                });
            } else {
                current_lines.push(line.clone());
                if !line.starts_with('+') {
                    current_match.push(raw.to_string());
                }
            }
        }
        hunk_segs.push(HunkSegment {
            lines: current_lines,
            match_lines: current_match,
        });

        debug!(
            "    Partitioned wildcard hunk into {} segment(s) with {} gap marker(s).",
            hunk_segs.len(),
            ellipsis_is_removal.len()
        );

        let file_matched_slice =
            &target_lines[location.start_index..location.start_index + location.length];
        let mut seg_offsets = Vec::with_capacity(hunk_segs.len());
        let mut search_offset = 0;

        for (m, seg) in hunk_segs.iter().enumerate() {
            if m == 0 {
                seg_offsets.push(0);
                search_offset = seg.match_lines.len();
                trace!(
                    "      Segment 0: anchored at target slice offset 0 ({} match line(s))",
                    seg.match_lines.len()
                );
            } else if m == hunk_segs.len() - 1 {
                let offset = file_matched_slice
                    .len()
                    .saturating_sub(seg.match_lines.len());
                let final_offset = offset.max(search_offset);
                seg_offsets.push(final_offset);
                trace!(
                    "      Final Segment {}: anchored at target slice offset {} ({} match line(s))",
                    m,
                    final_offset,
                    seg.match_lines.len()
                );
            } else {
                let seg_trimmed: Vec<&str> = seg.match_lines.iter().map(|s| s.trim()).collect();
                let mut found_pos = search_offset;
                if seg.match_lines.len() <= file_matched_slice.len().saturating_sub(search_offset) {
                    for i in search_offset
                        ..=file_matched_slice
                            .len()
                            .saturating_sub(seg.match_lines.len())
                    {
                        if file_matched_slice[i..i + seg.match_lines.len()]
                            .iter()
                            .map(|s| s.trim())
                            .eq(seg_trimmed.iter().copied())
                        {
                            found_pos = i;
                            break;
                        }
                    }
                }
                seg_offsets.push(found_pos);
                trace!(
                    "      Intermediate Segment {}: anchored at target slice offset {} (search_offset={})",
                    m,
                    found_pos,
                    search_offset
                );
                search_offset = found_pos + seg.match_lines.len();
            }
        }

        let mut final_lines = Vec::new();
        let mut current_hunk_indent = "";
        let mut current_target_indent = "";

        for m in 0..hunk_segs.len() {
            let seg = &hunk_segs[m];
            let seg_offset = seg_offsets[m];
            let seg_len = seg.match_lines.len();
            let target_seg = if seg_offset < file_matched_slice.len() {
                &file_matched_slice[seg_offset..file_matched_slice.len().min(seg_offset + seg_len)]
            } else {
                &[]
            };

            for t_line in target_seg {
                let ind = get_indent(t_line);
                if !ind.is_empty() && !t_line.trim().is_empty() {
                    current_target_indent = ind;
                    break;
                }
            }
            for line in &seg.lines {
                if line.starts_with([' ', '-']) {
                    let raw = line.strip_prefix([' ', '-']).unwrap_or(line);
                    let ind = get_indent(raw);
                    if !ind.is_empty() && !raw.trim().is_empty() {
                        current_hunk_indent = ind;
                        break;
                    }
                }
            }

            trace!(
                "      Reconstructing segment {} (offset={}, match_len={}): hunk_indent='{}', target_indent='{}'",
                m,
                seg_offset,
                seg_len,
                current_hunk_indent.escape_debug(),
                current_target_indent.escape_debug()
            );

            let mut t_idx = 0;
            let mut seg_added_count = 0;
            for line in &seg.lines {
                if line.starts_with(' ') {
                    if t_idx < target_seg.len() {
                        final_lines.push(target_seg[t_idx].clone());
                        t_idx += 1;
                    }
                } else if line.starts_with('-') {
                    t_idx += 1;
                } else if let Some(add) = line.strip_prefix('+') {
                    if !is_ellipsis_line(add) {
                        final_lines.push(adjust_indentation(
                            add,
                            current_hunk_indent,
                            current_target_indent,
                        ));
                        seg_added_count += 1;
                    }
                }
            }
            trace!(
                "        Segment {} emitted lines (current total: {}, added: {})",
                m,
                final_lines.len(),
                seg_added_count
            );

            if m < hunk_segs.len() - 1 && m < ellipsis_is_removal.len() {
                let gap_start = seg_offset + seg_len;
                let gap_end = seg_offsets[m + 1];
                if !ellipsis_is_removal[m]
                    && gap_start <= gap_end
                    && gap_end <= file_matched_slice.len()
                {
                    let gap_lines = &file_matched_slice[gap_start..gap_end];
                    trace!(
                        "        Preserving untouched code gap between segments {} and {} ({} line(s), offsets {}..{})",
                        m,
                        m + 1,
                        gap_lines.len(),
                        gap_start,
                        gap_end
                    );
                    for t_line in &file_matched_slice[gap_start..gap_end] {
                        final_lines.push(t_line.clone());
                    }
                } else if ellipsis_is_removal[m] {
                    trace!(
                        "        Omitting removed gap between segments {} and {} as requested by '-' prefix",
                        m,
                        m + 1
                    );
                }
            }
        }
        debug!(
            "    Wildcard gap reconstruction produced {} replacement line(s) for {} target line(s).",
            final_lines.len(),
            location.length
        );
        final_lines
    } else if matches!(match_type, MatchType::Exact) {
        // For Exact matches, we assume the patch's indentation is intentional and correct relative to the context.
        // We don't need dynamic adjustment because the context matched byte-for-byte.
        trace!("    Applying hunk via exact logic.");
        let block: Vec<String> = hunk
            .get_replace_block()
            .iter()
            .map(|s| {
                if s.trim().is_empty() {
                    String::new()
                } else {
                    s.to_string()
                }
            })
            .collect();
        debug!(
            "    Exact match replacement generated {} line(s) to replace {} line(s).",
            block.len(),
            location.length
        );
        trace!("    Replacement block content: {:?}", block);
        block
    } else {
        // For Fuzzy and ExactIgnoringWhitespace, indentation might mismatch or drift.
        // We use a robust reconstruction that dynamically adjusts indentation based on the
        // nearest matching line.
        debug!("    Applying hunk via robust reconstruction logic (preserving file context & adjusting indent).");
        trace!(
            "      Fuzzy match location: start={}, len={}",
            location.start_index,
            location.length
        );
        let file_matched_slice =
            &target_lines[location.start_index..location.start_index + location.length];
        trace!(
            "      File content in matched range ({} line(s)): {:?}",
            file_matched_slice.len(),
            file_matched_slice
        );

        // 1. Parse hunk to separate match lines and additions.
        // We map each line in the match block (Context/Removal) to a list of additions that follow it.
        // match_lines_meta: Vec<(is_removal, additions_after_this_line)>
        // Note: We store raw additions here and adjust them later during reconstruction.
        let mut match_lines_meta: Vec<(bool, Vec<&str>)> = Vec::new();
        let mut initial_additions: Vec<&str> = Vec::new();

        let mut line_iter = hunk.lines.iter().peekable();

        // Consume any additions that appear before the first context/removal line
        while let Some(line) = line_iter.peek() {
            if let Some(stripped) = line.strip_prefix('+') {
                initial_additions.push(stripped);
                line_iter.next();
            } else {
                break;
            }
        }

        // Process the rest of the hunk
        for line in line_iter {
            if let Some(stripped) = line.strip_prefix('+') {
                // Attach this addition to the most recent match line
                if let Some(last) = match_lines_meta.last_mut() {
                    last.1.push(stripped);
                } else {
                    // Should be unreachable if match block is not empty, but safe fallback
                    initial_additions.push(stripped);
                }
            } else {
                // It's a Context (' ') or Removal ('-') line
                let is_removal = line.starts_with('-');
                match_lines_meta.push((is_removal, Vec::new()));
            }
        }

        trace!(
            "      Parsed hunk: {} match line(s), {} initial addition(s).",
            match_lines_meta.len(),
            initial_additions.len()
        );

        // 2. Prepare text for diffing
        // We align the hunk's "old" view (match block) with the file's actual content.
        let match_block_content: Vec<&str> = hunk.get_match_block();
        let file_block_content: Vec<&str> = file_matched_slice.iter().map(|s| s.as_str()).collect();

        let match_block_trimmed_storage: Vec<String>;
        let file_block_trimmed_storage: Vec<String>;

        let (match_block_trimmed, file_block_trimmed): (Vec<&str>, Vec<&str>) = if lenient {
            match_block_trimmed_storage = match_block_content
                .iter()
                .map(|s| normalize_line_delimiters(s))
                .collect();
            file_block_trimmed_storage = file_block_content
                .iter()
                .map(|s| normalize_line_delimiters(s))
                .collect();
            (
                match_block_trimmed_storage
                    .iter()
                    .map(|s| s.as_str())
                    .collect(),
                file_block_trimmed_storage
                    .iter()
                    .map(|s| s.as_str())
                    .collect(),
            )
        } else {
            (
                match_block_content.iter().map(|s| s.trim()).collect(),
                file_block_content.iter().map(|s| s.trim()).collect(),
            )
        };

        // 3. Diff
        let diff = similar::TextDiff::from_slices(&match_block_trimmed, &file_block_trimmed);
        trace!(
            "      Computed diff between match block and file slice: {} operation(s), similarity ratio={:.3}",
            diff.ops().len(),
            diff.ratio()
        );

        // 4. Determine Initial Indentation Context
        // We scan the diff ops to find the first aligned line (Equal or Replace)
        // to establish the baseline indentation difference.
        let mut current_hunk_indent = "";
        let mut current_target_indent = "";

        for op in diff.ops() {
            let mut found = false;
            match op {
                similar::DiffOp::Equal {
                    old_index,
                    new_index,
                    len,
                } => {
                    // Use the first non-empty line of the block to gauge indentation
                    for i in 0..*len {
                        let h_line = match_block_content[*old_index + i];
                        let t_line = file_block_content[*new_index + i];
                        let h_ind = get_indent(h_line);
                        let t_ind = get_indent(t_line);
                        if (!h_ind.is_empty() || !t_ind.is_empty())
                            && !h_line.trim().is_empty()
                            && !t_line.trim().is_empty()
                        {
                            current_hunk_indent = h_ind;
                            current_target_indent = t_ind;
                            trace!(
                                "      Initial Indentation Context: Hunk='{}', Target='{}'",
                                h_ind.escape_debug(),
                                t_ind.escape_debug()
                            );
                            found = true;
                            break;
                        }
                    }
                }
                similar::DiffOp::Replace {
                    old_index,
                    new_index,
                    old_len,
                    new_len,
                } => {
                    let min_len = std::cmp::min(*old_len, *new_len);
                    for i in 0..min_len {
                        let h_line = match_block_content[*old_index + i];
                        let t_line = file_block_content[*new_index + i];
                        let h_ind = get_indent(h_line);
                        let t_ind = get_indent(t_line);
                        if (!h_ind.is_empty() || !t_ind.is_empty())
                            && !h_line.trim().is_empty()
                            && !t_line.trim().is_empty()
                        {
                            current_hunk_indent = h_ind;
                            current_target_indent = t_ind;
                            trace!("      Initial Indentation Context (from Replace): Hunk='{}', Target='{}'", h_ind.escape_debug(), t_ind.escape_debug());
                            found = true;
                            break;
                        }
                    }
                }
                _ => {}
            }
            if found {
                break;
            }
        }

        trace!(
            "      Active baseline indentation: hunk='{}', target='{}'",
            current_hunk_indent.escape_debug(),
            current_target_indent.escape_debug()
        );

        // 5. Reconstruct the block
        let mut final_lines = Vec::new();

        // Apply initial additions using the seeded indentation
        for line in initial_additions {
            let adj = adjust_indentation(line, current_hunk_indent, current_target_indent);
            trace!("        Applied initial addition: '{}'", adj.escape_debug());
            final_lines.push(adj);
        }

        let is_at_eof = (location.start_index + location.length) == target_lines.len();
        let ops = diff.ops().to_vec();

        for (op_idx, op) in ops.iter().enumerate() {
            match op {
                similar::DiffOp::Equal {
                    old_index,
                    new_index,
                    len,
                } => {
                    trace!(
                        "      DiffOp::Equal: {} line(s) aligned (hunk old_idx={}, file new_idx={})",
                        len,
                        old_index,
                        new_index
                    );
                    // The file content matches the hunk's expectation (fuzzy or exact).
                    for i in 0..*len {
                        let old_idx = old_index + i;
                        let new_idx = new_index + i;
                        let (is_removal, additions) = &match_lines_meta[old_idx];

                        // Update indentation context dynamically based on this matching line
                        let h_line = match_block_content[old_idx];
                        let t_line = &file_matched_slice[new_idx];
                        let h_ind = get_indent(h_line);
                        let t_ind = get_indent(t_line);
                        if (!h_ind.is_empty() || !t_ind.is_empty())
                            && !h_line.trim().is_empty()
                            && !t_line.trim().is_empty()
                            && (current_hunk_indent != h_ind || current_target_indent != t_ind)
                        {
                            trace!(
                                "      Dynamic Indentation Update (Equal): Hunk='{}', Target='{}'",
                                h_ind.escape_debug(),
                                t_ind.escape_debug()
                            );
                            current_hunk_indent = h_ind;
                            current_target_indent = t_ind;
                        }

                        // If it's not a removal, keep the file's version of the line (preserves local edits)
                        if !*is_removal {
                            trace!(
                                "        Equal: preserving target line: '{}'",
                                file_matched_slice[new_idx].escape_debug()
                            );
                            final_lines.push(file_matched_slice[new_idx].clone());
                        } else {
                            trace!(
                                "        Equal: removing target line: '{}'",
                                match_block_content[old_idx].escape_debug()
                            );
                        }
                        if !additions.is_empty() {
                            trace!(
                                "        Appended {} addition(s) after line {}",
                                additions.len(),
                                old_idx
                            );
                        }
                        // Always insert the additions associated with this line
                        for add in additions {
                            let adj =
                                adjust_indentation(add, current_hunk_indent, current_target_indent);
                            trace!("        Equal: applying addition: '{}'", adj.escape_debug());
                            final_lines.push(adj);
                        }
                    }
                }
                similar::DiffOp::Delete {
                    old_index, old_len, ..
                } => {
                    trace!(
                        "      DiffOp::Delete: {} line(s) missing from target file (hunk old_idx={})",
                        old_len,
                        old_index
                    );

                    let is_last_op = op_idx == ops.len() - 1;

                    // 1. Head context cannot be missing from target file
                    if *old_index == 0
                        && (0..*old_len).any(|i| {
                            let (is_removal, _) = &match_lines_meta[*old_index + i];
                            !is_removal && !match_block_content[*old_index + i].trim().is_empty()
                        })
                    {
                        warn!("    Fuzzy match rejected: Head context line(s) missing from target file.");
                        return Err(HunkApplyError::ContextNotFound);
                    }

                    // 2. Tail context cannot be missing from target file (unless restoring truncated context at EOF)
                    let is_tail = *old_index + *old_len == match_block_content.len();
                    let is_eof_restoration = is_at_eof
                        && is_last_op
                        && (0..*old_len)
                            .all(|i| is_low_entropy_line(match_block_content[*old_index + i]))
                        && location.length < match_block_content.len();

                    if is_tail
                        && !is_eof_restoration
                        && (0..*old_len).any(|i| {
                            let (is_removal, _) = &match_lines_meta[*old_index + i];
                            !is_removal && !match_block_content[*old_index + i].trim().is_empty()
                        })
                    {
                        warn!("    Fuzzy match rejected: Tail context line(s) missing from target file.");
                        return Err(HunkApplyError::ContextNotFound);
                    }

                    for i in 0..*old_len {
                        let old_idx = old_index + i;
                        let (is_removal, additions) = &match_lines_meta[old_idx];
                        let is_blank_context = match_block_content[old_idx].trim().is_empty();

                        if !*is_removal && is_eof_restoration {
                            trace!(
                                "        Restoring truncated context line at EOF: {:?}",
                                match_block_content[old_idx]
                            );
                            // Restore truncated context at EOF
                            let line = match_block_content[old_idx];
                            // Adjust it to match target style? Best effort using last known.
                            final_lines.push(adjust_indentation(
                                line,
                                current_hunk_indent,
                                current_target_indent,
                            ));
                            for add in additions {
                                final_lines.push(adjust_indentation(
                                    add,
                                    current_hunk_indent,
                                    current_target_indent,
                                ));
                            }
                        } else if !*is_removal && !additions.is_empty() {
                            if is_blank_context {
                                trace!(
                                    "        Re-anchoring {} addition(s) from missing blank contextline to current target location",
                                    additions.len()
                                );
                                for add in additions {
                                    final_lines.push(adjust_indentation(
                                        add,
                                        current_hunk_indent,
                                        current_target_indent,
                                    ));
                                }
                                continue;
                            }
                            // Context line from hunk is missing in target, but had additions attached.
                            // Splicing additions whose anchor context does not exist causes syntax corruption.
                            warn!(
                                        "    Fuzzy match rejected: Context line {:?} is missing from target, cannot anchor {} addition(s).",
                                        match_block_content[old_idx],
                                        additions.len()
                                    );
                            return Err(HunkApplyError::ContextNotFound);
                        } else if *is_removal {
                            trace!(
                                "        Skipping deleted line {:?} from old hunk, applying its {} addition(s)",
                                match_block_content[old_idx],
                                additions.len()
                            );
                            for add in additions {
                                final_lines.push(adjust_indentation(
                                    add,
                                    current_hunk_indent,
                                    current_target_indent,
                                ));
                            }
                        } else {
                            trace!(
                                "        Skipping stale context line missing in target: {:?}",
                                match_block_content[old_idx]
                            );
                        }
                    }
                }
                similar::DiffOp::Insert {
                    new_index, new_len, ..
                } => {
                    trace!(
                        "      DiffOp::Insert: preserving {} local insertion line(s) from target file (new_idx={})",
                        new_len,
                        new_index
                    );
                    // Extra lines in the file (local insertions).
                    // We preserve them.
                    for i in 0..*new_len {
                        let new_idx = new_index + i;
                        trace!(
                            "        Preserving inserted line: '{}'",
                            file_matched_slice[new_idx].escape_debug()
                        );
                        final_lines.push(file_matched_slice[new_idx].clone());
                    }
                }
                similar::DiffOp::Replace {
                    old_index,
                    old_len,
                    new_index,
                    new_len,
                } => {
                    trace!(
                        "      DiffOp::Replace: hunk lines {}..{} (len={}) vs file lines {}..{} (len={})",
                        old_index,
                        old_index + old_len,
                        old_len,
                        new_index,
                        new_index + new_len,
                        new_len
                    );
                    // A region where the file differs significantly from the hunk.
                    // Try to update indentation from the first non-empty line of the replacement block
                    if *old_len > 0 && *new_len > 0 {
                        let min_len = std::cmp::min(*old_len, *new_len);
                        for i in 0..min_len {
                            let h_line = match_block_content[*old_index + i];
                            let t_line = &file_matched_slice[*new_index + i];
                            let h_ind = get_indent(h_line);
                            let t_ind = get_indent(t_line);
                            if (!h_ind.is_empty() || !t_ind.is_empty())
                                && !h_line.trim().is_empty()
                                && !t_line.trim().is_empty()
                            {
                                if current_hunk_indent != h_ind || current_target_indent != t_ind {
                                    trace!("      Dynamic Indentation Update (Replace search): Hunk='{}', Target='{}'", h_ind.escape_debug(), t_ind.escape_debug());
                                    current_hunk_indent = h_ind;
                                    current_target_indent = t_ind;
                                }
                                break;
                            }
                        }
                    }

                    // If lengths match, we assume a 1-to-1 correspondence (e.g. whitespace changes).
                    if *old_len == *new_len {
                        trace!("        1-to-1 length replacement. Validating similarity of modified lines...");
                        for i in 0..*old_len {
                            let old_idx = old_index + i;
                            let new_idx = new_index + i;
                            let (is_removal, additions) = &match_lines_meta[old_idx];
                            let old_trimmed = match_block_trimmed[old_idx];
                            let new_trimmed = file_block_trimmed[new_idx];

                            // Definition mismatch check
                            if is_definition(old_trimmed) != is_definition(new_trimmed) {
                                warn!(
                                    "    Fuzzy match rejected: Definition status mismatch between hunk line {:?} and target line {:?}.",
                                    old_trimmed, new_trimmed
                                );
                                return Err(HunkApplyError::ContextNotFound);
                            }

                            // Primary identifier check: function/call names must match
                            if let (Some(id_old), Some(id_new)) = (
                                extract_primary_identifier(old_trimmed),
                                extract_primary_identifier(new_trimmed),
                            ) {
                                if id_old != id_new {
                                    warn!(
                                        "    Fuzzy match rejected: Primary identifier mismatch ('{}' vs '{}') between hunk line {:?} and target line {:?}.",
                                        id_old, id_new, old_trimmed, new_trimmed
                                    );
                                    return Err(HunkApplyError::ContextNotFound);
                                }
                            }

                            let sim_words = similar::TextDiff::configure()
                                .algorithm(similar::Algorithm::Histogram)
                                .diff_words(old_trimmed, new_trimmed)
                                .ratio();
                            let old_no_ws: String =
                                old_trimmed.chars().filter(|c| !c.is_whitespace()).collect();
                            let new_no_ws: String =
                                new_trimmed.chars().filter(|c| !c.is_whitespace()).collect();
                            let sim_chars =
                                similar::TextDiff::from_chars(&old_no_ws, &new_no_ws).ratio();

                            trace!(
                                "        1-to-1 replacement line validation [line {}]: is_removal={}, word_sim={:.3}, char_sim={:.3}",
                                old_idx,
                                is_removal,
                                sim_words,
                                sim_chars
                            );

                            if !*is_removal {
                                if !additions.is_empty() {
                                    let is_delimiter_match = lenient && {
                                        let h_norm = normalize_line_delimiters(old_trimmed);
                                        let t_norm = normalize_line_delimiters(new_trimmed);
                                        !h_norm.is_empty() && h_norm == t_norm
                                    };

                                    if !is_delimiter_match && (sim_words < 0.5 || sim_chars < 0.6) {
                                        warn!(
                                            "    Fuzzy match rejected: Context line {:?} differs completely from target line {:?}, cannot anchor {} addition(s).",
                                            match_block_content[old_idx],
                                            file_matched_slice[new_idx],
                                            additions.len()
                                        );
                                        return Err(HunkApplyError::ContextNotFound);
                                    }
                                } else if sim_words < 0.4 && sim_chars < 0.5 {
                                    let is_delimiter_match = lenient && {
                                        let h_norm = normalize_line_delimiters(old_trimmed);
                                        let t_norm = normalize_line_delimiters(new_trimmed);
                                        !h_norm.is_empty() && h_norm == t_norm
                                    };

                                    if !is_delimiter_match {
                                        warn!(
                                        "    Fuzzy match rejected: Context line {:?} differs completely from target line {:?}.",
                                        match_block_content[old_idx],
                                        file_matched_slice[new_idx]
                                    );
                                        return Err(HunkApplyError::ContextNotFound);
                                    }
                                }
                            } else {
                                let is_delimiter_match = lenient && {
                                    let h_norm = normalize_line_delimiters(old_trimmed);
                                    let t_norm = normalize_line_delimiters(new_trimmed);
                                    !h_norm.is_empty() && h_norm == t_norm
                                };
                                let req_threshold = if lenient {
                                    (options.fuzz_factor * 0.5).min(0.40)
                                } else {
                                    options.fuzz_factor.min(0.50)
                                };
                                if !is_delimiter_match
                                    && sim_words < req_threshold
                                    && sim_chars < req_threshold
                                {
                                    warn!(
                                        "    Fuzzy match rejected: Removal line {:?} differs from target line {:?} (sim_words={:.3}, sim_chars={:.3}, required={:.3}).",
                                        match_block_content[old_idx],
                                        file_matched_slice[new_idx],
                                        sim_words,
                                        sim_chars,
                                        req_threshold
                                    );
                                    return Err(HunkApplyError::ContextNotFound);
                                }
                            }

                            let h_line = match_block_content[old_idx];
                            let t_line = &file_matched_slice[new_idx];
                            let h_ind = get_indent(h_line);
                            let t_ind = get_indent(t_line);
                            if (!h_ind.is_empty() || !t_ind.is_empty())
                                && !h_line.trim().is_empty()
                                && !t_line.trim().is_empty()
                                && (current_hunk_indent != h_ind || current_target_indent != t_ind)
                            {
                                trace!("      Dynamic Indentation Update (Replace match): Hunk='{}', Target='{}'", h_ind.escape_debug(), t_ind.escape_debug());
                                current_hunk_indent = h_ind;
                                current_target_indent = t_ind;
                            }

                            if !*is_removal {
                                trace!(
                                    "        Replace (1-to-1): preserving context line: '{}'",
                                    file_matched_slice[new_idx].escape_debug()
                                );
                                final_lines.push(file_matched_slice[new_idx].clone());
                            } else {
                                trace!(
                                    "        Replace (1-to-1): removing line: '{}'",
                                    match_block_content[old_idx].escape_debug()
                                );
                            }
                            for add in additions {
                                let adj = adjust_indentation(
                                    add,
                                    current_hunk_indent,
                                    current_target_indent,
                                );
                                trace!(
                                    "        Replace (1-to-1): applied addition: '{}'",
                                    adj.escape_debug()
                                );
                                final_lines.push(adj);
                            }
                        }
                    } else {
                        trace!(
                            "        Multi-line replacement (hunk_len={}, target_len={}). Searching for statement alignment across line breaks...",
                            old_len,
                            new_len
                        );
                        let mut has_context = false;
                        for i in 0..*old_len {
                            if !match_lines_meta[old_index + i].0 {
                                has_context = true;
                                break;
                            }
                        }

                        let match_in_new = find_statement_match_in_block(
                            &match_block_content[*old_index..*old_index + *old_len],
                            &file_matched_slice[*new_index..*new_index + *new_len],
                            has_context,
                        );

                        if let Some(matching_sub_idx) = match_in_new {
                            let absolute_target_match = *new_index + matching_sub_idx;
                            debug!(
                                "        Statement match aligned with target line {} (relative offset {})",
                                absolute_target_match + 1,
                                matching_sub_idx
                            );
                            for line in &file_matched_slice[*new_index..absolute_target_match] {
                                final_lines.push(line.clone());
                            }

                            let t_line = &file_matched_slice[absolute_target_match];
                            let t_ind = get_indent(t_line);
                            if !t_ind.is_empty() && !t_line.trim().is_empty() {
                                current_target_indent = t_ind;
                            }

                            for i in 0..*old_len {
                                let old_idx = old_index + i;
                                let (is_removal, additions) = &match_lines_meta[old_idx];
                                if !*is_removal {
                                    let h_line = match_block_content[old_idx];
                                    let adj = adjust_indentation(
                                        h_line,
                                        current_hunk_indent,
                                        current_target_indent,
                                    );
                                    trace!(
                                        "        Statement match: preserving context line: '{}'",
                                        adj.escape_debug()
                                    );
                                    final_lines.push(adj);
                                }
                                for add in additions {
                                    let adj = adjust_indentation(
                                        add,
                                        current_hunk_indent,
                                        current_target_indent,
                                    );
                                    trace!(
                                        "        Statement match: applied addition: '{}'",
                                        adj.escape_debug()
                                    );
                                    final_lines.push(adj);
                                }
                            }

                            for line in &file_matched_slice
                                [(absolute_target_match + 1)..(*new_index + *new_len)]
                            {
                                final_lines.push(line.clone());
                            }
                        } else {
                            trace!(
                                "        No single statement alignment found (has_context={}). Using block fallback heuristic.",
                                has_context
                            );

                            let has_substantive_context = (0..*old_len).any(|i| {
                                let (is_removal, _) = &match_lines_meta[old_index + i];
                                !is_removal && !match_block_content[old_index + i].trim().is_empty()
                            });

                            // 1. Head context cannot be unaligned in replacement
                            if *old_index == 0 && has_substantive_context {
                                warn!("    Fuzzy match rejected: Head context is unaligned in replacement block.");
                                return Err(HunkApplyError::ContextNotFound);
                            }

                            // 2. Tail context cannot be unaligned in replacement
                            if *old_index + *old_len == match_block_content.len()
                                && has_substantive_context
                            {
                                warn!("    Fuzzy match rejected: Tail context is unaligned in replacement block.");
                                return Err(HunkApplyError::ContextNotFound);
                            }

                            // 3. Definitions cannot be unaligned in replacement
                            for i in 0..*old_len {
                                let line = match_block_content[old_index + i];
                                if is_definition(line) {
                                    warn!(
                                        "    Fuzzy match rejected: Definition {:?} is unaligned in replacement block.",
                                        line
                                    );
                                    return Err(HunkApplyError::ContextNotFound);
                                }
                            }

                            // 4. Context lines with additions attached cannot be unaligned
                            for i in 0..*old_len {
                                let (is_removal, additions) = &match_lines_meta[old_index + i];
                                if !*is_removal
                                    && !additions.is_empty()
                                    && !match_block_content[old_index + i].trim().is_empty()
                                {
                                    let is_reconciled = lenient && {
                                        let h_norm = normalize_line_delimiters(
                                            match_block_content[old_index + i],
                                        );
                                        (0..*new_len).any(|j| {
                                            let t_norm = normalize_line_delimiters(
                                                &file_matched_slice[new_index + j],
                                            );
                                            !h_norm.is_empty() && h_norm == t_norm
                                        })
                                    };
                                    if is_reconciled {
                                        continue;
                                    }
                                    warn!(
                                        "    Fuzzy match rejected: Context line {:?} was unaligned in replacement block, cannot anchor {} addition(s).",
                                        match_block_content[old_index + i],
                                        additions.len()
                                    );
                                    return Err(HunkApplyError::ContextNotFound);
                                }
                            }

                            if !has_context {
                                for i in 0..*old_len {
                                    let (_, additions) = &match_lines_meta[old_index + i];
                                    if !additions.is_empty() {
                                        warn!(
                                            "    Fuzzy match rejected: Removal lines were unaligned in replacement block, cannot anchor {} addition(s).",
                                            additions.len()
                                        );
                                        return Err(HunkApplyError::ContextNotFound);
                                    }
                                }
                            }

                            if has_context {
                                for i in 0..*new_len {
                                    trace!(
                                        "        Fallback: preserving target line: '{}'",
                                        file_matched_slice[new_index + i].escape_debug()
                                    );
                                    final_lines.push(file_matched_slice[new_index + i].clone());
                                }
                            }

                            // Always append additions associated with the old lines
                            for i in 0..*old_len {
                                let (_, additions) = &match_lines_meta[old_index + i];
                                for add in additions {
                                    let adj = adjust_indentation(
                                        add,
                                        current_hunk_indent,
                                        current_target_indent,
                                    );
                                    trace!(
                                        "        Fallback: applied addition: '{}'",
                                        adj.escape_debug()
                                    );
                                    final_lines.push(adj);
                                }
                            }
                        }
                    }
                }
            }
        }
        debug!(
            "    Robust reconstruction complete: produced {} replacement line(s) for {} target line(s).",
            final_lines.len(),
            location.length
        );
        final_lines
    };

    trace!(
        "  Splicing final replacement block into target lines: range [{}..{}], replacement line count={}",
        location.start_index,
        location.start_index + location.length,
        final_replace_block.len()
    );

    let replaced_lines: Vec<String> = target_lines
        .splice(
            location.start_index..location.start_index + location.length,
            final_replace_block,
        )
        .collect();
    info!(
        "  try_apply_hunk_at_location: successfully spliced hunk at line {} (replaced {} line(s), resulting target lines={})",
        location.start_index + 1,
        replaced_lines.len(),
        target_lines.len()
    );
    trace!("    Replaced lines: {:?}", replaced_lines);
    Ok(HunkApplyStatus::Applied {
        location,
        match_type,
        replaced_lines,
    })
}

/// A trait for strategies that find the location to apply a hunk.
///
/// This allows the core matching algorithm to be pluggable, enabling different
/// search strategies to be used if needed.
/// The library provides a robust [`DefaultHunkFinder`] that should be sufficient
/// for most use cases.
///
/// # Arguments
///
/// * `hunk` - The hunk to locate.
/// * `target_lines` - The content to search within.
///
/// # Returns
///
/// A tuple containing the [`HunkLocation`] and the [`MatchType`] on success.
///
/// # Errors
///
/// Returns `Err(`[`HunkApplyError`]`)` if no suitable location could be found.
///
/// # Examples
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
    /// # Arguments
    ///
    /// * `hunk` - The [`Hunk`] to locate.
    /// * `target_lines` - A slice of strings representing the content to search within.
    ///
    /// # Returns
    ///
    /// A tuple containing the [`HunkLocation`] and the [`MatchType`] on success.
    ///
    /// # Errors
    ///
    /// Returns `Err(`[`HunkApplyError`]`)` if no suitable location could be found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, DefaultHunkFinder, HunkFinder, ApplyOptions, HunkLocation, MatchType};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // 1. Create a hunk to search for.
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n line 1\n-line 2\n+line two\n```";
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
/// It uses line number hints from the patch to resolve ambiguities and supports
/// candidate location enumeration via [`find_candidate_locations`](DefaultHunkFinder::find_candidate_locations)
/// for backtracking during patch application.
///
/// While you can use this struct directly, it's typically used internally by
/// functions like [`find_hunk_location_in_lines()`].
///
/// # Examples
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
    /// Active configuration options governing fuzzy thresholds and matching strictness.
    options: &'a ApplyOptions,
}

/// Precomputed contiguous buffers, stripped references, and byte offset indices for target lines.
///
/// Hoisting these precomputations avoids heap string allocations and redundant iterations
/// when scoring thousands of candidate sliding windows during fuzzy search.
struct TargetPrecomputed<'a> {
    /// Target lines with leading whitespace trimmed for loose comparisons.
    target_loose_refs: Vec<&'a str>,
    /// Contiguous buffer of target lines joined by newline characters.
    target_content: String,
    /// Byte start offset of each line within `target_content`.
    line_starts: Vec<usize>,
    /// Byte end offset of each line within `target_content`.
    line_ends: Vec<usize>,
    /// Contiguous buffer of loose target lines joined by newline characters.
    target_loose_content: String,
    /// Byte start offset of each loose line within `target_loose_content`.
    loose_line_starts: Vec<usize>,
    /// Byte end offset of each loose line within `target_loose_content`.
    loose_line_ends: Vec<usize>,
    /// Contiguous buffer of all non-whitespace characters in target lines.
    target_no_ws_content: String,
    /// Offset of each line's non-whitespace content in `target_no_ws_content`.
    no_ws_line_starts: Vec<usize>,
    /// End offset of each line's non-whitespace content in `target_no_ws_content`.
    no_ws_line_ends: Vec<usize>,
}

#[inline(always)]
fn hash_str_fast(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in s.as_bytes() {
        h = (h ^ (b as u64)).wrapping_mul(0x100000001b3);
    }
    h
}

/// Precomputes contiguous target slices, stripped buffers, and character boundary indices.
///
/// Builds single-allocation strings for exact and whitespace-trimmed lines, and records
/// slice offsets to allow zero-copy subslice creation for candidate windows.
///
/// # Arguments
///
/// * `target_refs` - Slice of trimmed string references for each line in the target file.
///
/// # Returns
///
/// A [`TargetPrecomputed`] instance containing pre-allocated buffers and indexing tables.
fn precompute_target<'a>(target_refs: &[&'a str]) -> TargetPrecomputed<'a> {
    let n = target_refs.len();
    let mut target_loose_refs = Vec::with_capacity(n);
    let mut line_starts = Vec::with_capacity(n);
    let mut line_ends = Vec::with_capacity(n);
    let mut loose_line_starts = Vec::with_capacity(n);
    let mut loose_line_ends = Vec::with_capacity(n);
    let mut no_ws_line_starts = Vec::with_capacity(n);
    let mut no_ws_line_ends = Vec::with_capacity(n);

    let mut total_ref_len = 0;
    let mut total_loose_len = 0;
    let mut total_no_ws_len = 0;

    for &line in target_refs {
        let loose = line.trim_start();
        target_loose_refs.push(loose);
        total_ref_len += line.len() + 1;
        total_loose_len += loose.len() + 1;
        total_no_ws_len += line.len();
    }

    let mut target_content = String::with_capacity(total_ref_len);
    let mut target_loose_content = String::with_capacity(total_loose_len);
    let mut target_no_ws_content = String::with_capacity(total_no_ws_len);

    for i in 0..n {
        let line = target_refs[i];
        let loose = target_loose_refs[i];

        if i > 0 {
            target_content.push('\n');
        }
        let start = target_content.len();
        target_content.push_str(line);
        let end = target_content.len();
        line_starts.push(start);
        line_ends.push(end);

        if i > 0 {
            target_loose_content.push('\n');
        }
        let l_start = target_loose_content.len();
        target_loose_content.push_str(loose);
        let l_end = target_loose_content.len();
        loose_line_starts.push(l_start);
        loose_line_ends.push(l_end);

        let nw_start = target_no_ws_content.len();
        for c in line.chars() {
            if !c.is_whitespace() {
                target_no_ws_content.push(c);
            }
        }
        let nw_end = target_no_ws_content.len();
        no_ws_line_starts.push(nw_start);
        no_ws_line_ends.push(nw_end);
    }

    TargetPrecomputed {
        target_loose_refs,
        target_content,
        line_starts,
        line_ends,
        target_loose_content,
        loose_line_starts,
        loose_line_ends,
        target_no_ws_content,
        no_ws_line_starts,
        no_ws_line_ends,
    }
}

/// Precomputed representation of a hunk's match block used during fuzzy search scoring.
///
/// Contains pre-joined strings, stripped slices, non-whitespace representations, and character
/// metrics for fast evaluation against candidate target windows.
struct MatchPrecomputed<'a> {
    /// Match block lines with trailing whitespace removed.
    stripped_lines: Vec<&'a str>,
    /// Match block lines with both leading and trailing whitespace trimmed.
    loose_lines: Vec<&'a str>,
    /// Contiguous string of stripped lines joined by newline characters.
    content: String,
    /// Contiguous string of loose lines joined by newline characters.
    loose_content: String,
    /// All non-whitespace characters from `content` concatenated.
    no_ws: String,
    /// Total count of non-whitespace characters in `no_ws`.
    no_ws_chars: usize,
    /// Indicates whether `no_ws` contains exclusively ASCII characters.
    is_ascii: bool,
    hunk_hashes: std::collections::HashSet<u64>,
    loose_hunk_hashes: std::collections::HashSet<u64>,
    /// Number of lines in the match block.
    len: usize,
}

/// Precomputes match block lines, stripped content strings, and character statistics.
///
/// Strips whitespace from match block lines, concatenates them into contiguous buffers,
/// and computes non-whitespace character metrics for fast bounding during fuzzy scoring.
///
/// # Arguments
///
/// * `match_block` - The slice of lines to be matched in the target file.
///
/// # Returns
///
/// A [`MatchPrecomputed`] instance containing the pre-calculated metrics.
fn precompute_match<'a>(match_block: &[&'a str]) -> MatchPrecomputed<'a> {
    let stripped_lines: Vec<&str> = match_block.iter().map(|s| s.trim_end()).collect();
    let content = stripped_lines.join("\n");
    let loose_lines: Vec<&str> = match_block.iter().map(|s| s.trim()).collect();
    let loose_content = loose_lines.join("\n");
    let no_ws: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    let no_ws_chars = no_ws.chars().count();
    let is_ascii = no_ws.is_ascii();
    let len = match_block.len();

    let hunk_hashes: std::collections::HashSet<u64> =
        stripped_lines.iter().map(|s| hash_str_fast(s)).collect();
    let loose_hunk_hashes: std::collections::HashSet<u64> =
        loose_lines.iter().map(|s| hash_str_fast(s)).collect();

    MatchPrecomputed {
        stripped_lines,
        loose_lines,
        content,
        loose_content,
        no_ws,
        no_ws_chars,
        is_ascii,
        hunk_hashes,
        loose_hunk_hashes,
        len,
    }
}

/// Borrowed window view representing a contiguous candidate slice of target lines.
///
/// References precomputed slices and string sub-slices directly without heap allocations.
struct WindowData<'w, 'a> {
    /// Slice of stripped target lines within the window.
    stripped_lines: &'w [&'a str],
    /// Slice of loose (trimmed) target lines within the window.
    loose_lines: &'w [&'a str],
    /// Contiguous subslice of target content spanning this window.
    content: &'w str,
    /// Contiguous subslice of loose target content spanning this window.
    loose_content: &'w str,
    /// Contiguous subslice of non-whitespace target content spanning this window.
    no_ws: &'w str,
}

impl<'a> TargetPrecomputed<'a> {
    /// Extracts a borrowed [`WindowData`] view for a candidate window starting at `index` of length `len`.
    ///
    /// # Arguments
    ///
    /// * `target_refs` - The original slice of trimmed line references.
    /// * `index` - 0-based starting line index of the candidate window.
    /// * `len` - Number of lines in the candidate window.
    ///
    /// # Returns
    ///
    /// A [`WindowData`] view referencing sub-slices of the precomputed buffers without heap allocations.
    fn window_data<'w>(
        &'w self,
        target_refs: &'w [&'a str],
        index: usize,
        len: usize,
    ) -> WindowData<'w, 'a> {
        WindowData {
            stripped_lines: &target_refs[index..index + len],
            loose_lines: &self.target_loose_refs[index..index + len],
            content: &self.target_content[self.line_starts[index]..self.line_ends[index + len - 1]],
            loose_content: &self.target_loose_content
                [self.loose_line_starts[index]..self.loose_line_ends[index + len - 1]],
            no_ws: &self.target_no_ws_content
                [self.no_ws_line_starts[index]..self.no_ws_line_ends[index + len - 1]],
        }
    }
}

/// Computes multi-level similarity scores between a target window and a hunk match block.
///
/// Incorporates upper-bound pruning and short-circuit evaluation to skip expensive
/// word- and character-level Myers diff calculations when line-level similarity reaches 1.0
/// or non-whitespace bounds cannot alter the window's score.
///
/// # Arguments
///
/// * `window` - The candidate target window view.
/// * `match_data` - Precomputed data for the hunk's match block.
///
/// # Returns
///
/// A tuple `(score, ratio, ratio_lines, ratio_words)` where:
/// - `score`: Scaled composite similarity score used for candidate ranking.
/// - `ratio`: Unscaled similarity ratio used for threshold comparison.
/// - `ratio_lines`: Line-level sequence similarity ratio.
/// - `ratio_words`: Word-level sequence similarity ratio.
fn score_window(
    window: &WindowData<'_, '_>,
    match_data: &MatchPrecomputed<'_>,
    threshold: f64,
) -> (f64, f64, f64, f64) {
    let window_len = window.stripped_lines.len();
    let len = match_data.len;

    let ratio_lines = if window.content == match_data.content {
        1.0
    } else {
        similar::TextDiff::configure()
            .algorithm(similar::Algorithm::Histogram)
            .diff_slices(window.stripped_lines, &match_data.stripped_lines)
            .ratio()
    };

    let scale = if window_len > len && len > 0 {
        let raw_scale = (window_len + len) as f64 / (2.0 * len as f64);
        let expansion_ratio = (window_len - len) as f64 / len as f64;
        let penalty = (1.0 - 0.05 * expansion_ratio).max(0.60);
        (raw_scale * penalty).min(raw_scale)
    } else {
        1.0
    };

    let strict_line_score = (ratio_lines as f64 * scale).min(1.0);

    let (ratio_loose_lines, line_score) = if strict_line_score >= threshold {
        (ratio_lines as f64, strict_line_score)
    } else if window.loose_content == match_data.loose_content {
        let s = (1.0 * scale).min(1.0);
        (1.0, s)
    } else if window.content == window.loose_content
        && match_data.content == match_data.loose_content
    {
        (ratio_lines as f64, strict_line_score)
    } else {
        let r_loose = similar::TextDiff::configure()
            .algorithm(similar::Algorithm::Histogram)
            .diff_slices(window.loose_lines, &match_data.loose_lines)
            .ratio();
        let loose_score = (r_loose as f64 * scale).min(1.0);
        (r_loose as f64, strict_line_score.max(loose_score))
    };

    if ratio_lines == 1.0 {
        return (1.0, 1.0, 1.0, 1.0);
    }
    if ratio_loose_lines == 1.0 && window_len == len {
        return (1.0, 1.0, ratio_lines as f64, 1.0);
    }

    if line_score >= threshold || len > 30 {
        return (line_score, line_score, ratio_lines as f64, line_score);
    }

    let max_word_bonus = if len > 10 { 0.25 } else { 0.70 };
    let max_strict =
        0.3 * ratio_lines as f64 + 0.7 * (ratio_lines as f64 + max_word_bonus).min(1.0);
    let max_loose = 0.3 * ratio_loose_lines + 0.7 * (ratio_loose_lines + max_word_bonus).min(1.0);

    let c_w = if match_data.is_ascii && window.no_ws.is_ascii() {
        window.no_ws.len()
    } else {
        window.no_ws.chars().count()
    };
    let c_m = match_data.no_ws_chars;

    let ub_no_ws = if c_w == 0 && c_m == 0 {
        1.0
    } else if c_w == 0 || c_m == 0 {
        0.0
    } else {
        2.0 * (c_w.min(c_m) as f64) / ((c_w + c_m) as f64)
    };

    let ub_very_loose = if c_m <= 500 {
        0.1 * ratio_lines as f64 + 0.9 * ub_no_ws
    } else {
        0.0
    };

    // SOUND ADMISSIBLE PRUNING:
    // If neither line_score, max_strict, max_loose, nor ub_very_loose can reach threshold,
    // the composite score CANNOT reach threshold. Skip all word and character Myers diffs!
    let theoretical_max = line_score.max(max_strict).max(max_loose).max(ub_very_loose);
    if theoretical_max < threshold {
        return (theoretical_max, theoretical_max, ratio_lines as f64, 0.0);
    }

    let (ratio_words, _ratio_loose_words, current_max) =
        if max_strict <= line_score && max_loose <= line_score {
            (0.0, 0.0, line_score)
        } else {
            let r_words = if window.content == match_data.content {
                1.0
            } else {
                similar::TextDiff::configure()
                    .algorithm(similar::Algorithm::Histogram)
                    .diff_words(window.content, &match_data.content)
                    .ratio()
            };
            let r_strict = 0.3 * ratio_lines as f64 + 0.7 * r_words as f64;
            let c_max = r_strict.max(line_score);

            let r_loose_words = if max_loose <= c_max {
                0.0
            } else if window.loose_content == match_data.loose_content {
                1.0
            } else if window.content == window.loose_content
                && match_data.content == match_data.loose_content
            {
                r_words
            } else {
                similar::TextDiff::configure()
                    .algorithm(similar::Algorithm::Histogram)
                    .diff_words(window.loose_content, &match_data.loose_content)
                    .ratio()
            };
            let r_loose = 0.3 * ratio_loose_lines + 0.7 * r_loose_words as f64;
            (r_words, r_loose_words, c_max.max(r_loose))
        };

    let ratio_no_ws = if ub_very_loose <= current_max
        || ub_very_loose < threshold
        || current_max >= threshold
        || c_m > 500
    {
        0.0
    } else if window.no_ws == match_data.no_ws {
        1.0
    } else {
        similar::TextDiff::configure()
            .algorithm(similar::Algorithm::Histogram)
            .diff_chars(window.no_ws, &match_data.no_ws)
            .ratio()
    };

    let ratio_very_loose = 0.1 * ratio_lines as f64 + 0.9 * ratio_no_ws as f64;
    let ratio = current_max.max(ratio_very_loose);
    let score = ratio;

    trace!(
        "        score_window: window_len={}, match_len={}, line_score={:.3}, ratio_lines={:.3}, final_score={:.3}",
        window_len,
        len,
        line_score,
        ratio_lines,
        score
    );

    (score, ratio, ratio_lines as f64, ratio_words as f64)
}

/// Scored candidate window within the target file during fuzzy search.
#[derive(Clone, Copy)]
struct ScoredWindow {
    /// Scaled composite similarity score used for candidate ranking.
    score: f64,
    /// Unscaled similarity ratio used for threshold comparison.
    ratio: f64,
    /// Line-level sequence similarity ratio.
    ratio_lines: f64,
    /// Word-level sequence similarity ratio.
    ratio_words: f64,
    /// 0-based starting line index of the window in the target file.
    start_index: usize,
    /// Line count (height) of the candidate window.
    window_len: usize,
}

/// Evaluates and scores candidate sliding windows across the provided search ranges in parallel using Rayon.
///
/// # Arguments
///
/// * `search_ranges` - Slice of `(start, end)` line intervals to search within.
/// * `pre` - Precomputed buffers and line boundary indices for the target file.
/// * `target_refs` - Slice of trimmed target line string references.
/// * `match_pre` - Precomputed representation of the hunk's match block.
/// * `min_len` - Minimum candidate window line count.
/// * `max_len` - Maximum candidate window line count.
///
/// # Returns
///
/// A vector of [`ScoredWindow`] objects representing all evaluated windows.
/// Context and precomputed indexing tables for scoring candidate sliding windows.
#[derive(Clone, Copy)]
struct WindowScorer<'a, 't, 'm> {
    pre: &'a TargetPrecomputed<'t>,
    target_refs: &'a [&'t str],
    match_pre: &'a MatchPrecomputed<'m>,
    match_ps: &'a [u32],
    loose_ps: &'a [u32],
    min_len: usize,
    max_len: usize,
    threshold: f64,
}

/// Evaluates and scores candidate sliding windows across the provided search ranges sequentially.
///
/// # Arguments
///
/// * `search_ranges` - Slice of `(start, end)` line intervals to search within.
/// * `pre` - Precomputed buffers and line boundary indices for the target file.
/// * `target_refs` - Slice of trimmed target line string references.
/// * `match_pre` - Precomputed representation of the hunk's match block.
/// * `min_len` - Minimum candidate window line count.
/// * `max_len` - Maximum candidate window line count.
///
/// # Returns
///
/// A vector of [`ScoredWindow`] objects representing all evaluated windows.
impl WindowScorer<'_, '_, '_> {
    fn score_candidate(
        &self,
        range_start: usize,
        i: usize,
        target_len: usize,
    ) -> Vec<ScoredWindow> {
        let start_index = range_start + i;
        let actual_max_len = self.max_len.min(target_len - i);
        let mut local_windows = Vec::new();

        if actual_max_len < self.min_len {
            return local_windows;
        }

        let m = self.match_pre.len;
        let c_m = self.match_pre.no_ws_chars;

        if m > 10 {
            let max_match_count =
                self.match_ps[start_index + actual_max_len] - self.match_ps[start_index];
            let max_loose_count =
                self.loose_ps[start_index + actual_max_len] - self.loose_ps[start_index];
            let max_count = max_match_count.max(max_loose_count) as f64;
            if m > 30 && (max_count / m as f64) < self.threshold {
                return local_windows;
            }
            let max_ub_lines = (2.0 * max_count) / (self.min_len + m) as f64;
            let max_ub_loose = (2.0 * max_count) / (self.min_len + m) as f64;
            if max_ub_lines.max(max_ub_loose) < 0.20 {
                return local_windows;
            }
        }

        let nominal_len = m.clamp(self.min_len, actual_max_len);
        let mut lengths = Vec::with_capacity(actual_max_len - self.min_len + 1);
        lengths.push(nominal_len);
        for d in 1..=(actual_max_len - self.min_len + 1) {
            if nominal_len >= self.min_len + d {
                lengths.push(nominal_len - d);
            }
            if nominal_len + d <= actual_max_len {
                lengths.push(nominal_len + d);
            }
        }

        let mut best_local_score = 0.0;
        for window_len in lengths {
            let w_match_count =
                self.match_ps[start_index + window_len] - self.match_ps[start_index];
            let w_loose_count =
                self.loose_ps[start_index + window_len] - self.loose_ps[start_index];
            let w_ub_lines = (2 * w_match_count as usize) as f64 / (window_len + m) as f64;
            let w_ub_loose = (2 * w_loose_count as usize) as f64 / (window_len + m) as f64;
            let w_scale = if window_len > m && m > 0 {
                let raw = (window_len + m) as f64 / (2.0 * m as f64);
                let expansion_ratio = (window_len - m) as f64 / m as f64;
                let penalty = (1.0 - 0.05 * expansion_ratio).max(0.60);
                (raw * penalty).min(raw)
            } else {
                1.0
            };
            let w_ub_line_score = ((w_ub_lines * w_scale).max(w_ub_loose * w_scale)).min(1.0);
            let w_theoretical_max = if m > 30 {
                w_ub_line_score
            } else {
                let max_word_bonus = if m > 10 { 0.25 } else { 0.70 };
                let w_max_strict = 0.3 * w_ub_lines + 0.7 * (w_ub_lines + max_word_bonus).min(1.0);
                let w_max_loose = 0.3 * w_ub_loose + 0.7 * (w_ub_loose + max_word_bonus).min(1.0);
                let w_c_w = self.pre.no_ws_line_ends[start_index + window_len - 1]
                    - self.pre.no_ws_line_starts[start_index];
                let w_ub_no_ws = if w_c_w == 0 && c_m == 0 {
                    1.0
                } else if w_c_w == 0 || c_m == 0 {
                    0.0
                } else {
                    2.0 * (w_c_w.min(c_m) as f64) / ((w_c_w + c_m) as f64)
                };
                let w_ub_very_loose = if c_m <= 500 {
                    0.1 * w_ub_lines + 0.9 * w_ub_no_ws
                } else {
                    0.0
                };
                w_ub_line_score
                    .max(w_max_strict)
                    .max(w_max_loose)
                    .max(w_ub_very_loose)
            };
            if w_theoretical_max < self.threshold
                || (m > 30
                    && best_local_score >= self.threshold
                    && w_theoretical_max <= best_local_score)
            {
                local_windows.push(ScoredWindow {
                    score: w_theoretical_max,
                    ratio: w_theoretical_max,
                    ratio_lines: w_ub_lines,
                    ratio_words: 0.0,
                    start_index,
                    window_len,
                });
                continue;
            }
            let window = self
                .pre
                .window_data(self.target_refs, start_index, window_len);
            let (score, ratio, ratio_lines, ratio_words) =
                score_window(&window, self.match_pre, self.threshold);
            if score > best_local_score {
                best_local_score = score;
            }
            local_windows.push(ScoredWindow {
                score,
                ratio,
                ratio_lines,
                ratio_words,
                start_index,
                window_len,
            });
        }
        local_windows
    }
}

/// Evaluates and scores candidate sliding windows across the provided search ranges.
///
/// When the `parallel` feature is enabled, evaluations are performed concurrently across
/// threads using Rayon; otherwise, search ranges are processed sequentially.
///
/// # Arguments
///
/// * `search_ranges` - Slice of `(start, end)` line intervals to search within.
/// * `pre` - Precomputed buffers and line boundary indices for the target file.
/// * `target_refs` - Slice of trimmed target line string references.
/// * `match_pre` - Precomputed representation of the hunk's match block.
/// * `min_len` - Minimum candidate window line count.
/// * `max_len` - Maximum candidate window line count.
/// * `threshold` - Minimum similarity threshold for candidate acceptance.
///
/// # Returns
///
/// A vector of [`ScoredWindow`] objects representing all evaluated windows.
fn compute_scored_windows(
    search_ranges: &[(usize, usize)],
    pre: &TargetPrecomputed<'_>,
    target_refs: &[&str],
    match_pre: &MatchPrecomputed<'_>,
    min_len: usize,
    max_len: usize,
    threshold: f64,
) -> Vec<ScoredWindow> {
    let total_candidates: usize = search_ranges
        .iter()
        .map(|&(start, end)| {
            let t_len = end.saturating_sub(start);
            (min_len..=max_len)
                .map(|w_len| if w_len <= t_len { t_len - w_len + 1 } else { 0 })
                .sum::<usize>()
        })
        .sum();

    let mode = if cfg!(feature = "parallel") {
        "parallel"
    } else {
        "sequential"
    };

    debug!(
        "      compute_scored_windows ({}): evaluating {} candidate window(s) across {} range(s) (window lengths {}..={})",
        mode,
        total_candidates,
        search_ranges.len(),
        min_len,
        max_len
    );
    trace!(
        "        Match block length: {}, Target line count: {}, Search ranges: {:?}",
        match_pre.len,
        target_refs.len(),
        search_ranges
    );

    let n = target_refs.len();
    let mut match_prefix_sum = Vec::with_capacity(n + 1);
    let mut loose_match_prefix_sum = Vec::with_capacity(n + 1);
    match_prefix_sum.push(0u32);
    loose_match_prefix_sum.push(0u32);

    let mut cur_match = 0u32;
    let mut cur_loose = 0u32;
    for (&stripped, &loose) in target_refs.iter().zip(&pre.target_loose_refs) {
        if match_pre.hunk_hashes.contains(&hash_str_fast(stripped)) {
            cur_match += 1;
        }
        if match_pre.loose_hunk_hashes.contains(&hash_str_fast(loose)) {
            cur_loose += 1;
        }
        match_prefix_sum.push(cur_match);
        loose_match_prefix_sum.push(cur_loose);
    }

    let scorer = WindowScorer {
        pre,
        target_refs,
        match_pre,
        match_ps: &match_prefix_sum,
        loose_ps: &loose_match_prefix_sum,
        min_len,
        max_len,
        threshold,
    };

    #[cfg(feature = "parallel")]
    let windows: Vec<ScoredWindow> = search_ranges
        .par_iter()
        .flat_map(|&(range_start, range_end)| {
            let target_len = range_end.saturating_sub(range_start);
            (0..=target_len.saturating_sub(min_len))
                .into_par_iter()
                .flat_map(move |i| scorer.score_candidate(range_start, i, target_len))
        })
        .collect();

    #[cfg(not(feature = "parallel"))]
    let windows: Vec<ScoredWindow> = search_ranges
        .iter()
        .flat_map(|&(range_start, range_end)| {
            let target_len = range_end.saturating_sub(range_start);
            (0..=target_len.saturating_sub(min_len))
                .flat_map(move |i| scorer.score_candidate(range_start, i, target_len))
        })
        .collect();

    let (best_score, best_start, best_len) = windows
        .iter()
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|w| (w.score, w.start_index, w.window_len))
        .unwrap_or((-1.0, 0, 0));

    debug!(
        "      compute_scored_windows ({}) complete: scored {} window(s). Best candidate score={:.3} at line {} (len={}).",
        mode,
        windows.len(),
        best_score,
        best_start + 1,
        best_len
    );

    windows
}

impl<'a> DefaultHunkFinder<'a> {
    /// Creates a new finder with the given options.
    ///
    /// This is the standard way to instantiate the `DefaultHunkFinder`. The provided
    /// [`ApplyOptions`] will control the behavior of the finder, particularly the
    /// `fuzz_factor` which determines the threshold for fuzzy matching.
    ///
    /// # Arguments
    ///
    /// * `options` - Configuration for the patch operation.
    ///
    /// # Returns
    ///
    /// A new `DefaultHunkFinder` instance.
    ///
    /// # Examples
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

    /// Finds all candidate locations for applying a hunk in the target lines.
    ///
    /// This method evaluates potential match locations using the hierarchical search
    /// strategy (exact matching, whitespace-insensitive matching, and flexible-window
    /// fuzzy matching). The returned candidate locations are sorted by score and
    /// preference. Candidates whose match window length is smaller than the hunk's
    /// [`required_match_span()`](Hunk::required_match_span) are pruned to prevent attempting
    /// windows too short to accommodate all edits.
    ///
    /// This is used internally by [`apply_hunk_to_lines()`] to backtrack across candidate
    /// locations if an initial match candidate fails during hunk reconstruction (for example,
    /// when an addition cannot be cleanly anchored without corrupting surrounding context).
    ///
    /// # Arguments
    ///
    /// * `hunk` - The [`Hunk`] to locate.
    /// * `target_lines` - A slice of strings representing the content to search within.
    ///
    /// # Returns
    ///
    /// A vector of `(HunkLocation, MatchType)` pairs on success, ordered from highest to lowest preference.
    ///
    /// # Errors
    ///
    /// Returns `Err(`[`HunkApplyError`]`)` if no suitable candidate locations could be found, or
    /// if all potential candidates fail the required minimum match span.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, DefaultHunkFinder, ApplyOptions, HunkLocation, MatchType};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let diff = "```diff\n--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +1,2 @@\n line 1\n-line 2\n+line two\n```";
    /// let hunk = parse_single_patch(diff)?.hunks.remove(0);
    /// let target_lines = vec!["line 1", "line 2"];
    /// let options = ApplyOptions::new();
    /// let finder = DefaultHunkFinder::new(&options);
    ///
    /// let candidates = finder.find_candidate_locations(&hunk, &target_lines)?;
    /// assert_eq!(candidates.len(), 1);
    /// assert_eq!(candidates[0].0, HunkLocation { start_index: 0, length: 2 });
    /// assert!(matches!(candidates[0].1, MatchType::Exact));
    /// # Ok(())
    /// # }
    /// ```
    pub fn find_candidate_locations<T: AsRef<str> + Sync>(
        &self,
        hunk: &Hunk,
        target_lines: &[T],
    ) -> Result<Vec<(HunkLocation, MatchType)>, HunkApplyError> {
        let match_block = hunk.get_match_block();
        let min_span = hunk.required_match_span();
        trace!(
            "DefaultHunkFinder::find_candidate_locations: match block len={}, required_match_span={}, target lines={}",
            match_block.len(),
            min_span,
            target_lines.len()
        );
        let mut candidates = self.find_hunk_location_internal(
            &match_block,
            target_lines,
            hunk.old_start_line,
            hunk.new_start_line,
            min_span,
        )?;
        if min_span > 0 {
            let before = candidates.len();
            candidates.retain(|(loc, _)| loc.length >= min_span);
            trace!(
                "  Pruned candidates by min_span {}: {} -> {} candidate(s)",
                min_span,
                before,
                candidates.len()
            );
            if candidates.is_empty() {
                warn!(
                    "  All candidate locations were shorter than required match span {}",
                    min_span
                );
                return Err(HunkApplyError::ContextNotFound);
            }
        }
        trace!(
            "DefaultHunkFinder::find_candidate_locations: returning {} candidate(s)",
            candidates.len()
        );
        Ok(candidates)
    }

    /// Finds optimized search ranges within the target file to perform the fuzzy search.
    ///
    /// This is a performance heuristic. It tries to find an "anchor" line from the
    /// hunk that is relatively uncommon in the target file. If successful, it returns
    /// small search windows around the occurrences of that anchor. If no good anchor
    /// is found, it returns a single range covering the entire file.
    ///
    /// # Arguments
    ///
    /// * `match_block` - The lines that need to be matched in the target file.
    /// * `target_lines` - Slice of lines in the target file.
    /// * `hunk_size` - Number of lines in the match block.
    ///
    /// # Returns
    ///
    /// A vector of disjoint `(start_line, end_line)` tuples to scan.
    fn find_search_ranges<T: AsRef<str>>(
        match_block: &[&str],
        target_lines: &[T],
        hunk_size: usize,
        max_len: usize,
    ) -> Vec<(usize, usize)> {
        const MAX_ANCHOR_OCCURRENCES: usize = 5;
        const MIN_ANCHOR_LEN: usize = 5;
        // Search radius is this factor times the hunk size, with a minimum.
        const SEARCH_RADIUS_FACTOR: usize = 2;
        const MIN_SEARCH_RADIUS: usize = 15;
        const MAX_SEARCH_RADIUS: usize = 120;
        const MAX_CANDIDATES_TO_TEST: usize = 100;

        debug!(
            "      find_search_ranges: analyzing {} match line(s) against {} target line(s).",
            hunk_size,
            target_lines.len()
        );

        if hunk_size == 0 || target_lines.is_empty() {
            debug!("        Hunk size or target lines is 0. Returning full target range.");
            return vec![(0, target_lines.len())];
        }

        let mid = hunk_size / 2;
        // 1. Rank candidate lines across the hunk by distinctiveness / entropy.
        let mut candidates: Vec<(usize, usize)> = match_block
            .iter()
            .enumerate()
            .filter_map(|(idx, line)| {
                let trimmed = line.trim();
                if trimmed.len() < MIN_ANCHOR_LEN || is_low_entropy_line(trimmed) {
                    return None;
                }
                if !trimmed.chars().any(|c| c.is_alphanumeric()) {
                    return None;
                }
                // Score by length with a slight preference toward the middle
                let dist = idx.abs_diff(mid);
                let score = trimmed.len().min(80).saturating_sub(dist / 25);
                Some((idx, score))
            })
            .collect();

        // Test highest entropy lines first
        candidates.sort_by_key(|b| std::cmp::Reverse(b.1));

        let search_radius =
            (hunk_size * SEARCH_RADIUS_FACTOR).clamp(MIN_SEARCH_RADIUS, MAX_SEARCH_RADIUS);

        trace!(
            "        Identified {} high-entropy candidate anchor line(s) (search_radius={}, max candidates to test={})",
            candidates.len(),
            search_radius,
            MAX_CANDIDATES_TO_TEST
        );

        // Build single-pass inverted index of high-entropy trimmed lines in target file
        let mut target_index: HashMap<&str, Vec<usize>> =
            HashMap::with_capacity(target_lines.len());
        for (idx, l) in target_lines.iter().enumerate() {
            let trimmed = l.as_ref().trim();
            if trimmed.len() >= MIN_ANCHOR_LEN && !is_low_entropy_line(trimmed) {
                let entry = target_index.entry(trimmed).or_default();
                if entry.len() <= MAX_ANCHOR_OCCURRENCES + 1 {
                    entry.push(idx);
                }
            }
        }

        // Multi-anchor spatial consensus: record (estimated_start, hunk_idx, target_occurrence)
        let mut votes: HashMap<usize, usize> = HashMap::new();
        let mut anchor_records: Vec<(usize, Vec<usize>)> = Vec::new();

        for (line_idx, _score) in candidates.into_iter().take(MAX_CANDIDATES_TO_TEST) {
            let anchor_line = match_block[line_idx].trim();
            let occurrences = match target_index.get(anchor_line) {
                Some(occ) if !occ.is_empty() && occ.len() <= MAX_ANCHOR_OCCURRENCES => {
                    occ.as_slice()
                }
                _ => continue,
            };

            anchor_records.push((line_idx, occurrences.to_vec()));
            for &occ in occurrences {
                let est_start = occ.saturating_sub(line_idx);
                let bucket = est_start / 25;
                *votes.entry(bucket).or_insert(0) += 1;
            }

            // If we have collected enough anchors and found a strong consensus cluster, short-circuit
            if anchor_records.len() >= 15 {
                if let Some((_, &count)) = votes.iter().max_by_key(|(_, &c)| c) {
                    if count >= 3 {
                        break;
                    }
                }
            }
        }

        // Identify top consensus cluster
        let top_cluster = votes.into_iter().max_by_key(|(_, count)| *count);

        let mut best_anchor: Option<(usize, Vec<usize>)> = None;

        if let Some((best_bucket, count)) = top_cluster {
            if count >= 2 {
                let target_est_start = best_bucket * 25 + 12;
                // Find the anchor that votes closest to this consensus cluster
                let mut matching_anchors = Vec::new();
                for (h_idx, occs) in &anchor_records {
                    for &occ in occs {
                        let est = occ.saturating_sub(*h_idx);
                        if est.abs_diff(target_est_start) <= 50 {
                            matching_anchors.push((*h_idx, occ));
                        }
                    }
                }
                if !matching_anchors.is_empty() {
                    debug!(
                        "        Spatial consensus: {} anchor(s) agree on start near line {}",
                        matching_anchors.len(),
                        target_est_start + 1
                    );
                    let (h_idx, occ) = matching_anchors[0];
                    best_anchor = Some((h_idx, vec![occ]));
                }
            }
        }

        // Fallback to least-frequent anchor if no consensus cluster formed
        if best_anchor.is_none() && !anchor_records.is_empty() {
            anchor_records.sort_by_key(|(_, occs)| occs.len());
            best_anchor = anchor_records.into_iter().next();
        }

        if let Some((line_idx, occurrences)) = best_anchor {
            debug!(
                "      Found anchor line (hunk line {}) with {} occurrences.",
                line_idx + 1,
                occurrences.len(),
            );
            trace!("        Anchor text: '{}'", match_block[line_idx].trim());
            let mut ranges = Vec::with_capacity(occurrences.len());
            for &occurrence_idx in &occurrences {
                let estimated_start = occurrence_idx.saturating_sub(line_idx);
                let start = estimated_start.saturating_sub(search_radius);
                let end = (estimated_start + max_len + search_radius).min(target_lines.len());
                trace!(
                    "        Occurrence at target line {}: window estimated [{}..{}] (search radius +/-{})",
                    occurrence_idx + 1,
                    start + 1,
                    end,
                    search_radius
                );
                ranges.push((start, end));
            }
            trace!("        Raw ranges before merging: {:?}", ranges);
            let merged = Self::merge_ranges(ranges);
            let total_search_lines: usize = merged.iter().map(|(s, e)| e.saturating_sub(*s)).sum();
            let reduction_pct = if !target_lines.is_empty() {
                100.0 * (1.0 - (total_search_lines as f64 / target_lines.len() as f64))
            } else {
                0.0
            };
            debug!(
                "      Search ranges merged: {} disjoint range(s) covering {}/{} line(s) ({:.1}% pruned): {:?}",
                merged.len(),
                total_search_lines,
                target_lines.len(),
                reduction_pct,
                merged
            );
            return merged;
        }

        // If no good anchor was found, we must search the entire file.
        debug!(
            "      No suitable anchor line found with <= {} occurrences. Falling back to full file scan (0..{}).",
            MAX_ANCHOR_OCCURRENCES,
            target_lines.len()
        );
        trace!("        Full file scan range: [0..{}]", target_lines.len());
        vec![(0, target_lines.len())]
    }

    /// Merges a list of overlapping or adjacent ranges into a minimal set of disjoint ranges.
    ///
    /// Sorts ranges by starting line and consolidates intervals that intersect or touch.
    ///
    /// # Arguments
    ///
    /// * `ranges` - Vector of `(start, end)` line intervals.
    ///
    /// # Returns
    ///
    /// A merged vector of non-overlapping, sorted `(start, end)` line intervals.
    fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
        if ranges.is_empty() {
            trace!("merge_ranges: empty ranges input");
            return vec![];
        }
        trace!(
            "merge_ranges: merging {} input range(s): {:?}",
            ranges.len(),
            ranges
        );
        ranges.sort_unstable_by_key(|k| k.0);
        let mut merged = Vec::with_capacity(ranges.len());
        let mut current_range = ranges[0];

        for &(start, end) in &ranges[1..] {
            if start <= current_range.1 {
                // Overlap or adjacent, merge them.
                trace!(
                    "  merge_ranges: combining [{}..{}] with [{}..{}]",
                    current_range.0,
                    current_range.1,
                    start,
                    end
                );
                current_range.1 = current_range.1.max(end);
            } else {
                // No overlap, push the current range and start a new one.
                merged.push(current_range);
                current_range = (start, end);
            }
        }
        merged.push(current_range);
        trace!(
            "merge_ranges: result {} disjoint range(s): {:?}",
            merged.len(),
            merged
        );
        merged
    }

    /// Finds the starting index of the hunk's match block in the target lines.
    ///
    /// Implements the hierarchical search strategy:
    /// 1. Exact character-for-character match.
    /// 2. Exact match ignoring trailing whitespace.
    /// 3. Flexible sliding-window fuzzy matching using word and character diff ratios.
    /// 4. End-of-file truncation prefix match.
    ///
    /// # Arguments
    ///
    /// * `match_block` - The context and deletion lines to match.
    /// * `target_lines` - The lines of the file being patched.
    /// * `old_start_line` - Optional line number hint from the hunk header for tie-breaking.
    ///
    /// # Returns
    ///
    /// A list of candidate `(HunkLocation, MatchType)` matches ordered by score/preference.
    ///
    /// # Errors
    ///
    /// Returns [`HunkApplyError`] if no location satisfies the required thresholds or if ambiguity cannot be resolved.
    fn find_hunk_location_internal<T: AsRef<str> + Sync>(
        &self,
        match_block: &[&str],
        target_lines: &[T],
        old_start_line: Option<usize>,
        new_start_line: Option<usize>,
        min_span: usize,
    ) -> Result<Vec<(HunkLocation, MatchType)>, HunkApplyError> {
        let has_ellipsis = match_block.iter().any(|l| is_ellipsis_line(l));

        // If the target file literally contains the `...` line (e.g. Python stubs),
        // prefer matching it directly as exact code.
        let literal_match = if has_ellipsis && match_block.len() <= target_lines.len() {
            let exact_iter = target_lines
                .windows(match_block.len())
                .enumerate()
                .filter(|(_, window)| {
                    window
                        .iter()
                        .map(|s| s.as_ref())
                        .eq(match_block.iter().copied())
                })
                .map(|(i, _)| i);
            let match_stripped: Vec<&str> = match_block.iter().map(|s| s.trim()).collect();
            let loose_iter = target_lines
                .windows(match_block.len())
                .enumerate()
                .filter(|(_, window)| {
                    window
                        .iter()
                        .map(|s| s.as_ref().trim())
                        .eq(match_stripped.iter().copied())
                })
                .map(|(i, _)| i);
            Self::tie_break_with_line_number(exact_iter, old_start_line, "literal-exact", true)
                .ok()
                .flatten()
                .or_else(|| {
                    Self::tie_break_with_line_number(
                        loose_iter,
                        old_start_line,
                        "literal-loose",
                        true,
                    )
                    .ok()
                    .flatten()
                })
        } else {
            None
        };
        if let Some(index) = literal_match {
            return Ok(vec![(
                HunkLocation {
                    start_index: index,
                    length: match_block.len(),
                },
                MatchType::Exact,
            )]);
        }

        if has_ellipsis {
            trace!("find_hunk_location_internal: hunk contains ellipsis wildcard lines");
            let mut segments: Vec<&[&str]> = Vec::new();
            let mut current_start = 0;
            for (idx, line) in match_block.iter().enumerate() {
                if is_ellipsis_line(line) {
                    if idx > current_start {
                        segments.push(&match_block[current_start..idx]);
                    }
                    current_start = idx + 1;
                }
            }
            if current_start < match_block.len() {
                segments.push(&match_block[current_start..]);
            }
            trace!(
                "find_hunk_location_internal: partitioned match block into {} segment(s)",
                segments.len()
            );
            return self.find_wildcard_segments_location(
                &segments,
                target_lines,
                old_start_line,
                new_start_line,
            );
        }

        let match_has_entropy = match_block.iter().any(|l| !is_low_entropy_line(l));
        trace!(
            "  find_hunk_location_internal: match_block has {} lines (entropy={}), target has {} lines",
            match_block.len(),
            match_has_entropy,
            target_lines.len()
        );

        trace!(
            "  find_hunk_location_internal called for a hunk with {} lines to match against {} target lines.",
            match_block.len(),
            target_lines.len()
        );

        if match_block.is_empty() {
            trace!("    Match block is empty (pure addition).");
            if target_lines.is_empty() {
                debug!("    Pure addition into empty target file: match at (0, 0).");
                return Ok(vec![(
                    HunkLocation {
                        start_index: 0,
                        length: 0,
                    },
                    MatchType::Exact,
                )]);
            }

            // Target file is non-empty. Use line number intent.
            match old_start_line {
                // Line 0 on an existing non-empty file is a file creation conflict.
                Some(0) => {
                    warn!("    Target is not empty for file creation (line 0). Match failed.");
                    return Err(HunkApplyError::ContextNotFound);
                }
                // Line number at or beyond EOF indicates an append to the end of the file.
                Some(line) if line >= target_lines.len() => {
                    debug!(
                        "    Pure addition targeting line {} >= EOF ({}). Appending to EOF.",
                        line,
                        target_lines.len()
                    );
                    return Ok(vec![(
                        HunkLocation {
                            start_index: target_lines.len(),
                            length: 0,
                        },
                        MatchType::Exact,
                    )]);
                }
                // Line number inside the file.
                Some(line) => {
                    let target_idx = match new_start_line {
                        Some(new) if new <= line => line.saturating_sub(1).min(target_lines.len()),
                        _ => line.min(target_lines.len()),
                    };
                    debug!(
                        "    Pure addition targeting line {}. Inserting at index {}.",
                        line, target_idx
                    );
                    return Ok(vec![(
                        HunkLocation {
                            start_index: target_idx,
                            length: 0,
                        },
                        MatchType::Exact,
                    )]);
                }
                // No line number provided (e.g. unanchored conflict marker with 0 context).
                None => {
                    warn!("    Pure addition has no line number hint and target is not empty. Match failed.");
                    return Err(HunkApplyError::ContextNotFound);
                }
            }
        }

        // --- STRATEGY 1: Exact Match ---
        // The fastest and most reliable method.
        trace!(
            "    Attempting exact match for hunk (match block has {} line(s))...",
            match_block.len()
        );
        {
            let result = if match_block.len() <= target_lines.len() {
                let iter = target_lines
                    .windows(match_block.len())
                    .enumerate()
                    .filter(|(_, window)| {
                        window
                            .iter()
                            .map(|s| s.as_ref())
                            .eq(match_block.iter().copied())
                    })
                    .map(|(i, _)| i);
                Self::tie_break_with_line_number(iter, old_start_line, "exact", match_has_entropy)
            } else {
                Self::tie_break_with_line_number(
                    std::iter::empty(),
                    old_start_line,
                    "exact",
                    match_has_entropy,
                )
            };

            match result {
                Ok(Some(index)) => {
                    debug!(
                        "    Strategy 1 (Exact): found unique exact match at index {}.",
                        index
                    );
                    return Ok(vec![(
                        HunkLocation {
                            start_index: index,
                            length: match_block.len(),
                        },
                        MatchType::Exact,
                    )]);
                }
                Ok(None) => {
                    trace!("    Strategy 1 (Exact): no exact match found.");
                }
                Err(matches) => {
                    warn!(
                        "    Strategy 1 (Exact): ambiguous match at indices {:?}",
                        matches
                    );
                    return Err(HunkApplyError::AmbiguousExactMatch(matches));
                }
            }
        }

        // Pre-calculate trimmed lines for subsequent strategies.
        // This borrows string slices directly to avoid repeated allocation in loops.
        let target_refs: Vec<&str> = target_lines.iter().map(|s| s.as_ref().trim_end()).collect();
        // Create references to the trimmed strings to avoid allocations in TextDiff

        // --- STRATEGY 2: Exact Match (Ignoring Trailing Whitespace) ---
        // Handles minor formatting differences.
        trace!("    Attempting exact match (ignoring trailing whitespace) for hunk (match block has {} line(s))...", match_block.len());
        {
            let match_stripped: Vec<_> = match_block.iter().map(|s| s.trim_end()).collect();
            let result = if match_block.len() <= target_lines.len() {
                let iter = target_refs
                    .windows(match_block.len())
                    .enumerate()
                    .filter(|(_, window)| window.iter().copied().eq(match_stripped.iter().copied()))
                    .map(|(i, _)| i);
                Self::tie_break_with_line_number(
                    iter,
                    old_start_line,
                    "exact (ignoring whitespace)",
                    match_has_entropy,
                )
            } else {
                Self::tie_break_with_line_number(
                    std::iter::empty(),
                    old_start_line,
                    "exact (ignoring whitespace)",
                    match_has_entropy,
                )
            };

            match result {
                Ok(Some(index)) => {
                    debug!(
                        "    Found unique whitespace-insensitive match at index {}.",
                        index
                    );
                    return Ok(vec![(
                        HunkLocation {
                            start_index: index,
                            length: match_block.len(),
                        },
                        MatchType::ExactIgnoringWhitespace,
                    )]);
                }
                Ok(None) => {
                    trace!("    Strategy 2 (Whitespace-insensitive): no match found.");
                }
                Err(matches) => {
                    warn!(
                        "    Strategy 2 (Whitespace-insensitive): ambiguous match at indices {:?}",
                        matches
                    );
                    return Err(HunkApplyError::AmbiguousExactMatch(matches));
                }
            }
        }

        // --- STRATEGY 3: Fuzzy Match (with flexible window) ---
        // This is the core "smart" logic. If an exact match fails, we search for
        // the best-fitting slice in the target file, allowing the slice to be
        // slightly larger or smaller than the patch's context. This handles cases
        // where lines have been added or removed near the patch location.
        if self.options.fuzz_factor > 0.0 && !match_block.is_empty() {
            debug!(
                "    Strategy 3 (Fuzzy): beginning flexible window fuzzy search (threshold={:.2}, match block len={})",
                self.options.fuzz_factor,
                self.options.fuzz_factor
            );
            if log::log_enabled!(log::Level::Trace) {
                trace!(
                    "      Hunk match block ({} lines): {:?}",
                    match_block.len(),
                    match_block
                );
            }

            // Hoist invariants for performance
            let match_pre = precompute_match(match_block);

            let mut best_score = -1.0;
            let mut best_ratio_at_best_score = -1.0;
            let mut potential_matches = Vec::new(); // Vec<(start_index, length)>

            let len = match_block.len();
            // Define how far to search for different-sized windows.
            // Expand generously based on hunk size so local doc comments or inserted
            // statements don't push the target block outside the search window.
            let max_expansion = ((len / 2) + 20).clamp(15, 50);
            let min_reduction = ((len as f64 * 0.75) as usize).clamp(3, 20);
            let min_len = len.saturating_sub(min_reduction).max(1).max(min_span);
            let max_len = len.saturating_add(max_expansion);
            let fuzz_distance = max_expansion;
            trace!(
                "      Searching with window sizes from {} to {} (hunk size: {}, fuzz distance: {})",
                min_len,
                max_len,
                len,
                fuzz_distance
            );

            let threshold = f64::from(self.options.fuzz_factor);

            // Performance heuristic: narrow down the search space using anchor lines.
            let mut search_ranges =
                Self::find_search_ranges(match_block, &target_refs, len, max_len);
            trace!("    Using search ranges: {:?}", search_ranges);

            let pre = precompute_target(&target_refs);

            let mut all_scored_windows = compute_scored_windows(
                &search_ranges,
                &pre,
                &target_refs,
                &match_pre,
                min_len,
                max_len,
                threshold,
            );

            // Tiered Fallback: If search was bounded to an anchor range and no window passed the threshold,
            // the anchor may have been a coincidental collision. Fall back to a full file scan.
            let is_full_scan =
                search_ranges.len() == 1 && search_ranges[0] == (0, target_lines.len());
            if !is_full_scan
                && !all_scored_windows
                    .iter()
                    .any(|w| w.score >= threshold || w.ratio >= threshold)
            {
                debug!("    Anchored search range yielded no passing window. Falling back to full file search.");
                search_ranges = vec![(0, target_lines.len())];
                all_scored_windows = compute_scored_windows(
                    &search_ranges,
                    &pre,
                    &target_refs,
                    &match_pre,
                    min_len,
                    max_len,
                    threshold,
                );
            }

            if log::log_enabled!(log::Level::Trace) {
                let mut sorted_windows = all_scored_windows.clone();
                sorted_windows.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                trace!("      Top fuzzy match candidates:");
                for w in sorted_windows.iter().take(5) {
                    let window_content: Vec<_> =
                        target_refs[w.start_index..w.start_index + w.window_len].to_vec();
                    trace!(
                        "        - Index {}, Len {}: Score {:.3} (Ratio {:.3}) | Content: {:?}",
                        w.start_index,
                        w.window_len,
                        w.score,
                        w.ratio,
                        window_content
                    );
                }
            }

            // Process the collected results sequentially to find the best match and handle tie-breaking.
            for w in &all_scored_windows {
                if w.score > best_score {
                    trace!(
                        "        New best score: {:.3} (ratio {:.3} [l:{:.3},w:{:.3}]) at index {} (window len {})",
                        w.score,
                        w.ratio,
                        w.ratio_lines,
                        w.ratio_words,
                        w.start_index,
                        w.window_len
                    );
                    best_score = w.score;
                    best_ratio_at_best_score = w.ratio;
                    potential_matches.clear();
                    potential_matches.push((w.start_index, w.window_len));
                } else if f64::abs(w.score - best_score) < 1e-9 {
                    // Tie in score. Prefer the one with the higher raw similarity ratio,
                    // as it indicates a better content match before size penalties.
                    if potential_matches.is_empty() {
                        potential_matches.push((w.start_index, w.window_len));
                        continue;
                    }

                    if w.ratio > best_ratio_at_best_score {
                        // This is a better match despite the same score (e.g., less penalty, more similarity)
                        trace!(
                            "        Tie in score ({:.3}), but new ratio {:.3} is better than old {:.3}. New best.",
                            w.score,
                            w.ratio,
                            best_ratio_at_best_score
                        );
                        best_ratio_at_best_score = w.ratio;
                        potential_matches.clear();
                        potential_matches.push((w.start_index, w.window_len));
                    } else if f64::abs(w.ratio - best_ratio_at_best_score) < 1e-9 {
                        // Also a tie in ratio, so it's a true ambiguity
                        trace!(
                            "        Tie in score ({:.3}) and ratio ({:.3}). Adding candidate: index {}, len {}",
                            w.score,
                            w.ratio,
                            w.start_index,
                            w.window_len
                        );
                        potential_matches.push((w.start_index, w.window_len));
                    }
                }
            }

            let threshold = f64::from(self.options.fuzz_factor);
            let mut passing: Vec<(f64, usize, usize)> = all_scored_windows
                .iter()
                .filter(|w| w.score >= threshold || w.ratio >= threshold)
                .map(|w| (w.score, w.start_index, w.window_len))
                .collect();
            passing.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            debug!(
                "    Strategy 3 (Fuzzy): {} window(s) met threshold {:.2}",
                passing.len(),
                threshold
            );
            trace!(
                "      Top 3 passing candidates: {:?}",
                passing
                    .iter()
                    .take(3)
                    .map(|&(sc, st, ln)| (format!("{:.3}", sc), st + 1, ln))
                    .collect::<Vec<_>>()
            );

            if !passing.is_empty() {
                let mut candidates: Vec<(HunkLocation, MatchType)> = Vec::new();
                let top_score = passing[0].0;

                let mut top_candidates = Vec::new();
                for &(score, start, len) in &passing {
                    if (top_score - score).abs() < 1e-9
                        && !top_candidates.iter().any(|&(s, _)| s == start)
                    {
                        top_candidates.push((start, len));
                    }
                }

                if top_candidates.len() > 1 {
                    trace!(
                        "    Tie in top fuzzy score ({:.3}) across {} candidate locations: {:?}",
                        top_score,
                        top_candidates.len(),
                        top_candidates
                    );
                    let iter = top_candidates.iter().map(|&(start, _)| start);
                    match Self::tie_break_with_line_number(
                        iter,
                        old_start_line,
                        "fuzzy",
                        match_has_entropy,
                    ) {
                        Ok(Some(best_start)) => {
                            let best_len = top_candidates
                                .iter()
                                .find(|&&(s, _)| s == best_start)
                                .unwrap()
                                .1;
                            debug!(
                                "    Tie broken using line number hint ({:?}): selected line {} (window len {})",
                                old_start_line,
                                best_start + 1,
                                best_len
                            );
                            candidates.push((
                                HunkLocation {
                                    start_index: best_start,
                                    length: best_len,
                                },
                                MatchType::Fuzzy { score: top_score },
                            ));
                        }
                        Ok(None) => unreachable!(),
                        Err(matches) => {
                            warn!(
                                "    Fuzzy match ambiguity could not be resolved at lines {:?}",
                                matches
                            );
                            let locs = top_candidates
                                .into_iter()
                                .filter(|&(s, _)| matches.contains(&s))
                                .collect();
                            return Err(HunkApplyError::AmbiguousFuzzyMatch(locs));
                        }
                    }
                } else {
                    let (start, len) = top_candidates[0];
                    debug!(
                        "    Top fuzzy candidate: start line {} (len={}, score={:.3})",
                        start + 1,
                        len,
                        top_score
                    );
                    candidates.push((
                        HunkLocation {
                            start_index: start,
                            length: len,
                        },
                        MatchType::Fuzzy { score: top_score },
                    ));
                }

                let mut counts_per_start = HashMap::new();
                for (loc, _) in &candidates {
                    *counts_per_start.entry(loc.start_index).or_insert(0) += 1;
                }

                for &(score, start, len) in &passing {
                    if min_span > 0 && len < min_span {
                        continue;
                    }
                    let count = counts_per_start.entry(start).or_insert(0);
                    if *count < 5
                        && !candidates
                            .iter()
                            .any(|(loc, _)| loc.start_index == start && loc.length == len)
                    {
                        trace!(
                            "      Adding candidate location at line {} (len={}, score={:.3})",
                            start + 1,
                            len,
                            score
                        );
                        candidates.push((
                            HunkLocation {
                                start_index: start,
                                length: len,
                            },
                            MatchType::Fuzzy { score },
                        ));
                        *count += 1;
                    }
                    if candidates.len() >= 20 {
                        trace!("      Reached maximum candidate limit (20), stopping candidate collection");
                        break;
                    }
                }
                debug!(
                    "    Strategy 3 (Fuzzy): selected {} candidate location(s)",
                    candidates.len()
                );
                return Ok(candidates);
            } else if best_ratio_at_best_score >= 0.0 {
                warn!("    Fuzzy match failed: best score was below threshold");
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
            trace!("    Strategy 4: Target file ({} lines) is shorter than hunk ({} lines). Attempting end-of-file fuzzy match...", target_lines.len(), match_block.len());
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
                return Ok(vec![(
                    HunkLocation {
                        start_index: 0,
                        length: target_lines.len(),
                    },
                    MatchType::Fuzzy {
                        score: ratio as f64,
                    },
                )]);
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
    ///
    /// # Arguments
    ///
    /// * `matches` - Iterator yielding 0-based starting line indices of candidate matches.
    /// * `start_line` - Optional 1-based line number hint from the hunk header.
    /// * `match_type` - Descriptive name of the match strategy for diagnostic logging.
    /// * `has_sufficient_entropy` - Whether the match block contains non-trivial syntax tokens.
    ///
    /// # Returns
    ///
    /// `Ok(Some(index))` if a unique best match was determined, or `Ok(None)` if no matches were provided.
    ///
    /// # Errors
    ///
    /// Returns `Err(all_matches)` containing the conflicting match indices if ambiguity cannot be broken.
    fn tie_break_with_line_number(
        mut matches: impl Iterator<Item = usize>,
        start_line: Option<usize>,
        match_type: &str,
        has_sufficient_entropy: bool,
    ) -> Result<Option<usize>, Vec<usize>> {
        trace!(
            "tie_break_with_line_number: strategy='{}', hint={:?}, entropy={}",
            match_type,
            start_line,
            has_sufficient_entropy
        );
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

            if !has_sufficient_entropy {
                warn!(
                    "      Ambiguous {} match: Refusing to tie-break {} matches because match block has low entropy.",
                    match_type,
                    all_matches.len()
                );
                return Err(all_matches);
            }

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
                    let distance = (match_index + 1).abs_diff(line);
                    trace!(
                        "      Candidate index {}: distance from line hint {} is {}",
                        match_index,
                        line,
                        distance
                    );
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

    /// Locates candidate target windows for a multi-segment hunk split by wildcard ellipsis lines.
    ///
    /// Matches individual segments in the target lines and chains consecutive segments together
    /// within bounded gap distances, ensuring anchors appear in the correct forward sequence without
    /// runaway gaps or low-entropy anchor corruption.
    ///
    /// # Arguments
    ///
    /// * `segments` - Slice of non-ellipsis line segments to match in sequence.
    /// * `target_lines` - The lines of the file being patched.
    /// * `old_start_line` - Optional line number hint from the hunk header for tie-breaking.
    ///
    /// # Returns
    ///
    /// A vector of candidate `(HunkLocation, MatchType)` tuples ordered by proximity and gap compactness.
    ///
    /// # Errors
    ///
    /// Returns [`HunkApplyError::ContextNotFound`] if segments cannot be matched in sequence.
    fn find_wildcard_segments_location<T: AsRef<str> + Sync>(
        &self,
        segments: &[&[&str]],
        target_lines: &[T],
        old_start_line: Option<usize>,
        new_start_line: Option<usize>,
    ) -> Result<Vec<(HunkLocation, MatchType)>, HunkApplyError> {
        debug!(
            "find_wildcard_segments_location: locating {} segment(s) across {} target lines (hint={:?})",
            segments.len(),
            target_lines.len(),
            old_start_line
        );
        if segments.is_empty() {
            warn!("find_wildcard_segments_location: empty segments slice");
            return Err(HunkApplyError::ContextNotFound);
        }
        if segments.iter().any(|seg| is_low_entropy_segment(seg)) {
            warn!("find_wildcard_segments_location: one or more segments consist solely of low-entropy syntax");
            return Err(HunkApplyError::ContextNotFound);
        }
        if segments.len() == 1 {
            trace!(
                "find_wildcard_segments_location: single segment, delegating to internal finder"
            );
            return self.find_hunk_location_internal(
                segments[0],
                target_lines,
                old_start_line,
                new_start_line,
                0,
            );
        }

        let target_refs: Vec<&str> = target_lines.iter().map(|s| s.as_ref().trim_end()).collect();
        let target_loose: Vec<&str> = target_lines.iter().map(|s| s.as_ref().trim()).collect();

        let mut segment_matches: Vec<Vec<(usize, usize)>> = Vec::with_capacity(segments.len());
        for (seg_i, seg) in segments.iter().enumerate() {
            let mut matches_for_seg = Vec::new();
            if seg.len() <= target_lines.len() {
                for (i, window) in target_lines.windows(seg.len()).enumerate() {
                    if window.iter().map(|s| s.as_ref()).eq(seg.iter().copied()) {
                        matches_for_seg.push((i, seg.len()));
                    }
                }
                if matches_for_seg.is_empty() {
                    let seg_trimmed: Vec<&str> = seg.iter().map(|s| s.trim_end()).collect();
                    for (i, window) in target_refs.windows(seg.len()).enumerate() {
                        if window.iter().copied().eq(seg_trimmed.iter().copied()) {
                            matches_for_seg.push((i, seg.len()));
                        }
                    }
                }
                if matches_for_seg.is_empty() && self.options.fuzz_factor > 0.0 {
                    let seg_loose: Vec<&str> = seg.iter().map(|s| s.trim()).collect();
                    for (i, window) in target_loose.windows(seg.len()).enumerate() {
                        if window.iter().copied().eq(seg_loose.iter().copied()) {
                            matches_for_seg.push((i, seg.len()));
                        }
                    }
                }
            }
            trace!(
                "  Segment {} ({} lines): found {} match candidate(s)",
                seg_i,
                seg.len(),
                matches_for_seg.len()
            );
            if matches_for_seg.is_empty() {
                warn!(
                    "  Segment {} failed to match anywhere in target file",
                    seg_i
                );
                return Err(HunkApplyError::ContextNotFound);
            }
            if matches_for_seg.len() > 30 {
                trace!(
                    "  Segment {} truncated from {} to 30 match candidates",
                    seg_i,
                    matches_for_seg.len()
                );
                matches_for_seg.truncate(30);
            }
            segment_matches.push(matches_for_seg);
        }

        let max_gap = 250.max(segments.iter().map(|s| s.len()).sum::<usize>() * 5);
        trace!(
            "  Wildcard maximum allowable gap between segments: {} lines",
            max_gap
        );
        let mut all_chains = Vec::new();
        let mut current_chain = Vec::with_capacity(segments.len());
        Self::collect_wildcard_chains(
            0,
            0,
            max_gap,
            &mut current_chain,
            &segment_matches,
            &mut all_chains,
        );
        if all_chains.is_empty() {
            warn!(
                "  No valid monotonic chains connecting all {} segments within gap bound ({})",
                segments.len(),
                max_gap
            );
            return Err(HunkApplyError::ContextNotFound);
        }

        debug!(
            "  Found {} valid segment chain(s). Sorting candidates...",
            all_chains.len()
        );
        all_chains.sort_by_key(|chain| {
            let start = chain.first().unwrap().0;
            let end = chain.last().unwrap().0 + chain.last().unwrap().1;
            let total_seg_len: usize = chain.iter().map(|c| c.1).sum();
            let gap = end.saturating_sub(start + total_seg_len);
            let line_dist = old_start_line.map(|l| (start + 1).abs_diff(l)).unwrap_or(0);
            (line_dist, gap)
        });

        for (c_idx, chain) in all_chains.iter().take(5).enumerate() {
            let start = chain.first().unwrap().0;
            let end = chain.last().unwrap().0 + chain.last().unwrap().1;
            trace!(
                "    Candidate chain {}: span [{}..{}] (total length={})",
                c_idx,
                start,
                end,
                end - start
            );
        }

        let mut candidates = Vec::new();
        for chain in all_chains {
            let start_index = chain.first().unwrap().0;
            let length = (chain.last().unwrap().0 + chain.last().unwrap().1) - start_index;
            if !candidates
                .iter()
                .any(|(loc, _): &(HunkLocation, MatchType)| {
                    loc.start_index == start_index && loc.length == length
                })
            {
                trace!(
                    "    find_wildcard_segments_location: adding candidate at line {} (start={}, length={})",
                    start_index + 1,
                    start_index,
                    length
                );
                candidates.push((
                    HunkLocation {
                        start_index,
                        length,
                    },
                    MatchType::ExactIgnoringWhitespace,
                ));
            }
            if candidates.len() >= 20 {
                trace!("    find_wildcard_segments_location: reached maximum 20 candidates limit");
                break;
            }
        }
        debug!(
            "find_wildcard_segments_location: returning {} candidate location(s)",
            candidates.len()
        );
        Ok(candidates)
    }

    /// Recursively collects valid combinations of segment matches that satisfy forward ordering and gap bounds.
    ///
    /// # Arguments
    ///
    /// * `seg_idx` - Current segment index being chained.
    /// * `min_start` - Minimum start index for the next segment match.
    /// * `max_gap` - Maximum allowed line gap between adjacent segments.
    /// * `current` - Accumulator of `(start_index, length)` matches for the current chain.
    /// * `segment_matches` - Matches found for each segment in the target file.
    /// * `all_chains` - Collection of all completed valid match chains.
    fn collect_wildcard_chains(
        seg_idx: usize,
        min_start: usize,
        max_gap: usize,
        current: &mut Vec<(usize, usize)>,
        segment_matches: &[Vec<(usize, usize)>],
        all_chains: &mut Vec<Vec<(usize, usize)>>,
    ) {
        if all_chains.len() >= 100 {
            trace!("  collect_wildcard_chains: hit maximum chain limit (100)");
            return;
        }
        if seg_idx == segment_matches.len() {
            trace!(
                "  collect_wildcard_chains: discovered complete chain: {:?}",
                current
            );
            all_chains.push(current.clone());
            return;
        }
        for cand in &segment_matches[seg_idx] {
            if cand.0 >= min_start && (seg_idx == 0 || cand.0.saturating_sub(min_start) <= max_gap)
            {
                current.push(*cand);
                Self::collect_wildcard_chains(
                    seg_idx + 1,
                    cand.0 + cand.1,
                    max_gap,
                    current,
                    segment_matches,
                    all_chains,
                );
                current.pop();
            }
        }
    }
}

impl<'a> HunkFinder for DefaultHunkFinder<'a> {
    /// Finds the location to apply a hunk to a slice of lines.
    ///
    /// This implementation uses a hierarchical approach: exact match, exact match
    /// ignoring trailing whitespace, and finally a flexible fuzzy match.
    ///
    /// # Arguments
    ///
    /// * `hunk` - The [`Hunk`] to locate.
    /// * `target_lines` - A slice of strings representing the content to search within.
    ///
    /// # Returns
    ///
    /// A tuple containing the [`HunkLocation`] and the [`MatchType`] on success.
    ///
    /// # Errors
    ///
    /// Returns `Err(`[`HunkApplyError`]`)` if no suitable location could be found.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::{parse_single_patch, DefaultHunkFinder, HunkFinder, ApplyOptions, HunkLocation, MatchType};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n line 1\n-line 2\n+line two\n```";
    /// let hunk = parse_single_patch(diff)?.hunks.remove(0);
    /// let target_lines = vec!["line 1", "line 2"];
    /// let options = ApplyOptions::new();
    /// let finder = DefaultHunkFinder::new(&options);
    /// let (location, match_type) = finder.find_location(&hunk, &target_lines)?;
    /// assert_eq!(location, HunkLocation { start_index: 0, length: 2 });
    /// assert!(matches!(match_type, MatchType::Exact));
    /// # Ok(())
    /// # }
    /// ```
    fn find_location<T: AsRef<str> + Sync>(
        &self,
        hunk: &Hunk,
        target_lines: &[T],
    ) -> Result<(HunkLocation, MatchType), HunkApplyError> {
        trace!("DefaultHunkFinder::find_location: locating hunk");
        let candidates = self.find_candidate_locations(hunk, target_lines)?;
        let best = candidates
            .into_iter()
            .next()
            .ok_or(HunkApplyError::ContextNotFound);
        if let Ok((ref loc, ref mtype)) = best {
            debug!(
                "DefaultHunkFinder::find_location: best location {:?} ({:?})",
                loc, mtype
            );
        }
        best
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
/// A tuple containing the [`HunkLocation`] and the [`MatchType`] on success.
///
/// # Errors
///
/// Returns `Err(`[`HunkApplyError`]`)` if no suitable location could be found, with a reason
/// for the failure (e.g., context not found, ambiguous match).
///
/// # Examples
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
    trace!(
        "find_hunk_location: target content {} bytes, hunk has {} lines",
        target_content.len(),
        hunk.lines.len()
    );
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
/// A tuple containing the [`HunkLocation`] and the [`MatchType`] on success.
///
/// # Errors
///
/// Returns `Err(`[`HunkApplyError`]`)` if no suitable location could be found.
///
/// # Examples
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
    trace!(
        "find_hunk_location_in_lines: delegating to DefaultHunkFinder (target has {} lines, hunk has {} lines)",
        target_lines.len(),
        hunk.lines.len()
    );
    let finder = DefaultHunkFinder::new(options);
    finder.find_location(hunk, target_lines)
}

/// Parses a hunk header line (e.g., `@@ -1,3 +1,3 @@`) to extract starting line numbers.
///
/// Extracts the 1-based old file start line and new file start line numbers from standard
/// unified diff headers.
///
/// # Arguments
///
/// * `line` - The raw hunk header string slice.
///
/// # Returns
///
/// A tuple `(old_start_line, new_start_line)` where each element is `Some(line_number)` if parsed,
/// or `None` if the line was malformed or omitted the number.
fn parse_hunk_header(line: &str) -> (Option<usize>, Option<usize>) {
    // We are interested in the original file's line number, which is the first number after '-'.
    // Example: @@ -21,8 +21,8 @@
    trace!("parse_hunk_header: parsing '{}'", line);
    let mut parts = line.split_whitespace();
    parts.next(); // skip "@@"
    let (Some(old_part), Some(new_part)) = (parts.next(), parts.next()) else {
        return (None, None);
    };
    let old_line = old_part
        .strip_prefix('-')
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse::<usize>().ok());
    let new_line = new_part
        .strip_prefix('+')
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse::<usize>().ok());
    trace!(
        "parse_hunk_header: extracted old_start={:?}, new_start={:?}",
        old_line,
        new_line
    );
    (old_line, new_line)
}

/// Formats an inline word-level diff between expected context lines and actual target lines.
///
/// Uses `similar`'s `inline` feature to highlight exact sub-line word additions and deletions.
/// Terminal color codes are applied via `colored` and will automatically be suppressed
/// in non-TTY or `NO_COLOR` environments.
///
/// # Arguments
///
/// * `expected_lines` - Slice of expected lines (e.g., from a hunk's match block).
/// * `actual_lines` - Slice of actual lines from the target file.
///
/// # Returns
///
/// A formatted string containing the annotated inline diff with word-level highlights.
///
/// # Examples
///
/// ```rust
/// use mpatch::format_inline_diff;
///
/// let expected = vec!["fn compute(x: i32) -> i32 {"];
/// let actual = vec!["fn compute(x: i64) -> i32 {"];
/// let diff = format_inline_diff(&expected, &actual);
///
/// assert!(diff.contains("compute"));
/// ```
pub fn format_inline_diff<T: AsRef<str>>(expected_lines: &[&str], actual_lines: &[T]) -> String {
    trace!(
        "format_inline_diff: comparing {} expected line(s) with {} actual line(s)",
        expected_lines.len(),
        actual_lines.len()
    );
    use colored::Colorize;
    let expected_str = expected_lines.join("\n");
    let actual_refs: Vec<&str> = actual_lines.iter().map(|s| s.as_ref()).collect();
    let actual_str = actual_refs.join("\n");
    let diff = TextDiff::from_lines(&expected_str, &actual_str);

    let mut out = String::new();
    let mut change_count = 0;
    for op in diff.ops() {
        for change in diff.iter_inline_changes(op) {
            change_count += 1;
            let sign = match change.tag() {
                similar::ChangeTag::Delete => "-".red(),
                similar::ChangeTag::Insert => "+".green(),
                similar::ChangeTag::Equal => " ".normal(),
            };
            let _ = write!(out, "{} ", sign);
            for (emphasized, token) in change.iter_strings_lossy() {
                if emphasized {
                    match change.tag() {
                        similar::ChangeTag::Delete => {
                            let _ = write!(out, "{}", token.red().bold().underline());
                        }
                        similar::ChangeTag::Insert => {
                            let _ = write!(out, "{}", token.green().bold().underline());
                        }
                        similar::ChangeTag::Equal => {
                            let _ = write!(out, "{}", token.normal());
                        }
                    }
                } else {
                    match change.tag() {
                        similar::ChangeTag::Delete => {
                            let _ = write!(out, "{}", token.red());
                        }
                        similar::ChangeTag::Insert => {
                            let _ = write!(out, "{}", token.green());
                        }
                        similar::ChangeTag::Equal => {
                            let _ = write!(out, "{}", token.normal());
                        }
                    }
                }
            }
            if change.missing_newline() {
                out.push('\n');
            }
        }
    }
    debug!(
        "format_inline_diff: formatted inline diff generated ({} bytes, {} lines, {} tokens)",
        out.len(),
        out.lines().count(),
        change_count
    );
    out
}

/// Performs a three-way line merge among a common ancestor (`base`), current content (`ours`),
/// and incoming changes (`theirs`).
///
/// Uses `similar`'s `TextMerge` engine. If conflicts exist, standard Diff3 conflict markers
/// (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`) are inserted into the output.
///
/// # Arguments
///
/// * `base` - The common ancestor content string.
/// * `ours` - The current local content string.
/// * `theirs` - The incoming changed content string.
/// * `labels` - Optional tuple `(base_label, ours_label, theirs_label)` for conflict marker headers.
///
/// # Returns
///
/// A tuple `(merged_text, is_conflicted)` where `is_conflicted` is `true` if any conflict occurred.
///
/// # Examples
///
/// ```rust
/// use mpatch::merge_three_way;
///
/// let base = "alpha\nbeta\ngamma\n";
/// let ours = "alpha\nbeta_local\ngamma\n";
/// let theirs = "alpha\nbeta\ngamma_remote\n";
///
/// let (merged, is_conflicted) = merge_three_way(base, ours, theirs, None);
/// assert!(!is_conflicted);
/// assert_eq!(merged, "alpha\nbeta_local\ngamma_remote\n");
///
/// // Conflicting 3-way merge with labels:
/// let (conflict_merged, is_conflicted) = merge_three_way(
///     "val = 1\n",
///     "val = 2\n",
///     "val = 3\n",
///     Some(("base", "ours", "theirs")),
/// );
/// assert!(is_conflicted);
/// assert!(conflict_merged.contains("<<<<<<< ours"));
/// assert!(conflict_merged.contains(">>>>>>> theirs"));
/// ```
pub fn merge_three_way(
    base: &str,
    ours: &str,
    theirs: &str,
    labels: Option<(&str, &str, &str)>,
) -> (String, bool) {
    debug!(
        "merge_three_way: starting 3-way merge (base: {} bytes / {} lines, ours: {} bytes / {} lines, theirs: {} bytes / {} lines, labels: {:?})",
        base.len(),
        base.lines().count(),
        ours.len(),
        ours.lines().count(),
        theirs.len(),
        theirs.lines().count(),
        labels
    );
    let mut merge = similar::TextMerge::from_lines(base, ours, theirs);
    let is_conflicted = merge.is_conflicted();
    let conflict_count = if is_conflicted {
        merge.conflict_count()
    } else {
        0
    };
    if is_conflicted {
        debug!(
            "merge_three_way: conflict detected ({} conflict(s)), formatting with Diff3 conflict style",
            conflict_count
        );
        merge.conflict_style(similar::ConflictStyle::Diff3);
        if let Some((base_lbl, ours_lbl, theirs_lbl)) = labels {
            trace!(
                "merge_three_way: applying custom labels ({}, {}, {})",
                base_lbl,
                ours_lbl,
                theirs_lbl
            );
            merge.labels(base_lbl, ours_lbl, theirs_lbl);
        }
    } else {
        debug!("merge_three_way: merged cleanly without conflicts");
    }
    let res = merge.to_string();
    info!(
        "merge_three_way: finished 3-way merge (conflicted={}, result length: {} bytes)",
        is_conflicted,
        res.len()
    );
    (res, is_conflicted)
}

/// Finds close matching file paths in a target directory when a patch specifies a missing file.
///
/// Uses `similar::get_close_matches` to suggest possible intended files among existing paths in `dir`.
/// Skips common non-source directories (such as `target`, `node_modules`, and hidden dot-directories).
///
/// # Arguments
///
/// * `missing_path` - The relative or absolute path of the missing target file.
/// * `base_dir` - The root directory to search within.
/// * `limit` - The maximum number of close match suggestions to return.
///
/// # Returns
///
/// A vector of [`PathBuf`] candidates relative to `base_dir`, sorted by similarity.
///
/// # Examples
///
/// ```rust
/// use mpatch::suggest_close_file_paths;
/// use std::path::Path;
/// use tempfile::tempdir;
/// use std::fs;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let dir = tempdir()?;
/// let file_path = dir.path().join("calculator.rs");
/// fs::write(&file_path, "fn add() {}\n")?;
///
/// // Search for a typo: "calculate.rs" instead of "calculator.rs"
/// let suggestions = suggest_close_file_paths(Path::new("calculate.rs"), dir.path(), 3);
/// assert_eq!(suggestions.len(), 1);
/// assert_eq!(suggestions[0], Path::new("calculator.rs"));
/// # Ok(())
/// # }
/// ```
pub fn suggest_close_file_paths(
    missing_path: &Path,
    base_dir: &Path,
    limit: usize,
) -> Vec<PathBuf> {
    debug!(
        "suggest_close_file_paths: searching for candidates close to '{}' in '{}'",
        missing_path.display(),
        base_dir.display()
    );
    let mut candidates = Vec::new();
    fn visit_dir(dir: &Path, base: &Path, candidates: &mut Vec<String>, depth: usize) {
        if depth > 8 || candidates.len() > 300 {
            trace!(
                "suggest_close_file_paths::visit_dir: halting recursion at depth {} (candidates: {})",
                depth,
                candidates.len()
            );
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(rel) = path.strip_prefix(base) {
                        trace!(
                            "  suggest_close_file_paths: found candidate file '{}'",
                            rel.display()
                        );
                        candidates.push(rel.to_string_lossy().into_owned());
                    }
                } else if path.is_dir() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if !name_str.starts_with('.')
                        && name_str != "target"
                        && name_str != "node_modules"
                    {
                        trace!(
                            "  suggest_close_file_paths: descending into dir '{}'",
                            path.display()
                        );
                        visit_dir(&path, base, candidates, depth + 1);
                    } else {
                        trace!(
                            "  suggest_close_file_paths: skipping directory '{}'",
                            path.display()
                        );
                    }
                }
            }
        } else {
            trace!(
                "suggest_close_file_paths::visit_dir: failed to read directory '{}'",
                dir.display()
            );
        }
    }
    visit_dir(base_dir, base_dir, &mut candidates, 0);
    trace!(
        "suggest_close_file_paths: scanned {} candidate file(s) in base directory",
        candidates.len()
    );

    let query_rel = if missing_path.is_relative() {
        missing_path
    } else {
        missing_path
            .strip_prefix(base_dir)
            .or_else(|_| {
                fs::canonicalize(base_dir)
                    .ok()
                    .and_then(|cb| missing_path.strip_prefix(&cb).ok().map(Path::new))
                    .ok_or(())
            })
            .unwrap_or_else(|_| {
                if let Some(name) = missing_path.file_name() {
                    Path::new(name)
                } else {
                    missing_path
                }
            })
    };

    let query_rel = query_rel.strip_prefix("./").unwrap_or(query_rel);
    let query = query_rel.to_string_lossy().replace('\\', "/");
    let query_str: &str = &query;
    let cand_refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();
    let matches = similar::get_close_matches(query_str, &cand_refs, limit, 0.6);

    info!(
        "suggest_close_file_paths: query '{}' yielded {} suggestion(s): {:?}",
        query_str,
        matches.len(),
        matches
    );
    matches.into_iter().map(PathBuf::from).collect()
}
