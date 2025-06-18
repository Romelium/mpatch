use log::{debug, info, trace, warn};
use similar::udiff::unified_diff;
use similar::TextDiff;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

// --- Error Types ---

#[derive(Error, Debug)]
pub enum PatchError {
    #[error("I/O error while processing {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Path '{0}' resolves outside the target directory. Aborting for security.")]
    PathTraversal(PathBuf),
    #[error("Target file not found for patching: {0}")]
    TargetNotFound(PathBuf),
    #[error("Hunk {hunk_index} could not be applied to '{file_path}': {reason}")]
    HunkApplyFailed {
        hunk_index: usize,
        file_path: String,
        reason: String,
    },
    #[error("Invalid diff format: {0}")]
    InvalidDiff(String),
    #[error("A diff block was found without a file path header (e.g., '--- a/path/to/file')")]
    MissingFileHeader,
}

// --- Data Structures ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// Raw lines including the prefix (' ', '+', '-').
    pub lines: Vec<String>,
}

impl Hunk {
    /// Lines to search for in the target file (context + deletions).
    pub fn get_match_block(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| !l.starts_with('+'))
            .map(|l| &l[1..])
            .collect()
    }

    /// Lines to replace the match block with (context + additions).
    pub fn get_replace_block(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter(|l| !l.starts_with('-'))
            .map(|l| &l[1..])
            .collect()
    }

    /// Checks if the hunk contains any additions or deletions.
    pub fn has_changes(&self) -> bool {
        self.lines.iter().any(|l| l.starts_with(['+', '-']))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    pub file_path: PathBuf,
    pub hunks: Vec<Hunk>,
    pub ends_with_newline: bool,
}

// --- Core Logic ---

/// Parses a string containing one or more ```diff blocks into a vector of Patches.
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
                        current_hunks.push(Hunk { lines: current_hunk_lines });
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
                    current_hunks.push(Hunk { lines: std::mem::take(&mut current_hunk_lines) });
                }
            } else if line.starts_with(['+', '-', ' ']) {
                current_hunk_lines.push(line.to_string());
            } else if line.starts_with('\\') {
                ends_with_newline_for_block = false;
            }
        }

        // Finalize the last hunk and patch for the block
        if !current_hunk_lines.is_empty() {
            current_hunks.push(Hunk { lines: current_hunk_lines });
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

/// Applies a single patch to the target directory.
/// Returns Ok(true) on success, Ok(false) if a hunk could not be applied cleanly.
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
        fs::create_dir_all(parent).map_err(|e| PatchError::Io { path: parent.to_path_buf(), source: e })?;
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
            debug!("  Skipping Hunk {}/{} (no changes).", hunk_index, patch.hunks.len());
            continue;
        }
        info!("  Applying Hunk {}/{}...", hunk_index, patch.hunks.len());

        let match_block = hunk.get_match_block();
        let replace_block = hunk.get_replace_block();

        match find_hunk_location(&match_block, &current_lines, fuzz_factor) {
            Some(start_index) => {
                current_lines.splice(start_index..start_index + match_block.len(), replace_block.into_iter().map(String::from));
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
        info!("  DRY RUN: Would write changes to '{}'", target_file_path.display());
        let diff = unified_diff(
            similar::Algorithm::default(),
            &original_content,
            &new_content,
            3,
            Some(("a", "b")),
        );
        println!("----- Proposed Changes for {} -----", patch.file_path.display());
        print!("{}", diff);
        println!("------------------------------------");
    } else {
        if let Some(parent) = target_file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| PatchError::Io { path: parent.to_path_buf(), source: e })?;
        }
        fs::write(&target_file_path, new_content).map_err(|e| PatchError::Io {
            path: target_file_path.clone(),
            source: e,
        })?;
        if all_hunks_applied_cleanly {
            info!("  Successfully wrote changes to '{}'", target_file_path.display());
        } else {
            warn!("  Wrote partial changes to '{}'", target_file_path.display());
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
            window.iter().map(|s| s.trim_end()).eq(match_stripped.iter().copied())
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

    debug!("    Exact matches failed. Attempting fuzzy match (threshold={:.2})...", fuzz_threshold);
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
        } else if (ratio - best_ratio).abs() < 1e-9 { // f64 equality
            potential_matches.push(i);
        }
    }

    if best_ratio >= f64::from(fuzz_threshold) {
        if potential_matches.len() == 1 {
            let best_index = potential_matches[0];
            debug!("    Found best fuzzy match at index {} (similarity: {:.3}).", best_index, best_ratio);
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
