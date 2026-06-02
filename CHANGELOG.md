# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.6.3] - 2026-06-02

## [1.6.2] - 2026-06-02

## [1.6.1] - 2026-06-02

### Added
- **Python Bindings:** Introduced official Python bindings via PyO3. The `mpatch` package is now available on PyPI!
  - Exposes a rich, object-oriented, and fully typed API (with `.pyi` stubs) for tool builders and AI agent frameworks.
  - Provides high-level convenience functions (`patch_content`, `apply_directory`) and granular control over patches and hunks.
  - Includes pre-compiled wheels for Linux, macOS, and Windows, with support for Python 3.8+ and Python 3.13 free-threaded builds.
  - Implements Pythonic paradigms like slicing, unary inversion (`~patch`), and native exceptions (`ParseError`, `PathTraversalError`).

## [1.5.0] - 2026-05-28

### Added

-   **CLI:** Added `-c` / `--clipboard` flag to input directly from the system clipboard.

### Changed

-   **Logging:** Improved debug and trace logging. Reduced log spam during parsing, elevated key file I/O operations to `debug` level, and added detailed `trace` logs to the smart indentation algorithm (using `escape_debug` to visualize tabs vs. spaces). Also added visibility into `parse_auto` format detection and fallback behavior.
-   **Diagnostics:** Significantly enhanced the anonymization of debug reports (`-vvvv`). The report generator now actively redacts the input file path, target directory, current working directory, and user home directory from the entire report, including the full trace log, file contents, and error messages.
-   **Diagnostics:** The discrepancy check failure output in the debug report (`-vvvv`) now includes a unified diff between the original input patch and the regenerated patch to make differences easier to spot. The full patch contents are now placed inside a collapsible `<details>` block to reduce visual clutter.
-   **Diagnostics:** Improved the discrepancy check to normalize patches before comparison. It now ignores context lines, hunk headers, +/- interleaving, and self-replacements, significantly reducing noise and eliminating false positives caused by structural differences.

### Security

-   **Path Validation:** Fixed a vulnerability in `ensure_path_is_safe` where a dangling symlink could bypass the path traversal check and allow arbitrary file creation outside the target directory. The validation now correctly detects dangling symlinks using `symlink_metadata`.

### Fixed

-   **CLI:** Fixed a critical bug where `--dry-run` would incorrectly create parent directories on the filesystem. The path safety check now performs symlink resolution without requiring intermediate directory creation.
-   **CLI/API:** Fixed an issue where the preview diff generated during a dry run (`PatchResult::diff`) contained hardcoded `"a"` and `"b"` file headers instead of the actual target file paths.
-   **Patch Application:** Fixed a bug where the trailing newline status could be handled incorrectly if a patch contained out-of-order hunks that modified the end of the file before modifying earlier lines.

## [1.4.4] - 2026-05-02

### Security

-   **Path Validation:** Fixed a directory traversal vulnerability in `ensure_path_is_safe` where parent directories of a patch target were created on the filesystem *before* the path was validated. The function now performs a lexical path validation to ensure the path does not escape the base directory before interacting with the filesystem, preventing arbitrary directory creation.

### Fixed

-   **Patch Application:** Fixed a bug in the smart indentation adjustment where empty lines containing trailing whitespace were incorrectly used to calculate indentation drift. The logic now strictly requires both the patch line and the target line to contain non-whitespace characters before updating the indentation context.

## [1.4.3] - 2026-05-01

### Fixed

-   **Patch Application:** Fixed a critical bug in the fuzzy matching reconstruction logic where differing indentation levels between the patch and the target file could cause the internal alignment to scramble (often anchoring incorrectly on empty lines). The alignment is now based on trimmed lines, ensuring semantic correctness while still dynamically adjusting indentation during application.

## [1.4.2] - 2026-04-30

### Fixed

-   **Patch Application:** Fixed smart indentation adjustment to correctly translate multiple levels of indentation between spaces and tabs, making it robust against nested patches (e.g., inside markdown lists) by defaulting to 4 spaces per tab if the ratio is skewed. Also fixed an issue where empty lines (or lines with only whitespace) would result in trailing whitespace being added.
-   **Patch Application:** Fixed a bug where the auto-indentation logic would lose track of the target file's indentation style (e.g., tabs vs. spaces) when encountering unindented lines within the patch context.
-   **Patch Application:** Fixed a bug in the fallback logic for adjusting indentation of lines that are outdented relative to the hunk's context. The logic now correctly strips the indentation difference from the end of the line's actual indentation.
-   **CLI:** Fixed a bug in the debug report's discrepancy check (`-vvvv`) where patches that created or deleted files were skipped. The check now correctly treats non-existent files as empty strings, allowing verification to proceed.
-   **CLI:** Fixed a false positive in the debug report's discrepancy check (`-vvvv`) where patches with out-of-order hunks were incorrectly flagged as failures. The check now correctly verifies hunks regardless of their order.

## [1.4.1] - 2026-04-04

### Fixed

-   **Parser:** Fixed a bug where empty lines between file diffs (common in LLM outputs) were incorrectly absorbed as trailing context lines, breaking file creation and deletion detection.
-   **API:** Made `Patch::is_creation()` and `Patch::is_deletion()` more robust by checking hunk header line numbers (e.g., `@@ -0,0 ...`).
-   **Fuzzy Matching:** Fixed a bug in the search optimization heuristic where indentation differences between the patch and the target file would cause the fuzzy search to fail or look in the wrong location. The anchor line search now correctly ignores leading whitespace.

## [1.4.0] - 2026-03-20

### Performance

-   **Patch Creation:** Optimized `Patch::from_texts` to construct hunks directly from diff operations, avoiding the overhead of generating and re-parsing a unified diff string.
-   **Formatting:** Optimized `Hunk::fmt` to calculate line counts in a single pass.

### Fixed

-   **Patch Application:** Fixed a bug where a trailing newline was incorrectly added to files lacking one, even if the patch only modified lines at the beginning or middle of the file. The original newline status is now preserved unless the patch explicitly modifies the end of the file.
-   **Patch Application:** Fixed a bug where files resulting in exactly one newline (e.g., replacing all content with a single empty line) were incorrectly truncated to 0 bytes.
-   **Parser:** Fixed handling of the `\ No newline at end of file` marker. It now correctly verifies that the marker immediately follows a context or addition line.
-   **Parser:** Fixed parsing of empty hunks (e.g., `@@ -0,0 +0,0 @@`) which were previously ignored.
-   **Parser:** Fixed a bug where a context line in a diff that looks like a closing fence (e.g., ` ``` `) would incorrectly terminate the markdown code block. The parser now enforces that a closing fence must not be more indented than the opening fence.
-   **Parser:** Improved Conflict Marker detection to prevent false positives from Markdown H1 headers (lines of `====`). Detection now strictly requires a start marker (`<<<<`) followed by a middle (`====`) or end (`>>>>`) marker.
-   **CLI:** Fixed formatting in the debug report (`-vvvv`) where final summary logs were appended outside the markdown code block, breaking the report structure.

### Added

-   **CLI:** Added `-R` / `--reverse` flag to reverse patches before applying them (useful for undoing changes).
-   **API:** Added `mpatch::invert_patches` helper function to programmatically invert a list of patches.
-   **API:** Added `HunkApplier::set_original_newline_status` to manually control the expected newline behavior when using the line-based API.
-   **API:** Derived `PartialEq` for `ApplyOptions`.

### Changed

-   **Patch Application:** Files are now automatically deleted if the patch application results in empty content.
-   **Internal:** Refactored debug report finalization to be more robust and prevent redundant file locking.

## [1.3.5] - 2025-12-28

### Performance

-   **Search:** Optimized the exact and whitespace-insensitive search logic to avoid unnecessary heap allocations (`Box<dyn Iterator>`), improving efficiency for exact matches.
-   **Parser:** Optimized memory allocation in `parse_patches_from_lines` by pre-allocating buffers for hunk lines, reducing reallocations during parsing.

## [1.3.4] - 2025-12-11

### Fixed

-   **Fuzzy Matching:** Enhanced the fuzzy matching algorithm to be robust against indentation differences. It now calculates a "loose" similarity score based on trimmed lines, allowing patches with extra indentation (e.g., nested in Markdown lists) to correctly match flat code in the target file.
-   **Patch Application:** Implemented smart indentation adjustment. When applying a patch via fuzzy matching or whitespace-insensitive matching, the indentation of added lines is now dynamically adjusted to match the surrounding context of the target file, preventing "drift" or corruption of indentation styles.
-   **Parser:** Fixed a bug where indented code blocks inside a diff (e.g., within a list item in the diff content) were incorrectly interpreted as the end of the diff block. The parser now checks indentation to distinguish nested blocks from the closing fence.

## [1.3.3] - 2025-11-23

### Fixed

-   **Parser:** Fixed a bug where Git extended headers (e.g., `diff --git`, `index`, `new file mode`) appearing between file sections were incorrectly parsed as context lines for the preceding hunk. This prevents patch corruption when parsing raw Git output containing multiple files or metadata changes.

## [1.3.2] - 2025-11-22

### Fixed

-   **Fuzzy Matching:** Fixed a critical bug where applying a patch via fuzzy matching would overwrite local changes in the context lines (e.g., updated comments, different indentation). The application logic now performs a granular merge to preserve the target file's content while applying the patch's specific changes.
-   **Context Restoration:** Improved the heuristic for handling missing context lines. Missing lines at the end of a file are now restored (fixing truncated files), while missing lines in the middle of a block are treated as stale and skipped.

## [1.3.1] - 2025-11-21

### Performance

-   **Search:** Optimized the fuzzy matching algorithm by pre-calculating trimmed lines and using string references. This significantly reduces memory allocation and CPU usage when searching for hunk locations, especially in large files.

## [1.3.0] - 2025-11-21

### Added

-   **API:** Added `mpatch::parse_conflict_markers` to parse patches in the "Conflict Marker" format (`<<<<`, `====`, `>>>>`), commonly used in Git merge conflicts and AI suggestions.
-   **API:** Added `mpatch::PatchFormat` enum and `mpatch::detect_patch` function to programmatically identify if content is a Unified Diff, Markdown block, or Conflict Marker.
-   **API:** Added `mpatch::parse_auto` as a robust, unified entry point that automatically detects the format and parses the content accordingly.
-   **Parser:** `parse_diffs` now automatically detects and parses conflict marker blocks if standard unified diff parsing fails.
-   **API:** Added `mpatch::parse_single_patch` to simplify the common workflow of parsing a diff that is expected to contain exactly one patch. It returns a `Result<Patch, SingleParseError>`, handling the "zero or many" cases as an error.
-   **API:** Added `ApplyOptions::new()`, `ApplyOptions::dry_run()`, and `ApplyOptions::exact()` as convenience constructors to simplify common configuration setups.
-   **API:** Added a high-level `mpatch::patch_content_str` function for the common one-shot workflow of parsing a diff string and applying it to a content string. It handles parsing, validates that exactly one patch is present, and performs a strict application, returning the new content or a comprehensive error.
-   **API:** Added "strict" variants of the core apply functions: `try_apply_patch_to_file`, `try_apply_patch_to_content`, and `try_apply_patch_to_lines`. These functions return a `Result` and treat partial applications (where some hunks fail) as an `Err`, simplifying the common apply-or-fail workflow.
-   **API:** Implemented `std::fmt::Display` for `Patch` and `Hunk` to format them as a valid unified diff string. This provides a canonical representation useful for logging, debugging, and serialization.
-   **API:** Added fluent, chainable methods `ApplyOptions::with_dry_run(bool)` and `ApplyOptions::with_fuzz_factor(f32)` to simplify creating custom configurations.
-   **API:** Added convenience methods `ApplyResult::has_failures()`, `ApplyResult::failure_count()`, and `ApplyResult::success_count()` to simplify inspecting the outcome of a patch operation.
-   **Parser:** `parse_diffs` now scans **all** markdown code blocks for diffs, not just those explicitly tagged with `diff` or `patch`. This allows extracting patches from blocks labeled with other languages (e.g., ` ```rust `) often output by LLMs.
-   **Parser:** `parse_diffs` is now lenient. Blocks that look like diffs but are syntactically invalid (e.g., missing file headers) are silently ignored instead of returning a `ParseError`. This prevents the parser from choking on random code snippets that coincidentally resemble diff syntax.

### Changed

-   **API:** `patch_content_str` and `parse_single_patch` now use `parse_auto` internally. This means they now accept raw unified diff strings and conflict markers directly, in addition to the previously supported Markdown blocks.
-   **CLI:** The `mpatch` command now automatically detects the input format using `parse_auto`. This enables support for raw unified diffs and conflict markers as input files, alongside the existing Markdown support.
-   **Parser:** The Markdown parser now supports variable-length code fences (e.g., ` ```` `). A code block opened with `N` backticks requires a closing fence of at least `N` backticks. This enables support for files containing nested code blocks.
-   **Performance:** Optimized `Patch::from_texts` to use the raw diff parser directly, avoiding unnecessary Markdown wrapping and string allocation.

### Fixed

-   **Parser:** Fixed false positives where diffs inside nested code blocks (such as examples in documentation) were incorrectly identified as patches. The parser now checks that patch signatures appear at the top level of the code block.
-   **CLI:** Fixed a deadlock (freeze) that occurred when running with `-vvvv` (debug report mode). The report generator now correctly manages file locks to prevent recursive locking when internal functions log debug messages.

## [1.2.0] - 2025-11-17

### Added

-   **API:** Added `mpatch::parse_patches` to parse raw unified diff strings directly, without requiring them to be in a markdown code block.
-   **API:** Added `mpatch::parse_patches_from_lines` to parse raw unified diffs from an iterator of lines, offering more granular control and avoiding large string allocations.

-   **Diagnostics:** Significantly enhanced debug reports and error feedback.
    -   The debug report (`-vvvv`) now includes a **"Final Target File(s)"** section (showing the "after" state) and a **"Discrepancy Check"** section to programmatically validate patch integrity.
    -   When a hunk fails to apply, the CLI error output now includes the content of the failed hunk for easier debugging.

### Fixed

-   **Error Handling:** Improved error reporting for markdown diff blocks. Errors for missing file headers (`--- a/path`) now correctly report the line number where the ` ```diff` block begins, instead of an internal line number.
-   **Fuzzy Matching:** Fixed a critical bug where fuzzy matching would incorrectly overwrite the target file's context with the patch's (potentially stale) context. The new logic now correctly preserves the file's original context by applying only the specific additions and deletions from the hunk.

## [1.1.0] - 2025-11-11

### Added

-   **Parser:** The markdown parser now supports flexible code block headers. It will recognize a block as a diff if it contains `diff` or `patch` as a distinct word, allowing for headers like ` ```rust, diff` or ` ```diff rust`.

## [1.0.0] - 2025-11-05

This version marks the first major stable release of `mpatch`. It introduces a comprehensive overhaul of the library API to improve clarity, modularity, and programmatic control. Due to the extensive nature of these improvements, several breaking changes were necessary to create a more robust and extensible foundation for the future.

### Changed

-   **[BREAKING]** The core function signatures have been refactored for clarity and consistency. All patch application and location-finding functions now accept a unified `&ApplyOptions` struct.
    -   **Reason:** This consolidates configuration (like `dry_run` and `fuzz_factor`) into a single, extensible struct, simplifying the API.
    -   **Migration:**
        -   Rename `mpatch::apply_patch` to `mpatch::apply_patch_to_file`.
        -   Rename `mpatch::find_best_hunk_location` to `mpatch::find_hunk_location`.
        -   Pass configuration via the new `ApplyOptions` struct.

        **Before:**
        ```rust
        let result = mpatch::apply_patch(patch, dir.path(), true, 0.7)?;
        ```
        **After:**
        ```rust
        use mpatch::{apply_patch_to_file, ApplyOptions};

        let options = ApplyOptions { dry_run: true, fuzz_factor: 0.7 };
        let result = apply_patch_to_file(patch, dir.path(), options)?;
        ```

-   **[BREAKING]** Functions no longer return simple booleans or tuples for success status. They now return structured result types (`PatchResult`, `InMemoryResult`, `ApplyResult`) that provide detailed feedback for each hunk.
    -   **Reason:** A simple `bool` was insufficient for diagnosing partial failures. The new structs allow consumers to programmatically inspect the outcome of every hunk.
    -   **Migration:** Instead of checking a boolean, call the `.report.all_applied_cleanly()` method on the result. The dry-run diff is now accessed via `result.diff`.

        **Before:**
        ```rust
        let (new_content, success) = mpatch::apply_patch_to_content(patch, Some(content), 0.7);
        if !success {
            // handle failure
        }
        ```
        **After:**
        ```rust
        use mpatch::{apply_patch_to_content, ApplyOptions};

        let options = ApplyOptions::default();
        let result = apply_patch_to_content(patch, Some(content), &options);
        if !result.report.all_applied_cleanly() {
            // Optionally inspect failures:
            // for failure in result.report.failures() { ... }
        }
        let new_content = result.new_content;
        ```

-   **[BREAKING]** The `HunkApplyStatus::Applied` variant is no longer a unit variant. It now contains detailed information about how and where the hunk was applied.
    -   **Reason:** This provides consumers with rich, actionable feedback for successful operations, which is useful for logging and analysis.
    -   **Migration:** Update `match` statements to destructure the new struct variant.

        **Before:**
        ```rust
        match status {
            HunkApplyStatus::Applied => println!("Success!"),
            // ...
        }
        ```
        **After:**
        ```rust
        match status {
            HunkApplyStatus::Applied { location, match_type, .. } => {
                println!("Success at line {} via {:?}", location.start_index + 1, match_type);
            }
            // ...
        }
        ```

-   **Error Handling:** The `PatchError` enum is now more specific, with new `PermissionDenied` and `TargetIsDirectory` variants for more precise error handling.
-   **Diagnostics:** The `HunkApplyStatus::Applied` variant now includes the exact `replaced_lines`, and `HunkApplyError::FuzzyMatchBelowThreshold` includes the `location` of the best near-miss.

### Added

-   **Granular Control:** Introduced `apply_hunk_to_lines` for in-place, single-hunk modifications and the `HunkApplier` iterator for step-by-step patch application.
-   **Efficient In-Memory API:** Added `apply_patch_to_lines` and `find_hunk_location_in_lines` to operate directly on slices of strings, avoiding unnecessary allocations.
-   **Patch Creation & Inversion:** Added `Patch::from_texts` to programmatically create patches from string comparisons and `Patch::invert` to reverse them.
-   **Batch Operations:** A new high-level `apply_patches_to_dir` function simplifies applying multiple patches at once.
-   **Semantic Helpers:** Added methods like `Hunk::added_lines()`, `Hunk::removed_lines()`, `Patch::is_creation()`, and `Patch::is_deletion()` for more ergonomic code.
-   **Extensibility:** Introduced a public `HunkFinder` trait to decouple the search strategy from the core logic, allowing for custom search implementations.
-   **Optional Parallelism:** The `rayon` dependency is now optional via a `parallel` feature flag (enabled by default) for use in non-threaded environments.
-   **Security:** The path traversal check was extracted into a robust, public `ensure_path_is_safe` function.

### Fixed

-   **Panic Safety:** Replaced internal `unwrap()` calls with robust error handling to prevent potential panics.
-   **Documentation:** Corrected markdown rendering for code fences in Rustdoc comments.
