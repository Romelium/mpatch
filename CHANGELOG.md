# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

-   **API:** Added `ApplyOptions::new()`, `ApplyOptions::dry_run()`, and `ApplyOptions::exact()` as convenience constructors to simplify common configuration setups.
-   **API:** Added a high-level `mpatch::patch_content_str` function for the common one-shot workflow of parsing a diff string and applying it to a content string. It handles parsing, validates that exactly one patch is present, and performs a strict application, returning the new content or a comprehensive error.
-   **API:** Added "strict" variants of the core apply functions: `try_apply_patch_to_file`, `try_apply_patch_to_content`, and `try_apply_patch_to_lines`. These functions return a `Result` and treat partial applications (where some hunks fail) as an `Err`, simplifying the common apply-or-fail workflow.

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
