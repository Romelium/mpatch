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
//! - **Markdown-Aware:** Directly parses unified diffs from within ````diff ` code blocks.
//! - **Context-Driven:** Ignores `@@ ... @@` line numbers, finding patch locations by
//!   matching context lines.
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
    #[error("I/O error while processing {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The patch attempted to access a path outside the target directory.
    /// This is a security measure to prevent malicious patches from modifying
    /// unintended files.
    #[error("Path '{0}' resolves outside the target directory. Aborting for security.")]
    PathTraversal(PathBuf),
    /// The target file for a patch could not be found, and the patch was not
    /// a file creation patch.
    #[error("Target file not found for patching: {0}")]
    TargetNotFound(PathBuf),
    /// A ````diff ` block was found, but it was missing the `--- a/path/to/file`
    /// header required to identify the target file.
    #[error("A diff block was found without a file path header (e.g., '--- a/path/to/file')")]
    MissingFileHeader,
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
}

impl Hunk {
    /// Extracts the lines that need to be matched in the target file.
    ///
    /// This includes context lines (starting with ' ') and deletion lines
    /// (starting with '-'). The leading character is stripped.
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
    /// (starting with '+'). The leading character is stripped.
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
    /// A hunk with only context lines has no changes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mpatch::Hunk;
    /// let hunk_with_changes = Hunk {
    ///     lines: vec![ "+ a".to_string() ],
    /// };
    /// assert!(hunk_with_changes.has_changes());
    ///
    /// let hunk_without_changes = Hunk {
    ///     lines: vec![ " a".to_string() ],
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
    let mut lines = content.lines().peekable();

    while let Some(_) = lines.by_ref().find(|l| l.trim().starts_with("```diff")) {
        let mut current_file: Option<PathBuf> = None;
        let mut current_hunks: Vec<Hunk> = Vec::new();
        let mut current_hunk_lines: Vec<String> = Vec::new();
        let mut ends_with_newline_for_block = true;

        // Consume lines within the ```diff block
        while let Some(line) = lines.next() {
            if line.trim() == "```" {
                break; // End of block
            }

            if let Some(path_str) = line.strip_prefix("--- a/") {
                let new_file = PathBuf::from(path_str.trim());
                if let Some(existing_file) = &current_file {
                    // This is a new file path within the same ```diff block.
                    // Finalize the previous file's patch.
                    if !current_hunk_lines.is_empty() {
                        current_hunks.push(Hunk {
                            lines: current_hunk_lines,
                        });
                    }
                    if !current_hunks.is_empty() {
                        all_patches.push(Patch {
                            file_path: existing_file.clone(),
                            hunks: current_hunks,
                            ends_with_newline: ends_with_newline_for_block,
                        });
                    }
                    // Reset for the new file
                    current_hunks = Vec::new();
                    current_hunk_lines = Vec::new();
                    ends_with_newline_for_block = true;
                }
                current_file = Some(new_file);
            } else if line.starts_with("+++") {
                // Ignore +++ lines, they are part of the header but don't contain patch data.
            } else if line.starts_with("@@") {
                if !current_hunk_lines.is_empty() {
                    current_hunks.push(Hunk {
                        lines: std::mem::take(&mut current_hunk_lines),
                    });
                }
            } else if line.starts_with(['+', '-', ' ']) {
                current_hunk_lines.push(line.to_string());
            } else if line.starts_with('\\') {
                ends_with_newline_for_block = false;
            }
        }

        // Finalize the last hunk and patch for the block
        if !current_hunk_lines.is_empty() {
            current_hunks.push(Hunk {
                lines: current_hunk_lines,
            });
        }

        if let Some(file_path) = current_file {
            if !current_hunks.is_empty() {
                all_patches.push(Patch {
                    file_path,
                    hunks: current_hunks,
                    ends_with_newline: ends_with_newline_for_block,
                });
            }
        } else if !current_hunks.is_empty() {
            return Err(PatchError::MissingFileHeader);
        }
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
        // For new files, canonicalize the parent and append the filename
        let parent = target_file_path.parent().unwrap_or(Path::new(""));
        fs::create_dir_all(parent)
            .map_err(|e| PatchError::Io { path: parent.to_path_buf(), source: e })?;
        fs::canonicalize(parent)
            .map_err(|e| PatchError::Io { path: parent.to_path_buf(), source: e })?
            .join(target_file_path.file_name().unwrap_or_default())
    };

    if !final_path.starts_with(&base_path) {
        return Err(PatchError::PathTraversal(patch.file_path.clone()));
    }

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

    let (original_content, mut current_lines) = if target_file_path.is_file() {
        let content = fs::read_to_string(&target_file_path).map_err(|e| PatchError::Io {
            path: target_file_path.clone(),
            source: e,
        })?;
        let lines = content.lines().map(String::from).collect();
        (content, lines)
    } else {
        // File doesn't exist. This is only okay if it's a file creation patch.
        if patch.hunks.get(0).map_or(true, |h| !h.get_match_block().is_empty()) {
            return Err(PatchError::TargetNotFound(target_file_path));
        }
        info!("  Target file does not exist. Assuming file creation.");
        (String::new(), Vec::new())
    };

    let mut all_hunks_applied_cleanly = true;

    // --- Apply Hunks ---
    for (i, hunk) in patch.hunks.iter().enumerate() {
        let hunk_index = i + 1;
        if !hunk.has_changes() {
            debug!(
                "  Skipping Hunk {}/{} (no changes).",
                hunk_index,
                patch.hunks.len()
            );
            continue;
        }
        info!(
            "  Applying Hunk {}/{}...",
            hunk_index,
            patch.hunks.len()
        );

        let match_block = hunk.get_match_block();
        let replace_block = hunk.get_replace_block();

        match find_hunk_location(&match_block, &current_lines, fuzz_factor) {
            Some(start_index) => {
                current_lines.splice(
                    start_index..start_index + match_block.len(),
                    replace_block.into_iter().map(String::from),
                );
                debug!("    Hunk applied successfully at index {}.", start_index);
            }
            None => {
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
    let mut new_content = current_lines.join("\n");
    if patch.ends_with_newline && !new_content.is_empty() {
        new_content.push('\n');
    }

    if dry_run {
        info!(
            "  DRY RUN: Would write changes to '{}'",
            target_file_path.display()
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
        if let Some(parent) = target_file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| PatchError::Io { path: parent.to_path_buf(), source: e })?;
        }
        fs::write(&target_file_path, new_content).map_err(|e| PatchError::Io {
            path: target_file_path.clone(),
            source: e,
        })?;
        if all_hunks_applied_cleanly {
            info!(
                "  Successfully wrote changes to '{}'",
                target_file_path.display()
            );
        } else {
            warn!(
                "  Wrote partial changes to '{}'",
                target_file_path.display()
            );
        }
    }

    Ok(all_hunks_applied_cleanly)
}

/// Finds the starting index of the hunk's match block in the target lines.
fn find_hunk_location(
    match_block: &[&str],
    target_lines: &[String],
    fuzz_threshold: f32,
) -> Option<usize> {
    if match_block.is_empty() {
        // An empty match block (file creation) can only be applied to an empty file.
        return if target_lines.is_empty() { Some(0) } else { None };
    }
    if match_block.len() > target_lines.len() {
        return None;
    }

    // 1. Exact Match
    let exact_matches: Vec<_> = target_lines
        .windows(match_block.len())
        .enumerate()
        .filter(|(_, window)| *window == match_block)
        .map(|(i, _)| i)
        .collect();

    if exact_matches.len() == 1 {
        trace!("    Found exact match.");
        return Some(exact_matches[0]);
    }
    if exact_matches.len() > 1 {
        warn!("    Ambiguous exact match: Hunk context found at multiple locations: {:?}. Skipping.", exact_matches);
        return None;
    }

    // 2. Exact Match (Ignoring Trailing Whitespace)
    let match_stripped: Vec<_> = match_block.iter().map(|s| s.trim_end()).collect();
    let stripped_matches: Vec<_> = target_lines
        .windows(match_block.len())
        .enumerate()
        .filter(|(_, window)| {
            window
                .iter()
                .map(|s| s.trim_end())
                .eq(match_stripped.iter().copied())
        })
        .map(|(i, _)| i)
        .collect();

    if stripped_matches.len() == 1 {
        trace!("    Found exact match (ignoring trailing whitespace).");
        return Some(stripped_matches[0]);
    }
    if stripped_matches.len() > 1 {
        warn!("    Ambiguous exact match (ignoring trailing whitespace): Hunk context found at multiple locations: {:?}. Skipping.", stripped_matches);
        return None;
    }

    // 3. Fuzzy Match
    if fuzz_threshold <= 0.0 {
        debug!("    Failed exact matches. Fuzzy matching disabled.");
        return None;
    }

    debug!(
        "    Exact matches failed. Attempting fuzzy match (threshold={:.2})...",
        fuzz_threshold
    );
    let mut best_ratio = -1.0;
    let mut potential_matches = Vec::new();

    for (i, window) in target_lines.windows(match_block.len()).enumerate() {
        // Use character-level diff for fuzzy matching of line blocks
        let window_content = window.join("\n");
        let match_content = match_block.join("\n");
        let diff = TextDiff::from_chars(&window_content, &match_content);
        let ratio = diff.ratio() as f64;

        if ratio > best_ratio {
            best_ratio = ratio;
            potential_matches.clear();
            potential_matches.push(i);
        } else if (ratio - best_ratio).abs() < 1e-9 {
            // f64 equality
            potential_matches.push(i);
        }
    }

    if best_ratio >= f64::from(fuzz_threshold) {
        if potential_matches.len() == 1 {
            let best_index = potential_matches[0];
            debug!(
                "    Found best fuzzy match at index {} (similarity: {:.3}).",
                best_index, best_ratio
            );
            return Some(best_index);
        } else {
            warn!("    Ambiguous fuzzy match: Hunk context found at multiple locations with same top similarity ({:.3}): {:?}. Skipping.", best_ratio, potential_matches);
            return None;
        }
    } else {
        if !potential_matches.is_empty() {
            debug!("    Fuzzy match: Best location (index {}, similarity {:.3}) did not meet a threshold of {:.2}.", potential_matches[0], best_ratio, fuzz_threshold);
        } else {
            debug!("    Fuzzy match: Could not find any potential match location.");
        }
        return None;
    }
}
