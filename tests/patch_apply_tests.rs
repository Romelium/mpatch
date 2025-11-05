use indoc::indoc;
use mpatch::{
    apply_hunk_to_lines, apply_patch_to_file, apply_patch_to_lines, apply_patches_to_dir,
    find_hunk_location, find_hunk_location_in_lines, parse_diffs, ApplyOptions, DefaultHunkFinder,
    HunkApplyError, HunkApplyStatus, HunkFinder, HunkLocation, MatchType, ParseError, Patch,
    PatchError,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_parse_simple_diff() {
    let diff = indoc! {"
        Some text before.
        ```diff
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1,5 +1,5 @@
         fn main() {
        -    println!(\"Hello, world!\");
        +    println!(\"Hello, mpatch!\");
         }
        ```
        Some text after.
    "};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "src/main.rs");
    assert_eq!(patch.hunks.len(), 1);
    assert!(patch.ends_with_newline);
    let hunk = &patch.hunks[0];
    assert_eq!(hunk.lines.len(), 4);
    assert_eq!(
        hunk.get_match_block(),
        vec!["fn main() {", "    println!(\"Hello, world!\");", "}"]
    );
    assert_eq!(
        hunk.get_replace_block(),
        vec!["fn main() {", "    println!(\"Hello, mpatch!\");", "}"]
    );
}

#[test]
fn test_parse_patch_block_header() {
    let diff = indoc! {"
        Some text before.
        ```patch
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1,5 +1,5 @@
         fn main() {
        -    println!(\"Hello, world!\");
        +    println!(\"Hello, mpatch!\");
         }
        ```
        Some text after.
    "};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "src/main.rs");
    assert_eq!(patch.hunks.len(), 1);
    assert!(patch.ends_with_newline);
    let hunk = &patch.hunks[0];
    assert_eq!(hunk.lines.len(), 4);
    assert_eq!(
        hunk.get_match_block(),
        vec!["fn main() {", "    println!(\"Hello, world!\");", "}"]
    );
    assert_eq!(
        hunk.get_replace_block(),
        vec!["fn main() {", "    println!(\"Hello, mpatch!\");", "}"]
    );
}

#[test]
fn test_parse_multiple_diff_blocks() {
    let diff = indoc! {r#"
        First change:
        ```diff
        --- a/file1.txt
        +++ b/file1.txt
        @@ -1 +1 @@
        -foo
        +bar
        ```

        Second change:
        ```diff
        --- a/file2.txt
        +++ b/file2.txt
        @@ -1 +1 @@
        -baz
        +qux
        \ No newline at end of file
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 2);

    assert_eq!(patches[0].file_path.to_str().unwrap(), "file1.txt");
    assert_eq!(patches[0].hunks.len(), 1);
    assert_eq!(patches[0].hunks[0].get_replace_block(), vec!["bar"]);
    assert!(patches[0].ends_with_newline);

    assert_eq!(patches[1].file_path.to_str().unwrap(), "file2.txt");
    assert_eq!(patches[1].hunks.len(), 1);
    assert_eq!(patches[1].hunks[0].get_replace_block(), vec!["qux"]);
    assert!(!patches[1].ends_with_newline);
}

#[test]
fn test_parse_multiple_files_in_one_block() {
    let diff = indoc! {r#"
        ```diff
        --- a/file1.txt
        +++ b/file1.txt
        @@ -1 +1 @@
        -foo
        +bar
        --- a/file2.txt
        +++ b/file2.txt
        @@ -1 +1 @@
        -baz
        +qux
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 2);

    assert_eq!(patches[0].file_path.to_str().unwrap(), "file1.txt");
    assert_eq!(patches[0].hunks.len(), 1);
    assert_eq!(patches[0].hunks[0].get_replace_block(), vec!["bar"]);

    assert_eq!(patches[1].file_path.to_str().unwrap(), "file2.txt");
    assert_eq!(patches[1].hunks.len(), 1);
    assert_eq!(patches[1].hunks[0].get_replace_block(), vec!["qux"]);
}

#[test]
fn test_parse_multiple_sections_for_same_file_in_one_block() {
    let diff = indoc! {r#"
        ```diff
        --- a/same_file.txt
        +++ b/same_file.txt
        @@ -1 +1 @@
        -hunk1
        +hunk one
        --- a/same_file.txt
        +++ b/same_file.txt
        @@ -10 +10 @@
        -hunk2
        +hunk two
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    // This is the key assertion: it should be parsed as ONE patch for the file,
    // not two separate patches.
    assert_eq!(
        patches.len(),
        1,
        "Should produce a single patch for the same file"
    );

    assert_eq!(patches[0].file_path.to_str().unwrap(), "same_file.txt");
    assert_eq!(patches[0].hunks.len(), 2, "Should contain two hunks");
    assert_eq!(patches[0].hunks[0].get_replace_block(), vec!["hunk one"]);
    assert_eq!(patches[0].hunks[1].get_replace_block(), vec!["hunk two"]);
}

#[test]
fn test_parse_file_creation_with_dev_null() {
    let diff = indoc! {r#"
        ```diff
        --- /dev/null
        +++ b/new_from_null.txt
        @@ -0,0 +1,2 @@
        +hello
        +world
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "new_from_null.txt");
    assert_eq!(patch.hunks.len(), 1);
    assert_eq!(patch.hunks[0].old_start_line, Some(0));
    assert_eq!(patch.hunks[0].get_replace_block(), vec!["hello", "world"]);
    assert!(patch.ends_with_newline);
}

#[test]
fn test_parse_file_creation_with_a_dev_null() {
    let diff = indoc! {r#"
        ```diff
        --- a/dev/null
        +++ b/another_new.txt
        @@ -0,0 +1 @@
        +content
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "another_new.txt");
    assert_eq!(patch.hunks.len(), 1);
    assert_eq!(patch.hunks[0].old_start_line, Some(0));
    assert_eq!(patch.hunks[0].get_replace_block(), vec!["content"]);
}

#[test]
fn test_parse_diff_without_ab_prefix() {
    let diff = indoc! {r#"
        ```diff
        --- path/to/file.txt
        +++ path/to/file.txt
        @@ -1 +1 @@
        -old
        +new
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "path/to/file.txt");
    assert_eq!(patch.hunks.len(), 1);
    assert_eq!(patch.hunks[0].old_start_line, Some(1));
    assert_eq!(patch.hunks[0].get_replace_block(), vec!["new"]);
}

#[test]
fn test_parse_file_creation_without_b_prefix() {
    let diff = indoc! {r#"
        ```diff
        --- /dev/null
        +++ new_file.txt
        @@ -0,0 +1 @@
        +content
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "new_file.txt");
    assert_eq!(patch.hunks.len(), 1);
    assert_eq!(patch.hunks[0].old_start_line, Some(0));
    assert_eq!(patch.hunks[0].get_replace_block(), vec!["content"]);
}

#[test]
fn test_parse_error_on_missing_file_header() {
    let diff = indoc! {"
        Some text on line 1.
        ```diff
        @@ -1,2 +1,2 @@
        -foo
        +bar
        ```
    "};
    let result = parse_diffs(diff);
    assert!(matches!(
        result,
        Err(ParseError::MissingFileHeader { line: 2 })
    ));
}

#[test]
fn test_apply_simple_patch() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "line one\nline two\nline three\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         line one
        -line two
        +line 2
         line three
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "line one\nline 2\nline three\n");
}

#[test]
fn test_apply_multiple_hunks_in_one_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("multi.txt");
    let original_content = "Header\n\nunchanged line 1\n\nMiddle\n\nunchanged line 2\n\nFooter\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/multi.txt
        +++ b/multi.txt
        @@ -1,3 +1,3 @@
        -Header
        +New Header
         
         unchanged line 1
        @@ -7,3 +7,3 @@
         unchanged line 2
         
        -Footer
        +New Footer
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    let expected_content =
        "New Header\n\nunchanged line 1\n\nMiddle\n\nunchanged line 2\n\nNew Footer\n";
    assert_eq!(content, expected_content);
}

#[test]
fn test_file_creation() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("new_file.txt");

    let diff = indoc! {"
        ```diff
        --- a/new_file.txt
        +++ b/new_file.txt
        @@ -0,0 +1,2 @@
        +Hello
        +New World
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "Hello\nNew World\n");
}

#[test]
fn test_patch_to_empty_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("empty.txt");
    fs::write(&file_path, "").unwrap(); // Create an existing, empty file

    let diff = indoc! {"
        ```diff
        --- a/empty.txt
        +++ b/empty.txt
        @@ -0,0 +1,2 @@
        +line 1
        +line 2
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "line 1\nline 2\n");
}

#[test]
fn test_file_creation_in_subdirectory() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("src/new_file.txt");

    let diff = indoc! {"
        ```diff
        --- a/src/new_file.txt
        +++ b/src/new_file.txt
        @@ -0,0 +1 @@
        +hello from subdir
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    assert!(file_path.exists());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "hello from subdir\n");
}

#[test]
fn test_file_deletion_by_removing_all_content() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("delete_me.txt");
    fs::write(&file_path, "line 1\nline 2\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/delete_me.txt
        +++ b/delete_me.txt
        @@ -1,2 +0,0 @@
        -line 1
        -line 2
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, ""); // The file is now empty
}

#[test]
fn test_no_newline_at_end_of_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "line one\n").unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1 +1 @@
        -line one
        +line one no newline
        \ No newline at end of file
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "line one no newline");
}

#[test]
fn test_fuzzy_match_succeeds() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The context in the file is slightly different from the patch
    fs::write(&file_path, "context A\nline two\ncontext C\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         context one
        -line two
        +line 2
         context three
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // Use a fuzz factor that allows the match
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.5,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    // The expected behavior of patch is to replace the matched block
    // with the content from the patch, including the context lines.
    assert_eq!(content, "context one\nline 2\ncontext three\n");
}

#[test]
fn test_fuzzy_match_with_internal_insertion() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file has an extra line "inserted line" compared to the patch's context.
    fs::write(
        &file_path,
        "context A\ninserted line\nline to change\ncontext C\n",
    )
    .unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         context A
        -line to change
        +line was changed
         context C
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // The old fixed-window logic would fail this. The new flexible window should find it.
    // It should match the 4 lines in the file against the 3 lines in the patch context.
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.7,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply by matching a slightly larger context block"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "context A\nline was changed\ncontext C\n");
}

#[test]
fn test_match_with_different_trailing_whitespace() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("whitespace.txt");
    // Note the trailing spaces
    fs::write(&file_path, "line one  \nchange me\nline three\t\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/whitespace.txt
        +++ b/whitespace.txt
        @@ -1,3 +1,3 @@
         line one
        -change me
        +changed
         line three
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // This should succeed with exact matching because of the trailing whitespace logic
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply by ignoring trailing whitespace"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "line one\nchanged\nline three\n");
}

#[test]
fn test_ambiguous_match_fails() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The context appears at line 1 and line 5.
    fs::write(
        &file_path,
        "header\nchange me\nfooter\n\nheader\nchange me\nfooter\n",
    )
    .unwrap();

    // The line number hint is 3, which is equidistant from both matches (1 and 5).
    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -3,3 +3,3 @@
         header
        -change me
        +changed
         footer
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // This should fail because the context appears twice and the hint is ambiguous
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        !result.report.all_applied_cleanly(),
        "Patch should have failed due to ambiguity"
    );
    assert!(matches!(
        result.report.hunk_results[0],
        HunkApplyStatus::Failed(HunkApplyError::AmbiguousExactMatch(_))
    ));
    // Ensure file is unchanged
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(
        content,
        "header\nchange me\nfooter\n\nheader\nchange me\nfooter\n"
    );
}

#[test]
fn test_ambiguous_fuzzy_match_fails() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // Two sections that are "equally different" from the patch context.
    // The best fuzzy matches will be at line 1 and line 5.
    let original_content =
        "section one\ncommon line\nDIFFERENT A\n\nsection two\ncommon line\nDIFFERENT B\n";
    fs::write(&file_path, original_content).unwrap();

    // The line number hint is 3, which is equidistant from the two best fuzzy matches
    // at line 1 (dist 2) and line 5 (dist 2).
    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -3,3 +3,3 @@
         section
        -common line
        +changed line
         DIFFERENT
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // This should fail because two locations have the same fuzzy score and the hint is ambiguous
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.5,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        !result.report.all_applied_cleanly(),
        "Patch should have failed due to fuzzy ambiguity"
    );
    assert!(matches!(
        result.report.hunk_results[0],
        HunkApplyStatus::Failed(HunkApplyError::AmbiguousFuzzyMatch(_))
    ));
    // Ensure file is unchanged
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, original_content);
}

#[test]
fn test_dry_run() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let original_content = "line one\nline two\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,2 +1,2 @@
         line one
        -line two
        +line 2
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: true,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap(); // dry_run = true

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_some());
    // File should not have been modified
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, original_content);
}

#[test]
fn test_path_traversal_is_blocked() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    // This diff attempts to write outside the target directory
    let diff = indoc! {"
        ```diff
        --- a/../evil.txt
        +++ b/../evil.txt
        @@ -0,0 +1 @@
        +hacked
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options);

    assert!(matches!(result, Err(PatchError::PathTraversal(_))));
    // Ensure no file was created outside the temp dir
    let evil_path = dir.path().parent().unwrap().join("evil.txt");
    assert!(!evil_path.exists());
}

#[test]
fn test_path_traversal_with_dot_is_blocked() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    // This diff attempts to write outside the target directory using a `.` component
    let diff = indoc! {"
        ```diff
        --- a/./../evil.txt
        +++ b/./../evil.txt
        @@ -0,0 +1 @@
        +hacked
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options);

    assert!(matches!(result, Err(PatchError::PathTraversal(_))));
    let evil_path = dir.path().parent().unwrap().join("evil.txt");
    assert!(!evil_path.exists());
}

#[test]
fn test_apply_to_nonexistent_file_fails_if_not_creation() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    // Note: file "missing.txt" is NOT created.

    let diff = indoc! {"
        ```diff
        --- a/missing.txt
        +++ b/missing.txt
        @@ -1 +1 @@
        -foo
        +bar
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options);

    assert!(matches!(result, Err(PatchError::TargetNotFound(_))));
}

#[test]
fn test_partial_apply_fails_on_second_hunk() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("partial.txt");
    let original_content = "line 1\nline 2\nline 3\n\nline 5\nline 6\nline 7\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/partial.txt
        +++ b/partial.txt
        @@ -1,3 +1,3 @@
         line 1
        -line 2
        +line two
         line 3
        @@ -5,3 +5,3 @@
         line 5
        -line WRONG
        +line six
         line 7
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    // The second hunk has wrong context ("line WRONG") and will fail to apply.
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    // The operation should be reported as a soft failure.
    assert!(!result.report.all_applied_cleanly());

    // Check the new failures() method
    let failures = result.report.failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].hunk_index, 2);
    assert!(matches!(
        failures[0].reason,
        HunkApplyError::ContextNotFound
    ));
    // The file should be in a partially-patched state (first hunk applied).
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(result.report.hunk_results.len(), 2);
    assert!(
        matches!(&result.report.hunk_results[0], HunkApplyStatus::Applied { replaced_lines, .. } if replaced_lines.as_slice() == ["line 1", "line 2", "line 3"])
    );
    assert!(matches!(
        result.report.hunk_results[1],
        HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
    ));
    let expected_content_after_first_hunk = "line 1\nline two\nline 3\n\nline 5\nline 6\nline 7\n";
    assert_eq!(content, expected_content_after_first_hunk);
}

#[test]
fn test_creation_patch_fails_on_non_empty_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("exists.txt");
    fs::write(&file_path, "I already exist.\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/exists.txt
        +++ b/exists.txt
        @@ -0,0 +1 @@
        +new content
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // This should fail because a creation patch (empty match block) cannot apply to a non-empty file.
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        !result.report.all_applied_cleanly(),
        "Creation patch should fail on a non-empty file"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "I already exist.\n", "File should be unchanged");
}

#[test]
fn test_hunk_with_no_changes_is_skipped() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let original_content = "line 1\nline 2\nline 3\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         line 1
         line 2
         line 3
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    assert!(!patch.hunks[0].has_changes());
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch with no changes should apply successfully"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, original_content, "File should be unchanged");
}

#[test]
fn test_parse_empty_diff_block() {
    let diff = indoc! {"
        Some text.
        ```diff
        ```
        More text.
    "};
    let patches = parse_diffs(diff).unwrap();
    assert!(
        patches.is_empty(),
        "Parsing an empty diff block should result in no patches"
    );
}

#[test]
fn test_parse_diff_block_with_header_only() {
    let diff = indoc! {"
        ```diff
        --- a/some_file.txt
        +++ b/some_file.txt
        ```
    "};
    let patches = parse_diffs(diff).unwrap();
    assert!(
        patches.is_empty(),
        "Parsing a diff block with only a header should result in no patches"
    );
}

#[test]
fn test_indented_diff_block_is_ignored() {
    let diff = indoc! {r#"
        This should not be parsed.
          ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -a
        +b
          ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert!(patches.is_empty(), "Indented diff blocks should be ignored");
}

#[test]
fn test_find_hunk_location_in_lines() {
    let original_lines = vec!["line 1", "line two", "line 3"];
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         line 1
        -line two
        +line 2
         line 3
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    let options = mpatch::ApplyOptions {
        fuzz_factor: 0.0,
        ..Default::default()
    };
    // Test with &[&str]
    let (location, match_type) =
        find_hunk_location_in_lines(hunk, &original_lines, &options).unwrap();
    assert_eq!(
        location,
        HunkLocation {
            start_index: 0,
            length: 3
        }
    );
    assert!(matches!(match_type, MatchType::Exact));

    // Test with &[String]
    let original_lines_string: Vec<String> = original_lines.iter().map(|s| s.to_string()).collect();
    let (location2, match_type2) =
        find_hunk_location_in_lines(hunk, &original_lines_string, &options).unwrap();
    assert_eq!(location, location2);
    assert_eq!(match_type, match_type2);
}

#[test]
fn test_apply_patch_to_lines() {
    let original_lines = vec!["Hello, world!"];
    let diff_str = [
        "```diff",
        "--- a/hello.txt",
        "+++ b/hello.txt",
        "@@ -1 +1 @@",
        "-Hello, world!",
        "+Hello, mpatch!",
        "```",
    ]
    .join("\n");

    let patches = parse_diffs(&diff_str).unwrap();
    let patch = &patches[0];

    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_lines(patch, Some(&original_lines), &options);

    assert_eq!(result.new_content, "Hello, mpatch!\n");
    assert!(result.report.all_applied_cleanly());
}

#[test]
fn test_apply_hunk_to_lines_in_place() {
    let mut original_lines = vec![
        "line 1".to_string(),
        "line two".to_string(),
        "line 3".to_string(),
    ];
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         line 1
        -line two
        +line 2
         line 3
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    let options = mpatch::ApplyOptions {
        fuzz_factor: 0.0,
        ..Default::default()
    };

    // Test success case
    let status = apply_hunk_to_lines(hunk, &mut original_lines, &options);

    assert!(
        matches!(status, HunkApplyStatus::Applied { replaced_lines, .. } if replaced_lines.as_slice() == ["line 1", "line two", "line 3"])
    );
    assert_eq!(original_lines, vec!["line 1", "line 2", "line 3"]);

    // Test failure case
    let mut failing_lines = vec!["completely".to_string(), "different".to_string()];
    let fail_status = apply_hunk_to_lines(hunk, &mut failing_lines, &options);
    assert!(matches!(
        fail_status,
        HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
    ));
    // Ensure lines are unchanged on failure
    assert_eq!(failing_lines, vec!["completely", "different"]);
}

#[test]
fn test_hunk_applier_iterator() {
    let original_content = "line 1\nline 2\nline 3\n\nline 5\nline 6\nline 7\n";
    let original_lines: Vec<_> = original_content.lines().collect();
    let diff = indoc! {r#"
        ```diff
        --- a/partial.txt
        +++ b/partial.txt
        @@ -1,3 +1,3 @@
         line 1
        -line 2
        +line two
         line 3
        @@ -5,3 +5,3 @@
         line 5
        -line WRONG
        +line six
         line 7
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };

    let mut applier = mpatch::HunkApplier::new(patch, Some(&original_lines), &options);

    // Apply first hunk
    let status1 = applier.next().unwrap();
    assert!(
        matches!(status1, HunkApplyStatus::Applied { replaced_lines, .. } if replaced_lines.as_slice() == ["line 1", "line 2", "line 3"])
    );
    assert_eq!(
        applier.current_lines(),
        &["line 1", "line two", "line 3", "", "line 5", "line 6", "line 7"]
    );

    // Apply second hunk (which will fail)
    let status2 = applier.next().unwrap();
    assert!(matches!(
        status2,
        HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
    ));
    // Content should be unchanged from the previous step
    assert_eq!(
        applier.current_lines(),
        &["line 1", "line two", "line 3", "", "line 5", "line 6", "line 7"]
    );

    // No more hunks
    assert!(applier.next().is_none());

    // Finalize
    let new_content = applier.into_content();
    let expected_content = "line 1\nline two\nline 3\n\nline 5\nline 6\nline 7\n";
    assert_eq!(new_content, expected_content);
}

#[test]
fn test_fuzzy_match_below_threshold_fails() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let original_content = "completely different content\nthat has no resemblance\nto the patch\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         context one
        -line two
        +line 2
         context three
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // Use a high fuzz factor that will not be met
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.9,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        !result.report.all_applied_cleanly(),
        "Patch should fail to apply as no hunk meets the fuzzy threshold"
    );
    assert!(matches!(
        result.report.hunk_results[0],
        HunkApplyStatus::Failed(HunkApplyError::FuzzyMatchBelowThreshold { location, .. }) if location.start_index == 0
    ));
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, original_content, "File should be unchanged");
}

#[test]
fn test_find_hunk_location_exact_match() {
    let original_content = "line 1\nline two\nline 3\n";
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         line 1
        -line two
        +line 2
         line 3
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    let options = mpatch::ApplyOptions {
        fuzz_factor: 0.0,
        ..Default::default()
    };
    let (location, match_type) = find_hunk_location(hunk, original_content, &options).unwrap();
    assert_eq!(
        location,
        HunkLocation {
            start_index: 0,
            length: 3
        }
    );
    assert!(matches!(match_type, MatchType::Exact));
}

#[test]
fn test_find_hunk_location_fuzzy_match() {
    // The file has an extra line compared to the patch's context.
    let original_content = "context A\ninserted line\nline to change\ncontext C\n";
    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         context A
        -line to change
        +line was changed
         context C
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    // The flexible window should find a match of length 4.
    let options = mpatch::ApplyOptions {
        fuzz_factor: 0.7,
        ..Default::default()
    };
    let (location, match_type) = find_hunk_location(hunk, original_content, &options).unwrap();
    assert_eq!(
        location,
        HunkLocation {
            start_index: 0,
            length: 4
        }
    );
    assert!(matches!(match_type, MatchType::Fuzzy { .. }));
}

#[test]
fn test_find_hunk_location_not_found() {
    let original_content = "completely different content\n";
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,1 +1,1 @@
        -foo
        +bar
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    let options = mpatch::ApplyOptions {
        fuzz_factor: 0.9,
        ..Default::default()
    };
    let result = find_hunk_location(hunk, original_content, &options);
    assert!(matches!(
        result,
        Err(HunkApplyError::FuzzyMatchBelowThreshold { location, .. }) if location.start_index == 0
    ));
}

#[test]
fn test_find_hunk_location_ambiguous() {
    let original_content = "duplicate\n\nduplicate\n";
    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -2,1 +2,1 @@
        -duplicate
        +changed
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    let options = mpatch::ApplyOptions {
        fuzz_factor: 0.0,
        ..Default::default()
    };
    let result = find_hunk_location(hunk, original_content, &options);
    assert!(matches!(
        result,
        Err(HunkApplyError::AmbiguousExactMatch(_))
    ));
}

#[test]
#[cfg(unix)] // fs::set_readonly is not stable on all platforms, but works on unix.
fn test_apply_to_readonly_file_fails() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("readonly.txt");
    let original_content = "don't change me\n";
    fs::write(&file_path, original_content).unwrap();

    // Get original permissions to restore them later
    let original_perms = fs::metadata(&file_path).unwrap().permissions();

    // Set file to read-only
    let mut perms = original_perms.clone();
    perms.set_readonly(true);
    fs::set_permissions(&file_path, perms).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/readonly.txt
        +++ b/readonly.txt
        @@ -1 +1 @@
        -don't change me
        +I tried to change you
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options);

    assert!(
        matches!(result, Err(PatchError::PermissionDenied { .. })),
        "Applying patch to a read-only file should result in a PermissionDenied error"
    );

    // Reset permissions to allow cleanup by tempdir
    fs::set_permissions(&file_path, original_perms).unwrap();

    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(
        content, original_content,
        "Read-only file should not be changed"
    );
}

#[test]
fn test_apply_to_path_that_is_a_directory() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let dir_as_file_path = dir.path().join("a_directory");
    fs::create_dir(&dir_as_file_path).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/a_directory
        +++ b/a_directory
        @@ -1 +1 @@
        -foo
        +bar
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options);

    // Reading the original file content will fail because it's a directory.
    assert!(
        matches!(result, Err(PatchError::TargetIsDirectory { .. })),
        "Applying patch to a path that is a directory should fail with TargetIsDirectory"
    );
}

#[test]
fn test_file_creation_with_spaces_in_path() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("a file with spaces.txt");

    let diff = indoc! {"
        ```diff
        --- a/a file with spaces.txt
        +++ b/a file with spaces.txt
        @@ -0,0 +1 @@
        +content
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should be applied successfully"
    );
    assert!(result.diff.is_none());
    assert!(
        file_path.exists(),
        "File with spaces in name should be created"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "content\n");
}

#[test]
fn test_apply_hunk_to_file_beginning() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "line 1\nline 2\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,2 +1,3 @@
        +new first line
         line 1
         line 2
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "new first line\nline 1\nline 2\n");
}

#[test]
fn test_apply_hunk_to_file_end() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "line 1\nline 2\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,2 +1,3 @@
         line 1
         line 2
        +new last line
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "line 1\nline 2\nnew last line\n");
}

#[test]
fn test_parse_diff_with_git_headers() {
    let diff = indoc! {r#"
        ```diff
        diff --git a/src/main.rs b/src/main.rs
        index 1234567..abcdefg 100644
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1,3 +1,3 @@
         fn main() {
        -    println!("Hello, world!");
        +    println!("Hello, mpatch!");
         }
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "src/main.rs");
    assert_eq!(patch.hunks.len(), 1);
    assert_eq!(
        patch.hunks[0].get_replace_block(),
        vec!["fn main() {", "    println!(\"Hello, mpatch!\");", "}"]
    );
}

#[test]
fn test_path_normalization_within_project() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let src_dir = dir.path().join("src");
    fs::create_dir(&src_dir).unwrap();
    let file_path = dir.path().join("main.rs");
    fs::write(&file_path, "fn main() {}\n").unwrap();

    // This patch uses a path that contains '..' but normalizes
    // to a path still within the project root.
    let diff = indoc! {"
        ```diff
        --- a/src/../main.rs
        +++ b/src/../main.rs
        @@ -1 +1 @@
        -fn main() {}
        +fn main() { /* changed */ }
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // The patch is applied from the project root (`dir`).
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch with '..' that resolves inside the project should apply"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "fn main() { /* changed */ }\n");
}

#[test]
fn test_apply_hunk_with_single_line_match_block() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "unique_line\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,1 +1,1 @@
        -unique_line
        +changed_line
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    assert_eq!(patch.hunks[0].get_match_block(), vec!["unique_line"]);
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "changed_line\n");
}

#[test]
fn test_file_creation_with_unicode_path() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_name = "文件.txt";
    let file_path = dir.path().join(file_name);

    let diff = format!(
        indoc! {r#"
        ```diff
        --- a/{}
        +++ b/{}
        @@ -0,0 +1 @@
        +内容
        ```
    "#},
        file_name, file_name
    );

    let patch = &parse_diffs(&diff).unwrap()[0];
    assert_eq!(patch.file_path.to_str().unwrap(), file_name);
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should be applied successfully"
    );
    assert!(result.diff.is_none());
    assert!(
        file_path.exists(),
        "File with unicode name should be created"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "内容\n");
}

#[test]
#[cfg(unix)] // Behavior of absolute paths in `join` is platform-specific.
fn test_path_traversal_with_absolute_path_is_blocked() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    // This diff attempts to write to an absolute path.
    let diff = indoc! {"
        ```diff
        --- a//etc/evil.txt
        +++ b//etc/evil.txt
        @@ -0,0 +1 @@
        +hacked
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options);

    assert!(matches!(result, Err(PatchError::PathTraversal(_))));
}

#[test]
fn test_apply_patch_where_file_is_prefix_of_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file is missing "line 3" which is part of the patch's context.
    let original_content = "line 1\nline 2\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         line 1
         line 2
        -line 3
        +line three
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    // Use fuzzy matching to enable the end-of-file logic.
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.7,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply via end-of-file fuzzy logic"
    );
    let content = fs::read_to_string(file_path).unwrap();
    // The entire file content should be replaced by the patch's `replace_block`.
    assert_eq!(content, "line 1\nline 2\nline three\n");
}

#[test]
fn test_apply_patch_at_end_of_file_with_fuzz_and_missing_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.rs");
    // Note: original content is missing the final `}` and a newline from the patch context.
    fs::write(&file_path, "fn main() {\n    println!(\"Hello\");\n").unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/test.rs
        +++ b/test.rs
        @@ -1,4 +1,5 @@
         fn main() {
             println!("Hello");
         }
        +    println!("World");
         
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.7,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply via end-of-file fuzzy logic"
    );
    let content = fs::read_to_string(file_path).unwrap();
    let expected_content = "fn main() {\n    println!(\"Hello\");\n}\n    println!(\"World\");\n\n";
    assert_eq!(content, expected_content);
}

#[test]
fn test_fuzzy_match_with_missing_line_in_patch_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file has an extra line ("line B") compared to the patch's context.
    // This simulates the case where a patch was generated from a slightly older
    // version of a file, which caused the original character-based diff to fail.
    fs::write(&file_path, "line A\nline B\nline C\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,2 +1,2 @@
         line A
        -line C
        +line changed
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // With line-based diffing, this should now have a high similarity score
    // and apply successfully, even though the patch context is missing a line.
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.7,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply successfully despite a missing line in its context"
    );
    let content = fs::read_to_string(file_path).unwrap();
    // The fuzzy match should identify the 3-line block in the file and replace it
    // with the 2-line replacement block from the patch.
    assert_eq!(content, "line A\nline changed\n");
}

#[test]
fn test_fuzzy_match_with_extra_line_in_patch_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file is missing a line ("line B") that exists in the patch's context.
    fs::write(&file_path, "line A\nline C\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,2 @@
         line A
         line B
        -line C
        +line changed
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // The fuzzy logic should match the 3-line context against the 2-line file
    // content and apply the change.
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.7,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply successfully despite an extra line in its context"
    );
    let content = fs::read_to_string(file_path).unwrap();
    // The fuzzy match should identify the 2-line block in the file ("line A", "line C")
    // and replace it with the 3-line replacement block from the patch
    // ("line A", "line B", "line changed").
    assert_eq!(content, "line A\nline B\nline changed\n");
}

#[test]
fn test_parse_hunk_header_line_number() {
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +2,3 @@
         a
        -b
        +c
         d
        @@ -10,1 +12,1 @@
        -x
        +y
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.hunks.len(), 2);
    assert_eq!(patch.hunks[0].old_start_line, Some(1));
    assert_eq!(patch.hunks[0].new_start_line, Some(2));
    assert_eq!(patch.hunks[1].old_start_line, Some(10));
    assert_eq!(patch.hunks[1].new_start_line, Some(12));
}

#[test]
fn test_ambiguous_match_resolved_by_line_number() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let original_content = indoc! {"
        // Block 1
        fn duplicate() {
            println!(\"hello\");
        }

        // Block 2
        fn duplicate() {
            println!(\"hello\");
        }
    "};
    fs::write(&file_path, original_content).unwrap();

    // This patch targets the second block, indicated by the line number `@@ -7,...`
    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -7,3 +7,3 @@
         fn duplicate() {
        -    println!("hello");
        +    println!("world");
         }
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    assert_eq!(patch.hunks[0].old_start_line, Some(7));

    // This should succeed because the line number hint resolves the ambiguity.
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should have applied successfully using line number hint"
    );
    let content = fs::read_to_string(file_path).unwrap();
    let expected_content = indoc! {"
        // Block 1
        fn duplicate() {
            println!(\"hello\");
        }

        // Block 2
        fn duplicate() {
            println!(\"world\");
        }
    "};
    assert_eq!(content, expected_content);
}

#[test]
fn test_ambiguous_match_fails_with_equidistant_line_hint() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let original_content = "duplicate\n\nduplicate\n";
    fs::write(&file_path, original_content).unwrap();

    // This patch has a line number hint of 2, which is equidistant
    // from line 1 (dist 1) and line 3 (dist 1).
    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -2,1 +2,1 @@
        -duplicate
        +changed
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    assert_eq!(patch.hunks[0].old_start_line, Some(2));

    // This should fail because the ambiguity cannot be resolved.
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.0,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        !result.report.all_applied_cleanly(),
        "Patch should fail due to unresolved ambiguity"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, original_content, "File should be unchanged");
}

#[test]
fn test_hunk_semantic_helpers() {
    let hunk = mpatch::Hunk {
        lines: vec![
            " context 1".to_string(),
            "-removed 1".to_string(),
            "-removed 2".to_string(),
            "+added 1".to_string(),
            " context 2".to_string(),
        ],
        old_start_line: Some(1),
        new_start_line: Some(1),
    };

    assert_eq!(hunk.context_lines(), vec!["context 1", "context 2"]);
    assert_eq!(hunk.added_lines(), vec!["added 1"]);
    assert_eq!(hunk.removed_lines(), vec!["removed 1", "removed 2"]);
}

#[test]
fn test_patch_is_creation() {
    let creation_diff = indoc! {r#"
        ```diff
        --- a/new_file.txt
        +++ b/new_file.txt
        @@ -0,0 +1,2 @@
        +Hello
        +World
        ```
    "#};
    let patches = parse_diffs(creation_diff).unwrap();
    assert!(patches[0].is_creation());

    let modification_diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,1 +1,1 @@
        -foo
        +bar
        ```
    "#};
    let patches = parse_diffs(modification_diff).unwrap();
    assert!(!patches[0].is_creation());
}

#[test]
fn test_patch_is_deletion() {
    let deletion_diff = indoc! {r#"
        ```diff
        --- a/old_file.txt
        +++ b/old_file.txt
        @@ -1,2 +0,0 @@
        -Hello
        -World
        ```
    "#};
    let patches = parse_diffs(deletion_diff).unwrap();
    assert!(patches[0].is_deletion());

    let modification_diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,1 +1,1 @@
        -foo
        +bar
        ```
    "#};
    let patches = parse_diffs(modification_diff).unwrap();
    assert!(!patches[0].is_deletion());

    let partial_removal_diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,1 @@
        -foo
        -bar
         baz
        ```
    "#};
    let patches = parse_diffs(partial_removal_diff).unwrap();
    // This is not a full deletion because the replace block contains "baz".
    assert!(!patches[0].is_deletion());
}

#[test]
fn test_apply_options_builder() {
    let options = ApplyOptions::builder()
        .dry_run(true)
        .fuzz_factor(0.99)
        .build();
    assert!(options.dry_run);
    assert_eq!(options.fuzz_factor, 0.99);

    let default_options = ApplyOptions::builder().build();
    assert!(!default_options.dry_run);
    assert_eq!(default_options.fuzz_factor, 0.7);
}

#[test]
fn test_patch_from_texts() {
    let old_text = "hello\nworld\n";
    let new_text = "hello\nrust\n";
    let patch = Patch::from_texts("file.txt", old_text, new_text, 3).unwrap();

    assert_eq!(patch.file_path.to_str(), Some("file.txt"));
    assert_eq!(patch.hunks.len(), 1);
    let hunk = &patch.hunks[0];
    assert_eq!(hunk.context_lines(), vec!["hello"]);
    assert_eq!(hunk.removed_lines(), vec!["world"]);
    assert_eq!(hunk.added_lines(), vec!["rust"]);
}

#[test]
fn test_patch_from_texts_no_change() {
    let old_text = "hello\nworld\n";
    let patch = Patch::from_texts("file.txt", old_text, old_text, 3).unwrap();
    assert!(patch.hunks.is_empty());
}

#[test]
fn test_patch_inversion() {
    let old_text = "line 1\nline 2\n";
    let new_text = "line 1\nline two\n";
    let patch = Patch::from_texts("file.txt", old_text, new_text, 3).unwrap();
    let inverted_patch = patch.invert();

    assert_eq!(inverted_patch.hunks.len(), 1);
    let inverted_hunk = &inverted_patch.hunks[0];
    assert_eq!(inverted_hunk.removed_lines(), vec!["line two"]);
    assert_eq!(inverted_hunk.added_lines(), vec!["line 2"]);

    // Apply the original patch
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, old_text).unwrap();
    apply_patch_to_file(&patch, dir.path(), ApplyOptions::default()).unwrap();
    let content_after_patch = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content_after_patch, new_text);

    // Apply the inverted patch
    apply_patch_to_file(&inverted_patch, dir.path(), ApplyOptions::default()).unwrap();
    let content_after_inversion = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content_after_inversion, old_text);
}

#[test]
fn test_apply_patches_to_dir() {
    let dir = tempdir().unwrap();
    let file1_path = dir.path().join("file1.txt");
    let file2_path = dir.path().join("file2.txt");
    fs::write(&file1_path, "foo\n").unwrap();
    fs::write(&file2_path, "baz\n").unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/file1.txt
        +++ b/file1.txt
        @@ -1 +1 @@
        -foo
        +bar
        --- a/file2.txt
        +++ b/file2.txt
        @@ -1 +1 @@
        -baz
        +qux
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 2);

    let batch_result = apply_patches_to_dir(&patches, dir.path(), ApplyOptions::default());

    assert!(batch_result.all_succeeded());
    assert!(batch_result.hard_failures().is_empty());
    assert_eq!(batch_result.results.len(), 2);

    let content1 = fs::read_to_string(file1_path).unwrap();
    let content2 = fs::read_to_string(file2_path).unwrap();
    assert_eq!(content1, "bar\n");
    assert_eq!(content2, "qux\n");
}

mod ensure_path_is_safe_tests {
    use mpatch::{ensure_path_is_safe, PatchError};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_safe_path_succeeds() {
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let safe_path = "src/main.rs";

        // We need to create the file for the `exists()` branch to be tested
        fs::create_dir_all(base_dir.join("src")).unwrap();
        fs::write(base_dir.join(safe_path), "content").unwrap();

        let result = ensure_path_is_safe(base_dir, safe_path.as_ref());
        assert!(result.is_ok());
        let resolved_path = result.unwrap();
        assert!(resolved_path.ends_with(safe_path));
        assert!(resolved_path.is_absolute());
    }

    #[test]
    fn test_safe_path_to_nonexistent_file_succeeds() {
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let safe_path = "new/file.txt";

        let result = ensure_path_is_safe(base_dir, safe_path.as_ref());
        assert!(result.is_ok());
        let resolved_path = result.unwrap();
        assert!(resolved_path.ends_with(safe_path));
        assert!(resolved_path.is_absolute());
        // The function creates the parent directory
        assert!(base_dir.join("new").is_dir());
    }

    #[test]
    fn test_traversal_path_fails() {
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let unsafe_path = "../evil.txt";

        let result = ensure_path_is_safe(base_dir, unsafe_path.as_ref());
        assert!(matches!(result, Err(PatchError::PathTraversal(_))));
    }

    #[test]
    fn test_traversal_path_to_nonexistent_file_fails() {
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let unsafe_path = "src/../../evil.txt";

        let result = ensure_path_is_safe(base_dir, unsafe_path.as_ref());
        assert!(matches!(result, Err(PatchError::PathTraversal(_))));
    }

    #[test]
    #[cfg(unix)]
    fn test_absolute_path_fails() {
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let unsafe_path = "/etc/passwd";

        let result = ensure_path_is_safe(base_dir, unsafe_path.as_ref());
        assert!(matches!(result, Err(PatchError::PathTraversal(_))));
    }

    #[test]
    fn test_path_normalization_within_project_succeeds() {
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        fs::create_dir(base_dir.join("src")).unwrap();
        let normalized_path = "src/../main.rs";
        fs::write(base_dir.join("main.rs"), "content").unwrap();

        let result = ensure_path_is_safe(base_dir, normalized_path.as_ref());
        assert!(result.is_ok());
        let resolved = result.unwrap();
        assert!(resolved.ends_with("main.rs"));
    }
}

mod hunk_finder_tests {
    use super::*; // Import everything from the parent module
    use mpatch::Hunk;

    fn setup_hunk(diff_content: &str) -> Hunk {
        parse_diffs(diff_content).unwrap().remove(0).hunks.remove(0)
    }

    #[test]
    fn test_default_finder_exact_match() {
        let options = ApplyOptions {
            fuzz_factor: 0.0,
            ..Default::default()
        };
        let finder = DefaultHunkFinder::new(&options);

        let hunk = setup_hunk(indoc! {r#"
            ```diff
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,3 @@
             line 1
            -line two
            +line 2
             line 3
            ```
        "#});
        let target_lines = vec!["line 1", "line two", "line 3"];

        let (location, match_type) = finder.find_location(&hunk, &target_lines).unwrap();

        assert_eq!(
            location,
            HunkLocation {
                start_index: 0,
                length: 3
            }
        );
        assert!(matches!(match_type, MatchType::Exact));
    }

    #[test]
    fn test_default_finder_fuzzy_match() {
        let options = ApplyOptions {
            fuzz_factor: 0.7,
            ..Default::default()
        };
        let finder = DefaultHunkFinder::new(&options);

        let hunk = setup_hunk(indoc! {r#"
            ```diff
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,3 @@
             context A
            -line to change
            +line was changed
             context C
            ```
        "#});
        // File has an extra line, requiring a flexible fuzzy match
        let target_lines = vec!["context A", "inserted line", "line to change", "context C"];

        let (location, match_type) = finder.find_location(&hunk, &target_lines).unwrap();

        assert_eq!(
            location,
            HunkLocation {
                start_index: 0,
                length: 4
            }
        );
        assert!(matches!(match_type, MatchType::Fuzzy { .. }));
    }

    #[test]
    fn test_default_finder_not_found() {
        let options = ApplyOptions {
            fuzz_factor: 0.9,
            ..Default::default()
        };
        let finder = DefaultHunkFinder::new(&options);

        let hunk = setup_hunk(indoc! {r#"
            ```diff
            --- a/file.txt
            +++ b/file.txt
            @@ -1,1 +1,1 @@
            -foo
            +bar
            ```
        "#});
        let target_lines = vec!["completely", "different", "content"];

        let result = finder.find_location(&hunk, &target_lines);
        assert!(matches!(
            result,
            Err(HunkApplyError::FuzzyMatchBelowThreshold { location, .. }) if location.start_index == 0
        ));
    }

    #[test]
    fn test_default_finder_ambiguous_match() {
        let options = ApplyOptions {
            fuzz_factor: 0.0,
            ..Default::default()
        };
        let finder = DefaultHunkFinder::new(&options);

        let hunk = setup_hunk(indoc! {r#"
            ```diff
            --- a/file.txt
            +++ b/file.txt
            @@ -2,1 +2,1 @@
            -duplicate
            +changed
            ```
        "#});
        let target_lines = vec!["duplicate", "", "duplicate"];

        let result = finder.find_location(&hunk, &target_lines);
        assert!(matches!(
            result,
            Err(HunkApplyError::AmbiguousExactMatch(_))
        ));
    }
}
