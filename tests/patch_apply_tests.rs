use indoc::indoc;
use mpatch::{apply_patch, parse_diffs, PatchError};
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
    assert_eq!(hunk.get_match_block(), vec!["fn main() {", "    println!(\"Hello, world!\");", "}"]);
    assert_eq!(hunk.get_replace_block(), vec!["fn main() {", "    println!(\"Hello, mpatch!\");", "}"]);
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
fn test_parse_error_on_missing_file_header() {
    let diff = indoc! {"
        ```diff
        @@ -1,2 +1,2 @@
        -foo
        +bar
        ```
    "};
    let result = parse_diffs(diff);
    assert!(matches!(result, Err(PatchError::MissingFileHeader)));
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result);
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result);
    let content = fs::read_to_string(file_path).unwrap();
    let expected_content = "New Header\n\nunchanged line 1\n\nMiddle\n\nunchanged line 2\n\nNew Footer\n";
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result);
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result);
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result);
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result);
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result);
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
    let result = apply_patch(patch, dir.path(), false, 0.5).unwrap();

    assert!(result);
    let content = fs::read_to_string(file_path).unwrap();
    // The expected behavior of patch is to replace the matched block
    // with the content from the patch, including the context lines.
    assert_eq!(content, "context one\nline 2\ncontext three\n");
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result, "Patch should apply by ignoring trailing whitespace");
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "line one\nchanged\nline three\n");
}

#[test]
fn test_ambiguous_match_fails() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "header\nchange me\nfooter\nheader\nchange me\nfooter\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         header
        -change me
        +changed
         footer
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // This should fail because the context appears twice
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(!result, "Patch should have failed due to ambiguity");
    // Ensure file is unchanged
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "header\nchange me\nfooter\nheader\nchange me\nfooter\n");
}

#[test]
fn test_ambiguous_fuzzy_match_fails() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // Two sections that are "equally different" from the patch context
    let original_content = "section one\ncommon line\nDIFFERENT A\n\nsection two\ncommon line\nDIFFERENT B\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         section
        -common line
        +changed line
         DIFFERENT
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    // This should fail because two locations have the same fuzzy score
    let result = apply_patch(patch, dir.path(), false, 0.5).unwrap();

    assert!(!result, "Patch should have failed due to fuzzy ambiguity");
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
    let result = apply_patch(patch, dir.path(), true, 0.0).unwrap(); // dry_run = true

    assert!(result);
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
    let result = apply_patch(patch, dir.path(), false, 0.0);

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
    let result = apply_patch(patch, dir.path(), false, 0.0);

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
    let result = apply_patch(patch, dir.path(), false, 0.0);

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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    // The operation should be reported as a soft failure.
    assert!(!result);

    // The file should be in a partially-patched state (first hunk applied).
    let content = fs::read_to_string(file_path).unwrap();
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(!result, "Creation patch should fail on a non-empty file");
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result, "Patch with no changes should apply successfully");
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
    assert!(patches.is_empty(), "Parsing an empty diff block should result in no patches");
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
    assert!(patches.is_empty(), "Parsing a diff block with only a header should result in no patches");
}

#[test]
fn test_fuzzy_match_fails_below_threshold() {
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
    let result = apply_patch(patch, dir.path(), false, 0.9).unwrap();

    assert!(!result, "Patch should fail to apply as no hunk meets the fuzzy threshold");
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, original_content, "File should be unchanged");
}

#[test]
#[cfg(unix)] // fs::set_readonly is not stable on all platforms, but works on unix.
fn test_apply_to_readonly_file_fails() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("readonly.txt");
    let original_content = "don't change me\n";
    fs::write(&file_path, original_content).unwrap();

    // Set file to read-only
    let mut perms = fs::metadata(&file_path).unwrap().permissions();
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
    let result = apply_patch(patch, dir.path(), false, 0.0);

    assert!(matches!(result, Err(PatchError::Io { .. })), "Applying patch to a read-only file should result in an I/O error");

    // Reset permissions to allow cleanup by tempdir
    let mut perms = fs::metadata(&file_path).unwrap().permissions();
    perms.set_readonly(false);
    fs::set_permissions(&file_path, perms).unwrap();

    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, original_content, "Read-only file should not be changed");
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
    let result = apply_patch(patch, dir.path(), false, 0.0);

    // Reading the original file content will fail because it's a directory.
    assert!(matches!(result, Err(PatchError::Io { .. })), "Applying patch to a path that is a directory should fail");
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
    let result = apply_patch(patch, dir.path(), false, 0.0).unwrap();

    assert!(result, "Patch should be applied successfully");
    assert!(file_path.exists(), "File with spaces in name should be created");
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "content\n");
}
