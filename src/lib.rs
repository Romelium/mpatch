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
//! ## Core Features
//!
//! - **Markdown-Aware:** Directly parses unified diffs from within ` ```diff` or ` ```patch` code blocks.
//! - **Context-Driven:** Primarily finds patch locations by matching context lines.
//!   It intelligently uses the `@@ ... @@` line numbers as a hint to resolve
//!   ambiguity when the same context appears in multiple places.
//! - **Fuzzy Matching:** If an exact context match isn't found, `mpatch` uses a
//!   similarity algorithm to find the *best* fuzzy match.
//! - **Safe by Design:** Includes a dry-run mode and protection against path traversal attacks.
//!
//! ## Main Workflow
//!
//! The typical library usage involves two main steps:
//!
//! 1.  **Parsing:** Use [`parse_diffs`] to read a string (e.g., the content of a
//!     markdown file) and extract a `Vec<Patch>`. Each [`Patch`] represents the
//!     changes for a single file.
//! 2.  **Applying:** Iterate through the `Patch` objects and use [`apply_patch`]
//!     to apply each one to a target directory on the filesystem.
//!
//! ## Example
//!
//! Here's a complete example of how to use the library to patch a file in a
//! temporary directory.
//!
//! ````rust
//! use mpatch::{parse_diffs, apply_patch};
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
//! // For this example, we disable fuzzy matching (fuzz_factor = 0.0)
//! // and are not doing a dry run.
//! let success = apply_patch(patch, dir.path(), false, 0.0)?;
//!
//! // The patch should apply cleanly.
//! assert!(success);
//!
//! // 5. Verify the file was changed correctly.
//! let new_content = fs::read_to_string(&file_path)?;
//! assert_eq!(new_content, "fn main() {\n    println!(\"Hello, mpatch!\");\n}\n");
//! # Ok(())
//! # }
//! ````
use log::{debug, info, trace, warn};
use similar::udiff::unified_diff;
use similar::TextDiff;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

// --- Error Types ---

/// Represents the possible errors that can occur during patch operations.
#[derive(Error, Debug)]
pub enum PatchError {
    /// An I/O error occurred while reading or writing a file.
    /// This is a "hard" error that stops the entire process.
    #[error("I/O error while processing {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The patch attempted to access a path outside the target directory.
    /// This is a security measure to prevent malicious patches from modifying
    /// unintended files (e.g., `--- a/../../etc/passwd`).
    #[error("Path '{0}' resolves outside the target directory. Aborting for security.")]
    PathTraversal(PathBuf),
    /// The target file for a patch could not be found, and the patch did not
    /// appear to be for file creation (i.e., its first hunk was not an addition-only hunk).
    #[error("Target file not found for patching: {0}")]
    TargetNotFound(PathBuf),
    /// A ````diff ` block was found, but it was missing the `--- a/path/to/file`
    /// header required to identify the target file.
    #[error(
        "Diff block starting on line {line} was found without a file path header (e.g., '--- a/path/to/file')"
    )]
    MissingFileHeader {
        /// The line number where the diff block started.
        line: usize,
    },
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
    pub start_line: Option<usize>,
}

impl Hunk {
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
    ///     start_line: None,
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
    ///     start_line: None,
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
    ///     start_line: None,
    /// };
    /// assert!(hunk_with_changes.has_changes());
    ///
    /// let hunk_without_changes = Hunk {
    ///     lines: vec![ " a".to_string() ],
    ///     start_line: None,
    /// };
    /// assert!(!hunk_without_changes.has_changes());
    /// ```
    pub fn has_changes(&self) -> bool {
        self.lines.iter().any(|l| l.starts_with(['+', '-']))
    }
}

/// Represents all the changes to be applied to a single file.
///
/// A `Patch` is derived from a `--- a/path/to/file` section within a ````diff `
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

// --- Core Logic ---

/// Parses a string containing one or more ````diff ` blocks into a vector of [`Patch`] objects.
///
/// This function scans the input `content` for markdown-style diff blocks
/// (i.e., ```diff ... ```). It can handle multiple blocks in one string, and
/// multiple file patches within a single block.
///
/// # Arguments
///
/// * `content` - A string slice containing the text to parse.
///
/// # Errors
///
/// Returns `Err(PatchError::MissingFileHeader)` if a diff block contains patch
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
pub fn parse_diffs(content: &str) -> Result<Vec<Patch>, PatchError> {
    let mut all_patches = Vec::new();
    let mut lines = content.lines().enumerate().peekable();

    // The `find` call consumes the iterator until it finds the start of a diff block.
    // The loop continues searching for more blocks from where the last one ended.
    while let Some((line_index, _)) = lines
        .by_ref()
        .find(|(_, l)| l.starts_with("```diff") || l.starts_with("```patch"))
    {
        let diff_block_start_line = line_index + 1; // Convert 0-based index to 1-based line number

        // This temporary vec will hold all patch sections found in this block,
        // even if they are for the same file. They will be merged later.
        let mut unmerged_block_patches: Vec<Patch> = Vec::new();

        // State variables for the parser as it moves through the diff block.
        let mut current_file: Option<PathBuf> = None;
        let mut current_hunks: Vec<Hunk> = Vec::new();
        let mut current_hunk_lines: Vec<String> = Vec::new();
        let mut current_hunk_start_line: Option<usize> = None;
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
                            start_line: current_hunk_start_line,
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
                current_hunk_start_line = None;
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
                        start_line: current_hunk_start_line,
                    });
                }
                // Parse the line number hint for the new hunk.
                current_hunk_start_line = parse_hunk_header(line);
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
                start_line: current_hunk_start_line,
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
            return Err(PatchError::MissingFileHeader {
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

/// Applies a single [`Patch`] to the specified target directory.
///
/// This function attempts to apply all hunks from the `patch` to the corresponding
/// file inside `target_dir`. It handles file creation, modification, and deletion
/// (by emptying the file).
///
/// # Arguments
///
/// * `patch` - The [`Patch`] object to apply.
/// * `target_dir` - The base directory where the patch should be applied.
///   The `patch.file_path` will be joined to this directory.
/// * `dry_run` - If `true`, the function will not modify any files. Instead, it
///   will print a diff of the proposed changes to standard output.
/// * `fuzz_factor` - A float between `0.0` and `1.0` that sets the similarity
///   threshold for fuzzy matching.
///   - `1.0` requires a perfect match.
///   - `0.7` (the default for the CLI) allows for some differences.
///   - `0.0` disables fuzzy matching, only allowing exact matches (after
///     trimming trailing whitespace).
///
/// # Returns
///
/// - `Ok(true)` if all hunks in the patch were applied successfully.
/// - `Ok(false)` if one or more hunks could not be applied (e.g., context not
///   found). In this case, the file may be in a partially patched state.
/// - `Err(PatchError)` for "hard" errors like I/O problems, path traversal
///   violations, or a missing target file.
pub fn apply_patch(
    patch: &Patch,
    target_dir: &Path,
    dry_run: bool,
    fuzz_factor: f32,
) -> Result<bool, PatchError> {
    let target_file_path = target_dir.join(&patch.file_path);
    info!("Applying patch to: {}", patch.file_path.display());

    // --- Path Safety Check ---
    // This is a critical security measure to prevent a malicious patch from
    // writing files outside of the intended target directory.
    trace!(
        "  Checking path safety for relative path '{}'",
        patch.file_path.display()
    );
    // Canonicalize paths to resolve `..`, symlinks, etc., into absolute paths.
    let base_path = fs::canonicalize(target_dir).map_err(|e| PatchError::Io {
        path: target_dir.to_path_buf(),
        source: e,
    })?;
    let final_path = if target_file_path.exists() {
        fs::canonicalize(&target_file_path).map_err(|e| PatchError::Io {
            path: target_file_path.clone(),
            source: e,
        })?
    } else {
        // For new files, canonicalize the parent and append the filename.
        // This requires creating the parent directory first.
        let parent = target_file_path.parent().unwrap_or(Path::new(""));
        fs::create_dir_all(parent).map_err(|e| PatchError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
        fs::canonicalize(parent)
            .map_err(|e| PatchError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?
            .join(target_file_path.file_name().unwrap_or_default())
    };

    // Ensure the final, resolved path is still inside the base directory.
    if !final_path.starts_with(&base_path) {
        return Err(PatchError::PathTraversal(patch.file_path.clone()));
    }
    trace!("    Path is safe.");

    // --- Read Original File ---
    if target_file_path.is_dir() {
        return Err(PatchError::Io {
            path: target_file_path,
            source: std::io::Error::new(
                std::io::ErrorKind::IsADirectory,
                "target path is a directory, not a file",
            ),
        });
    }

    // Read the file content into a vector of lines for easier manipulation.
    let (original_content, mut current_lines) = if target_file_path.is_file() {
        trace!("  Reading target file '{}'", patch.file_path.display());
        let content = fs::read_to_string(&target_file_path).map_err(|e| PatchError::Io {
            path: target_file_path.clone(),
            source: e,
        })?;
        let lines = content.lines().map(String::from).collect();
        (content, lines)
    } else {
        // File doesn't exist. This is only okay if it's a file creation patch.
        // A creation patch has a first hunk with an empty match block.
        if patch
            .hunks
            .first()
            .is_none_or(|h| !h.get_match_block().is_empty())
        {
            return Err(PatchError::TargetNotFound(target_file_path));
        }
        info!("  Target file does not exist. Assuming file creation.");
        (String::new(), Vec::new())
    };
    trace!("  Read {} lines from target file.", current_lines.len());

    let mut all_hunks_applied_cleanly = true;

    // --- Apply Hunks ---
    // Iterate through each hunk and attempt to apply it to the `current_lines`.
    for (i, hunk) in patch.hunks.iter().enumerate() {
        let hunk_index = i + 1;
        // Optimization: skip hunks that contain no changes.
        if !hunk.has_changes() {
            debug!(
                "  Skipping Hunk {}/{} (no changes).",
                hunk_index,
                patch.hunks.len()
            );
            continue;
        }
        info!("  Applying Hunk {}/{}...", hunk_index, patch.hunks.len());
        trace!("    Hunk start line hint: {:?}", hunk.start_line);

        let match_block = hunk.get_match_block();
        let replace_block = hunk.get_replace_block();

        trace!("    --- Match Block ({} lines) ---", match_block.len());
        for line in &match_block {
            trace!("      |{}", line);
        }
        trace!("    --- Replace Block ({} lines) ---", replace_block.len());
        for line in &replace_block {
            trace!("      |{}", line);
        }
        trace!("    -----------------------------");

        // This is the core logic: find where the hunk should be applied.
        match find_hunk_location(&match_block, &current_lines, fuzz_factor, hunk.start_line) {
            Some((start_index, match_len)) => {
                trace!(
                    "    Found location: start_index={}, match_len={}",
                    start_index,
                    match_len
                );
                // Special case: end-of-file fuzzy match where the file is shorter than the context.
                // In this scenario, we treat it as a command to replace the entire file's content
                // with the hunk's replacement block.
                let is_short_file_eof_match = start_index == 0
                    && match_len == current_lines.len()
                    && !current_lines.is_empty() // Avoid for empty file creation
                    && current_lines.len() < match_block.len();

                if is_short_file_eof_match {
                    debug!("    Applying as full-file replacement due to end-of-file fuzzy match.");
                    current_lines.clear();
                    current_lines.extend(replace_block.into_iter().map(String::from));
                } else {
                    // The main operation: remove the matched lines and insert the replacement lines.
                    current_lines.splice(
                        start_index..start_index + match_len,
                        replace_block.into_iter().map(String::from),
                    );
                    debug!("    Hunk applied successfully at index {}.", start_index);
                }
            }
            None => {
                // If `find_hunk_location` returns `None`, the hunk could not be applied.
                let reason = "Context not found or ambiguous".to_string();
                warn!("  Failed to apply Hunk {}. {}", hunk_index, reason);
                trace!("    --- Expected Block (Context/Deletions) ---");
                for line in &match_block {
                    trace!("      '{}'", line);
                }
                trace!("    ----------------------------------------");
                all_hunks_applied_cleanly = false;
            }
        }
    }

    // --- Write Changes ---
    // Join the modified lines back into a single string.
    let mut new_content = current_lines.join("\n");
    if patch.ends_with_newline && !new_content.is_empty() {
        new_content.push('\n');
    }
    trace!("  Final content has {} characters.", new_content.len());

    if dry_run {
        // In dry-run mode, generate and print a diff instead of writing to the file.
        info!(
            "  DRY RUN: Would write changes to '{}'",
            patch.file_path.display()
        );
        let diff = unified_diff(
            similar::Algorithm::default(),
            &original_content,
            &new_content,
            3,
            Some(("a", "b")),
        );
        println!(
            "----- Proposed Changes for {} -----",
            patch.file_path.display()
        );
        print!("{}", diff);
        println!("------------------------------------");
    } else {
        // Write the modified content to the file system.
        if let Some(parent) = target_file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| PatchError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        fs::write(&target_file_path, new_content).map_err(|e| PatchError::Io {
            path: target_file_path.clone(),
            source: e,
        })?;
        if all_hunks_applied_cleanly {
            info!(
                "  Successfully wrote changes to '{}'",
                patch.file_path.display()
            );
        } else {
            warn!("  Wrote partial changes to '{}'", patch.file_path.display());
        }
    }

    Ok(all_hunks_applied_cleanly)
}

/// Finds optimized search ranges within the target file to perform the fuzzy search.
///
/// This is a performance heuristic. It tries to find an "anchor" line from the
/// hunk that is relatively uncommon in the target file. If successful, it returns
/// small search windows around the occurrences of that anchor. If no good anchor
/// is found, it returns a single range covering the entire file.
fn find_search_ranges(
    match_block: &[&str],
    target_lines: &[String],
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
                    .filter(|(_, l)| l.trim_end() == anchor_line)
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
                    let search_radius = (hunk_size * SEARCH_RADIUS_FACTOR).max(MIN_SEARCH_RADIUS);

                    for &occurrence_idx in &occurrences {
                        // Estimate where the hunk would start based on the anchor's position.
                        let estimated_start = occurrence_idx.saturating_sub(line_idx);
                        let start = estimated_start.saturating_sub(search_radius);
                        let end =
                            (estimated_start + hunk_size + search_radius).min(target_lines.len());
                        ranges.push((start, end));
                    }
                    // Merge any overlapping ranges created by nearby occurrences.
                    return merge_ranges(ranges);
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
fn find_hunk_location(
    match_block: &[&str],
    target_lines: &[String],
    fuzz_threshold: f32,
    start_line: Option<usize>,
) -> Option<(usize, usize)> {
    trace!(
        "  find_hunk_location called for a hunk with {} lines to match against {} target lines.",
        match_block.len(),
        target_lines.len()
    );

    if match_block.is_empty() {
        // An empty match block (file creation) can only be applied to an empty file.
        trace!("    Match block is empty (file creation).");
        return if target_lines.is_empty() {
            trace!("    Target is empty, match successful at (0, 0).");
            Some((0, 0))
        } else {
            trace!("    Target is not empty, match failed.");
            None
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
                        .filter(|(_, window)| *window == match_block)
                        .map(|(i, _)| i),
                )
            } else {
                Box::new(std::iter::empty())
            };

        if let Some(index) = tie_break_with_line_number(exact_matches, start_line, "exact") {
            debug!("    Found unique exact match at index {}.", index);
            return Some((index, match_block.len()));
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
                                .map(|s| s.trim_end())
                                .eq(match_stripped.iter().copied())
                        })
                        .map(|(i, _)| i),
                )
            } else {
                Box::new(std::iter::empty())
            };

        if let Some(index) =
            tie_break_with_line_number(stripped_matches, start_line, "exact (ignoring whitespace)")
        {
            debug!(
                "    Found unique whitespace-insensitive match at index {}.",
                index
            );
            return Some((index, match_block.len()));
        }
    }

    // --- STRATEGY 3: Fuzzy Match (with flexible window) ---
    // This is the core "smart" logic. If an exact match fails, we search for
    // the best-fitting slice in the target file, allowing the slice to be
    // slightly larger or smaller than the patch's context. This handles cases
    // where lines have been added or removed near the patch location.
    if fuzz_threshold > 0.0 && !match_block.is_empty() {
        trace!(
            "    Exact matches failed. Attempting flexible fuzzy match (threshold={:.2})...",
            fuzz_threshold
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
        let search_ranges = find_search_ranges(match_block, target_lines, len);
        trace!("    Using search ranges: {:?}", search_ranges);

        for (range_start, range_end) in search_ranges {
            let target_slice = &target_lines[range_start..range_end];

            // Iterate through all possible window sizes.
            for window_len in min_len..=max_len {
                if window_len > target_slice.len() {
                    continue;
                }

                // Iterate through all windows of the current size.
                for (i, window) in target_slice.windows(window_len).enumerate() {
                    let absolute_index = range_start + i;
                    // HYBRID SCORING: Combine line-based and word-based similarity.
                    // This correctly handles both structural changes (added/removed lines)
                    // and content changes (modified text within lines).
                    let window_stripped_lines: Vec<_> =
                        window.iter().map(|s| s.trim_end()).collect();

                    // 1. Line-based score for structural similarity
                    let diff_lines = similar::TextDiff::from_slices(
                        &window_stripped_lines,
                        &match_stripped_lines,
                    );
                    let ratio_lines = diff_lines.ratio() as f64;

                    // 2. Word-based score for content similarity
                    let window_content = window_stripped_lines.join("\n");
                    let diff_words = similar::TextDiff::from_words(&window_content, &match_content);
                    let ratio_words = diff_words.ratio() as f64;

                    // 3. Combine them (weighting structure more heavily)
                    let ratio = 0.6 * ratio_lines + 0.4 * ratio_words;

                    // Penalize matches that have a different size from the match_block.
                    // This helps prefer matches of the correct size even if a smaller
                    // sub-window has a slightly higher raw ratio.
                    let size_diff = window_len.abs_diff(len) as f64;
                    let penalty = (size_diff / len.max(window_len) as f64) * 0.2;
                    let score = ratio - penalty;

                    // Check if this is the new best score.
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
            }
        }

        trace!(
            "    Fuzzy search complete. Best score: {:.3}, best ratio: {:.3}, potential matches: {:?}",
            best_score,
            best_ratio_at_best_score,
            potential_matches
        );

        // Check if the best match found meets the user-defined threshold.
        if best_ratio_at_best_score >= f64::from(fuzz_threshold) {
            if potential_matches.len() == 1 {
                let (start, len) = potential_matches[0];
                debug!(
                    "    Found best fuzzy match at index {} (length {}, similarity: {:.3}).",
                    start, len, best_ratio_at_best_score
                );
                return Some((start, len));
            }
            // AMBIGUOUS FUZZY MATCH - TRY TO TIE-BREAK
            if let Some(line) = start_line {
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
                        return Some((start, len));
                    }
                } else {
                    trace!("    Tie-breaking failed: multiple fuzzy matches are equidistant from the line number hint.");
                }
            }
            warn!("    Ambiguous fuzzy match: Multiple locations found with same top score ({:.3}): {:?}. Skipping.", best_ratio_at_best_score, potential_matches);
            debug!("    Failed to apply hunk: Ambiguous fuzzy match could not be resolved.");
            return None;
        } else if best_ratio_at_best_score >= 0.0 {
            // Did not meet threshold
            let (start, len) = potential_matches[0];
            trace!(
                "    Fuzzy match: Best location (index {}, len {}, similarity {:.3}) did not meet threshold of {:.2}.",
                start, len, best_ratio_at_best_score, fuzz_threshold
            );
        } else {
            // No potential matches found at all
            debug!("    Fuzzy match: Could not find any potential match location.");
        }
    } else if fuzz_threshold <= 0.0 {
        trace!("    Failed exact matches. Fuzzy matching disabled.");
    }

    // --- STRATEGY 4: End-of-file fuzzy match for short files ---
    // This handles cases where the entire file is a good fuzzy match for the
    // start of the hunk context, which can happen if the file is missing
    // context lines that the patch expects to be there at the end.
    if !target_lines.is_empty() && target_lines.len() < match_block.len() && fuzz_threshold > 0.0 {
        trace!("    Target file is shorter than hunk. Attempting end-of-file fuzzy match...");
        let target_stripped: Vec<_> = target_lines.iter().map(|s| s.trim_end()).collect();
        let match_stripped: Vec<_> = match_block.iter().map(|s| s.trim_end()).collect();
        let diff = TextDiff::from_slices(&target_stripped, &match_stripped);
        let ratio = diff.ratio();

        // Be slightly more lenient for this specific end-of-file prefix case.
        let effective_threshold = (f64::from(fuzz_threshold) - 0.1).max(0.5);
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
            return Some((0, target_lines.len()));
        } else {
            trace!(
                "    End-of-file fuzzy match ratio {:.3} did not meet effective threshold {:.3}.",
                ratio,
                effective_threshold
            );
        }
    }

    debug!("    Failed to find any suitable match location for hunk.");
    None
}

/// Given an iterator of match indices, attempts to find the best one using the
/// hunk's original line number as a hint. Returns the index of the best match,
/// or `None` if the ambiguity cannot be resolved.
/// This function avoids collecting matches into a vector if there are 0 or 1 matches.
fn tie_break_with_line_number(
    mut matches: impl Iterator<Item = usize>,
    start_line: Option<usize>,
    match_type: &str,
) -> Option<usize> {
    // --- Step 1: Check for 0 or 1 matches without allocation ---
    let first_match = match matches.next() {
        Some(m) => m,
        None => {
            trace!("      No {} matches found.", match_type);
            return None;
        }
    };

    let second_match = matches.next();
    if second_match.is_none() {
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
        return Some(first_match);
    }

    // --- Step 2: Multiple matches found, collect and tie-break ---
    // At least two matches exist. Collect them all for analysis.
    let mut all_matches = vec![first_match];
    all_matches.push(second_match.unwrap());
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
            return Some(closest_index);
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
    warn!(
        "    Ambiguous {} match: Hunk context found at multiple locations: {:?}. Skipping.",
        match_type, all_matches
    );
    None
}

/// Parses a hunk header line (e.g., "@@ -1,3 +1,3 @@") to extract the starting line number.
fn parse_hunk_header(line: &str) -> Option<usize> {
    // We are interested in the original file's line number, which is the first number after '-'.
    // Example: @@ -21,8 +21,8 @@
    line.split(' ')
        .nth(1) // Get "-21,8"
        .and_then(|s| s.strip_prefix('-')) // Get "21,8"
        .and_then(|s| s.split(',').next()) // Get "21"
        .and_then(|s| s.parse::<usize>().ok()) // Get 21
}
