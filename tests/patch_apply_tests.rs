use indoc::indoc;
use mpatch::{
    apply_hunk_to_lines, apply_patch_to_file, apply_patch_to_lines, apply_patches_to_dir,
    detect_patch, find_hunk_location, find_hunk_location_in_lines, parse_auto, parse_diffs,
    parse_patches, parse_patches_from_lines, patch_content_str, try_apply_patch_to_content,
    try_apply_patch_to_file, try_apply_patch_to_lines, ApplyOptions, DefaultHunkFinder, Hunk,
    HunkApplyError, HunkApplyStatus, HunkFinder, HunkLocation, MatchType, ParseError, Patch,
    PatchError, PatchFormat, StrictApplyError,
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
fn test_parse_flexible_diff_block_headers() {
    let test_cases = vec![
        "```diff,rust",
        "```rust, diff",
        "```  patch ",
        "``` some info,patch,more info ",
        "```diff",       // no space
        "```patch",      // no space
        "``` diff",      // with space
        "``` diff rust", // multiple words
    ];

    for header in test_cases {
        let diff = format!(
            "{}\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-a\n+b\n```",
            header
        );
        let patches = parse_diffs(&diff).unwrap();
        assert_eq!(patches.len(), 1, "Failed for header: {}", header);
        assert_eq!(patches[0].file_path.to_str().unwrap(), "file.txt");
    }
}

#[test]
fn test_parse_accepts_all_code_blocks() {
    let test_cases = vec![
        "```rust",
        "```",
        "``` dif",        // partial match
        "``` patch-work", // not a whole word
        "```mydiff",      // not a whole word
        "```different",   // not a whole word
        "``` a,b,c",      // no diff/patch keyword
        "```patchwork",   // not a whole word
    ];

    for header in test_cases {
        let diff = format!(
            "{}\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-a\n+b\n```",
            header
        );
        let patches = parse_diffs(&diff).unwrap();
        assert_eq!(
            patches.len(),
            1,
            "Should have parsed block with header: {}",
            header
        );
        assert_eq!(patches[0].file_path.to_str().unwrap(), "file.txt");
    }
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
    let patches = parse_diffs(diff).unwrap();
    // With scan-all logic, blocks without headers are skipped/ignored to avoid false positives
    assert!(patches.is_empty());
}

#[test]
fn test_parse_patches_raw_diff() {
    let raw_diff = indoc! {r#"
        --- a/file1.txt
        +++ b/file1.txt
        @@ -1 +1 @@
        -foo
        +bar
    "#};
    let patches = parse_patches(raw_diff).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file1.txt");
    assert_eq!(patches[0].hunks.len(), 1);
    assert_eq!(patches[0].hunks[0].get_replace_block(), vec!["bar"]);
}

#[test]
fn test_parse_patches_multi_file_raw_diff() {
    let raw_diff = indoc! {r#"
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
    "#};
    let patches = parse_patches(raw_diff).unwrap();
    assert_eq!(patches.len(), 2);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file1.txt");
    assert_eq!(patches[1].file_path.to_str().unwrap(), "file2.txt");
}

#[test]
fn test_parse_ignores_irrelevant_code_blocks() {
    let content = indoc! {r#"
        Here is some rust code that is not a patch:
        ```rust
        fn main() {
            println!("Not a patch");
        }
        ```

        Here is a list:
        ```text
        - item 1
        - item 2
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert!(
        patches.is_empty(),
        "Should not find patches in standard code blocks that lack diff signatures"
    );
}

#[test]
fn test_parse_finds_patch_in_unlabeled_block() {
    let content = indoc! {r#"
        Here is a patch in a generic block:
        ```
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -old
        +new
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file.txt");
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["new"]);
}

#[test]
fn test_parse_finds_patch_in_misleading_language_block() {
    // Scenario: User mistakenly labeled the block as python, or it's a diff of python code
    // but they used the language tag 'python' instead of 'diff'.
    let content = indoc! {r#"
        ```python
        --- a/script.py
        +++ b/script.py
        @@ -1 +1 @@
        -print("old")
        +print("new")
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "script.py");
}

#[test]
fn test_parse_mixed_content_robustness() {
    // A complex file with TOML, a Patch, and Bash commands.
    let content = indoc! {r#"
        Step 1: Update config
        ```toml
        [package]
        name = "demo"
        ```

        Step 2: Apply this patch
        ```
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1 +1 @@
        -fn main() {}
        +fn main() { println!("hi"); }
        ```

        Step 3: Run it
        ```bash
        cargo run
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "src/main.rs");
}

#[test]
fn test_heuristic_skips_yaml_separators() {
    // YAML uses '---' which triggers the `starts_with("--- ")` check if followed by a space,
    // or just `---` (newline).
    // The parser should be robust enough to see `---` but no `+++` and return 0 patches.
    let content = indoc! {r#"
        ```yaml
        --- 
        title: Not a diff
        ---
        key: value
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_conflict_markers_in_rust_block() {
    // AI often outputs conflict markers inside a language-specific block.
    let content = indoc! {r#"
        ```rust
        fn main() {
        <<<<
            old();
        ====
            new();
        >>>>
        }
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].hunks[0].removed_lines(), vec!["    old();"]);
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["    new();"]);
}

#[test]
fn test_heuristic_trigger_but_invalid_diff_is_ignored() {
    // A block that triggers the "looks like patch" heuristic (has "--- ")
    // but isn't actually a valid diff (no "+++", no hunks).
    // It should return Ok(empty) rather than an error.
    let content = indoc! {r#"
        ```
        --- This looks like a header but isn't
        Just some text
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_block_with_only_hunk_no_header_is_skipped() {
    // If a block has `@@ ... @@` but no `---` or `diff --git` or `<<<<`,
    // the optimization heuristic `looks_like_patch` returns false.
    // This effectively skips blocks that are just fragments without file context,
    // which prevents "MissingFileHeader" errors for random code snippets that might look like hunks.
    let content = indoc! {r#"
        ```
        @@ -1 +1 @@
        -foo
        +bar
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_git_diff_header_triggers_parsing() {
    // Ensure `diff --git` triggers the parser even if `---` is further down.
    let content = indoc! {r#"
        ```
        diff --git a/file b/file
        index 0000000..1111111
        --- a/file
        +++ b/file
        @@ -1 +1 @@
        -a
        +b
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 1);
}

#[test]
fn test_yaml_block_with_header_like_content_is_ignored() {
    // YAML often uses "---" separators.
    // If a line is just "---", the heuristic `starts_with("--- ")` (note space) is false.
    // But "--- title" matches.
    // The parser should run, find no "+++", find no hunks, and safely return empty.
    let content = indoc! {r#"
        ```yaml
        --- title: Some YAML
        key: value
        ---
        other: value
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert!(patches.is_empty(), "YAML block should not produce patches");
}

#[test]
fn test_crlf_line_endings() {
    // Ensure the parser and heuristic handle Windows-style line endings.
    let content =
        "```diff\r\n--- a/file.txt\r\n+++ b/file.txt\r\n@@ -1 +1 @@\r\n-old\r\n+new\r\n```";
    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file.txt");
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["new"]);
}

#[test]
fn test_heuristic_skips_indented_unified_headers() {
    // Standard unified diffs require headers to be at the start of the line.
    // Indented headers inside a block are usually invalid or part of a quote/list.
    // The heuristic `starts_with("--- ")` enforces this strictness.
    let content = indoc! {r#"
        ```diff
          --- a/file.txt
          +++ b/file.txt
          @@ -1 +1 @@
          -old
          +new
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert!(
        patches.is_empty(),
        "Indented headers should be skipped by heuristic/parser"
    );
}

#[test]
fn test_multiple_blocks_with_noise() {
    // A stress test with a mix of valid patches, false positives, and noise.
    let content = indoc! {r#"
        # Documentation

        Here is a config example (should be ignored):
        ```yaml
        --- config
        setting: true
        ```

        Here is the actual fix (should be parsed):
        ```
        --- a/src/lib.rs
        +++ b/src/lib.rs
        @@ -1 +1 @@
        -bug
        +fix
        ```

        Here is a comment about bitwise operators (should be ignored):
        ```rust
        // We use << for shifting
        let x = 1 << 4;
        ```

        Here is a conflict marker block (should be parsed):
        ```
        <<<<
        old
        ====
        new
        >>>>
        ```
    "#};

    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 2);

    // First patch (Unified)
    assert_eq!(patches[0].file_path.to_str().unwrap(), "src/lib.rs");
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["fix"]);

    // Second patch (Conflict)
    assert_eq!(patches[1].file_path.to_str().unwrap(), "patch_target");
    assert_eq!(patches[1].hunks[0].added_lines(), vec!["new"]);
}

#[test]
fn test_horizontal_rule_in_markdown_code_block() {
    // Markdown-in-markdown might contain `---` horizontal rules.
    // These should not trigger the parser unless they look exactly like `--- path`.
    let content = indoc! {r#"
        ```markdown
        Title
        ---
        Content
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_diff_git_header_only_is_ignored() {
    // If a block has `diff --git ...` but no hunks or unified headers following it,
    // it triggers the heuristic but the parser should return empty (no hunks found).
    let content = indoc! {r#"
        ```
        diff --git a/file b/file
        index 123..456
        (end of block, no changes)
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_parse_patches_error_on_missing_header() {
    let raw_diff = indoc! {r#"
        @@ -1 +1 @@
        -foo
        +bar
    "#};
    assert!(matches!(
        parse_patches(raw_diff),
        Err(ParseError::MissingFileHeader { line: 1 })
    ));
}

#[test]
fn test_parse_patches_empty_input() {
    let patches = parse_patches("").unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_parse_patches_whitespace_input() {
    let patches = parse_patches("  \n\t\n  ").unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_parse_patches_file_creation() {
    let raw_diff = indoc! {r#"
        --- /dev/null
        +++ b/new_file.txt
        @@ -0,0 +1,2 @@
        +Hello
        +World
    "#};
    let patches = parse_patches(raw_diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "new_file.txt");
    assert!(patch.is_creation());
    assert_eq!(patch.hunks[0].get_replace_block(), vec!["Hello", "World"]);
}

#[test]
fn test_parse_patches_file_deletion() {
    let raw_diff = indoc! {r#"
        --- a/old_file.txt
        +++ b/old_file.txt
        @@ -1,2 +0,0 @@
        -Hello
        -World
    "#};
    let patches = parse_patches(raw_diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "old_file.txt");
    assert!(patch.is_deletion());
    assert_eq!(patch.hunks[0].get_match_block(), vec!["Hello", "World"]);
    assert!(patch.hunks[0].get_replace_block().is_empty());
}

#[test]
fn test_parse_patches_no_newline() {
    let raw_diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -foo
        +bar
        \ No newline at end of file
    "#};
    let patches = parse_patches(raw_diff).unwrap();
    assert_eq!(patches.len(), 1);
    assert!(!patches[0].ends_with_newline);
}

#[test]
fn test_parse_patches_with_git_headers() {
    let raw_diff = indoc! {r#"
        diff --git a/src/main.rs b/src/main.rs
        index 1234567..abcdefg 100644
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1 +1 @@
        -old
        +new
    "#};
    let patches = parse_patches(raw_diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "src/main.rs");
    assert_eq!(patch.hunks.len(), 1);
    assert_eq!(patch.hunks[0].get_replace_block(), vec!["new"]);
}

#[test]
fn test_parse_patches_merges_sections_for_same_file() {
    let raw_diff = indoc! {r#"
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
    "#};
    let patches = parse_patches(raw_diff).unwrap();
    assert_eq!(
        patches.len(),
        1,
        "Should merge sections into a single patch"
    );
    let patch = &patches[0];
    assert_eq!(patch.file_path.to_str().unwrap(), "same_file.txt");
    assert_eq!(patch.hunks.len(), 2, "Should contain two hunks");
    assert_eq!(patch.hunks[0].get_replace_block(), vec!["hunk one"]);
    assert_eq!(patch.hunks[1].get_replace_block(), vec!["hunk two"]);
}

#[test]
fn test_parse_patches_from_lines() {
    let raw_diff_lines = vec![
        "--- a/src/main.rs",
        "+++ b/src/main.rs",
        "@@ -1,3 +1,3 @@",
        " fn main() {",
        "-    println!(\"Hello, world!\");",
        "+    println!(\"Hello, mpatch!\");",
        " }",
    ];

    let patches = parse_patches_from_lines(raw_diff_lines.into_iter()).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str(), Some("src/main.rs"));
    assert_eq!(patches[0].hunks.len(), 1);
    assert_eq!(
        patches[0].hunks[0].added_lines(),
        vec!["    println!(\"Hello, mpatch!\");"]
    );
}

#[test]
fn test_parse_patches_from_lines_error() {
    let raw_diff_lines = vec![
        "@@ -1,3 +1,3 @@",
        "-    println!(\"Hello, world!\");",
        "+    println!(\"Hello, mpatch!\");",
    ];
    let result = parse_patches_from_lines(raw_diff_lines.into_iter());
    assert!(matches!(
        result,
        Err(ParseError::MissingFileHeader { line: 1 })
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    // The behavior preserves the file's original context on a fuzzy match.
    assert_eq!(content, "context A\nline 2\ncontext C\n");
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
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply by matching a slightly larger context block"
    );
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(
        content,
        "context A\ninserted line\nline was changed\ncontext C\n"
    );
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::dry_run();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let diff = r#"
        This should not be parsed.
          ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -a
        +b
          ```
    "#;
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

    let options = ApplyOptions::exact();
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

    let options = ApplyOptions::exact();
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

    let options = ApplyOptions::exact();

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
    let options = ApplyOptions::exact();

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
        HunkApplyStatus::Failed(HunkApplyError::FuzzyMatchBelowThreshold { .. })
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

    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::new();
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
        Err(HunkApplyError::FuzzyMatchBelowThreshold { .. })
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

    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::new();
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
    let options = ApplyOptions::new();
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
    let original_content = "line A\nline B\nline C\n";
    fs::write(&file_path, original_content).unwrap();

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
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    // The logic should preserve "line B" from the file and only change "line C".
    let expected_content = "line A\nline B\nline changed\n";

    if !result.report.all_applied_cleanly() || content != expected_content {
        eprintln!(
            "\n\n--- DIAGNOSTICS FOR `test_fuzzy_match_with_missing_line_in_patch_context` ---\n"
        );
        eprintln!("Original Content:\n```\n{}\n```", original_content);
        eprintln!("Patch:\n```diff\n{}\n```", diff);
        eprintln!("Apply Result: {:#?}", result.report);
        eprintln!("Expected Content:\n```\n{}\n```", expected_content);
        eprintln!("Actual Content:\n```\n{}\n```\n", content);
    }

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply successfully despite a missing line in its context"
    );
    assert_eq!(content, expected_content);
}

#[test]
fn test_finder_with_missing_line_in_patch_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let options = ApplyOptions::new();
    let finder = DefaultHunkFinder::new(&options);

    let hunk = parse_diffs(indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,2 +1,2 @@
         line A
        -line C
        +line changed
        ```
    "})
    .unwrap()
    .remove(0)
    .hunks
    .remove(0);

    let target_lines = vec!["line A", "line B", "line C"];

    // This test isolates the finder. The fuzzy logic should be smart enough to realize
    // that the best match is a 3-line window in the target file that accounts for the
    // inserted "line B", rather than a 2-line window that incorrectly matches "line B"
    // as a fuzzy version of "line C".
    let (location, match_type) = finder.find_location(&hunk, &target_lines).unwrap();

    assert!(matches!(match_type, MatchType::Fuzzy { .. }));
    assert_eq!(
        location,
        HunkLocation {
            start_index: 0,
            length: 3
        },
        "Finder should have matched all three lines to account for the insertion"
    );
}

#[test]
fn test_fuzzy_match_with_duplicated_context_line_insertion() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file has a duplicated "line A" which is not in the patch's context.
    let original_content = "line A\nline A\nline C\n";
    fs::write(&file_path, original_content).unwrap();

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
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    let expected_content = "line A\nline A\nline changed\n";

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply cleanly even with duplicated context lines"
    );
    assert_eq!(content, expected_content);
}

#[test]
fn test_fuzzy_match_with_more_context_and_insertion() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let original_content = "header\nline A\nline B\nline C\nfooter\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,4 +1,4 @@
         header
         line A
        -line C
        +line changed
         footer
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    let expected_content = "header\nline A\nline B\nline changed\nfooter\n";

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply cleanly with more context"
    );
    assert_eq!(content, expected_content);
}

#[test]
fn test_fuzzy_match_with_insertion_at_hunk_start() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file has an extra line at the beginning of the hunk's context.
    let original_content = "extra line\ncontext A\nline to change\ncontext C\n";
    fs::write(&file_path, original_content).unwrap();

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
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    let expected_content = "extra line\ncontext A\nline was changed\ncontext C\n";

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply cleanly with insertion at hunk start"
    );
    assert_eq!(content, expected_content);
}

#[test]
fn test_fuzzy_match_with_insertion_at_hunk_end() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file has an extra line at the end of the hunk's context.
    let original_content = "context A\nline to change\ncontext C\nextra line\n";
    fs::write(&file_path, original_content).unwrap();

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
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    let expected_content = "context A\nline was changed\ncontext C\nextra line\n";

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply cleanly with insertion at hunk end"
    );
    assert_eq!(content, expected_content);
}

#[test]
fn test_fuzzy_match_with_multiple_insertions() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let original_content = "context A\nextra line 1\nline to change\nextra line 2\ncontext C\n";
    fs::write(&file_path, original_content).unwrap();

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
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();
    let expected_content = "context A\nextra line 1\nline was changed\nextra line 2\ncontext C\n";

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply cleanly with multiple insertions"
    );
    assert_eq!(content, expected_content);
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
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply successfully despite an extra line in its context"
    );
    let content = fs::read_to_string(file_path).unwrap();
    // The logic should see that "line B" from the patch context is missing in the file,
    // and correctly apply the change to "line C" without re-inserting "line B".
    assert_eq!(content, "line A\nline changed\n");
}

#[test]
fn test_fuzzy_match_preserves_different_file_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file's context lines are different from the patch's.
    let original_content = "context in file (A)\nline to change\ncontext in file (C)\n";
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,3 +1,3 @@
         context in patch (A)
        -line to change
        +line was changed
         context in patch (C)
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply cleanly via fuzzy match"
    );
    let content = fs::read_to_string(file_path).unwrap();
    // The key assertion: the file's original context is preserved.
    let expected_content = "context in file (A)\nline was changed\ncontext in file (C)\n";
    assert_eq!(content, expected_content);
}

#[test]
fn test_fuzzy_match_with_multiple_differences_preserves_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    // The file has different context lines compared to the patch.
    let original_content = indoc! {"
        line A
        line B (in file)
        line C (to be changed)
        line D (in file)
        line E
    "};
    fs::write(&file_path, original_content).unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -1,4 +1,4 @@
         line A
         line B (in patch)
        -line C (to be changed)
        +line C (was changed)
         line D (in patch)
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "Patch should apply cleanly via fuzzy match"
    );
    let content = fs::read_to_string(file_path).unwrap();
    // The file's context (B and D) should be preserved, and the change to C should be applied.
    let expected_content = indoc! {"
        line A
        line B (in file)
        line C (was changed)
        line D (in file)
        line E
    "};
    assert_eq!(content, expected_content);
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
    let options = ApplyOptions::exact();
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
    let options = ApplyOptions::exact();
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
fn test_apply_options_convenience_constructors() {
    // Test ApplyOptions::new()
    let new_options = ApplyOptions::new();
    assert!(!new_options.dry_run);
    assert_eq!(new_options.fuzz_factor, 0.7);

    // Test ApplyOptions::dry_run()
    let dry_run_options = ApplyOptions::dry_run();
    assert!(dry_run_options.dry_run);
    assert_eq!(dry_run_options.fuzz_factor, 0.7);
}

#[test]
fn test_apply_options_fluent_methods() {
    let options = ApplyOptions::new().with_dry_run(true).with_fuzz_factor(0.9);

    assert!(options.dry_run);
    assert_eq!(options.fuzz_factor, 0.9);

    // Test that it returns a modified copy
    let options2 = options.with_dry_run(false);
    assert!(options.dry_run, "Original options should be unchanged");
    assert!(
        !options2.dry_run,
        "New options should have dry_run set to false"
    );
    assert_eq!(
        options2.fuzz_factor, 0.9,
        "Other fields should be preserved"
    );

    let options3 = options2.with_fuzz_factor(0.1);
    assert_eq!(
        options2.fuzz_factor, 0.9,
        "Original options should be unchanged"
    );
    assert_eq!(
        options3.fuzz_factor, 0.1,
        "New options should have new fuzz factor"
    );
    assert!(!options3.dry_run, "Other fields should be preserved");
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
    apply_patch_to_file(&patch, dir.path(), ApplyOptions::new()).unwrap();
    let content_after_patch = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content_after_patch, new_text);

    // Apply the inverted patch
    apply_patch_to_file(&inverted_patch, dir.path(), ApplyOptions::new()).unwrap();
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

    let batch_result = apply_patches_to_dir(&patches, dir.path(), ApplyOptions::new());

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
        let options = ApplyOptions::exact();
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
        let options = ApplyOptions::new();
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
            Err(HunkApplyError::FuzzyMatchBelowThreshold { .. })
        ));
    }

    #[test]
    fn test_default_finder_ambiguous_match() {
        let options = ApplyOptions::exact();
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

#[cfg(test)]
mod fuzzy_finder_diagnostics {
    use mpatch::{ApplyOptions, DefaultHunkFinder, Hunk, HunkFinder, HunkLocation, MatchType};

    #[test]
    fn test_apply_options_convenience_constructors() {
        // Test ApplyOptions::new()
        let new_options = ApplyOptions::new();
        assert!(!new_options.dry_run);
        assert_eq!(new_options.fuzz_factor, 0.7);

        // Test ApplyOptions::dry_run()
        let dry_run_options = ApplyOptions::dry_run();
        assert!(dry_run_options.dry_run);
        assert_eq!(dry_run_options.fuzz_factor, 0.7);

        // Test ApplyOptions::exact()
        let exact_options = ApplyOptions::exact();
        assert!(!exact_options.dry_run);
        assert_eq!(exact_options.fuzz_factor, 0.0);
    }

    /// Helper to test the DefaultHunkFinder's fuzzy logic.
    fn assert_fuzzy_location(
        hunk_match_block: &[&str],
        target_lines: &[&str],
        expected_location: HunkLocation,
        fuzz_factor: f32,
    ) {
        let options = ApplyOptions {
            fuzz_factor,
            ..Default::default()
        };
        let finder = DefaultHunkFinder::new(&options);

        // Create a dummy hunk. The only important part is the match block.
        let hunk = Hunk {
            lines: hunk_match_block.iter().map(|s| format!(" {}", s)).collect(), // Assume all context lines for simplicity
            old_start_line: Some(1),
            new_start_line: Some(1),
        };

        let result = finder.find_location(&hunk, &target_lines.iter().collect::<Vec<_>>());

        match result {
            Ok((location, match_type)) => {
                // We expect a fuzzy match, but if the content is very similar,
                // it might be classified as ExactIgnoringWhitespace. We accept both.
                assert!(
                    matches!(
                        match_type,
                        MatchType::Fuzzy { .. } | MatchType::ExactIgnoringWhitespace
                    ),
                    "Match was not fuzzy or whitespace-insensitive as expected. Was: {:?}",
                    match_type
                );
                assert_eq!(
                    location, expected_location,
                    "Fuzzy location did not match expectation"
                );
            }
            Err(e) => {
                panic!("Finder failed when a fuzzy match was expected: {:?}", e);
            }
        }
    }

    #[test]
    fn finder_single_insertion_middle() {
        // This reproduces the core logic failure from the failing tests.
        // The finder should select the larger window (len 3) that includes the insertion.
        assert_fuzzy_location(
            &["line A", "line C"],
            &["line A", "line B", "line C"],
            HunkLocation {
                start_index: 0,
                length: 3,
            },
            0.7,
        );
    }

    #[test]
    fn finder_single_insertion_start() {
        // This test previously expected a fuzzy match, but a perfect exact match exists.
        // The hierarchical search correctly finds the exact match at an offset and stops,
        // which is the desired behavior. The test is updated to reflect this.
        let options = ApplyOptions::new();
        let finder = DefaultHunkFinder::new(&options);

        let hunk = Hunk {
            lines: vec![" line A".to_string(), " line B".to_string()],
            old_start_line: Some(1),
            new_start_line: Some(1),
        };

        let target_lines = vec!["extra line", "line A", "line B"];

        let (location, match_type) = finder.find_location(&hunk, &target_lines).unwrap();

        assert!(
            matches!(match_type, MatchType::Exact),
            "Should have found an exact match, not {:?}",
            match_type
        );
        assert_eq!(
            location,
            HunkLocation {
                start_index: 1,
                length: 2,
            },
            "Exact match location is incorrect"
        );
    }

    #[test]
    fn finder_single_deletion_middle() {
        // The finder should select the smaller window (len 2) that reflects the deletion.
        assert_fuzzy_location(
            &["line A", "line B", "line C"],
            &["line A", "line C"],
            HunkLocation {
                start_index: 0,
                length: 2,
            },
            0.7,
        );
    }

    #[test]
    fn finder_multiple_insertions() {
        // This reproduces the other failing test case.
        // The score was just below the threshold. This test will fail if the scoring is too punitive.
        assert_fuzzy_location(
            &["context A", "line to change", "context C"],
            &[
                "context A",
                "extra line 1",
                "line to change",
                "extra line 2",
                "context C",
            ],
            HunkLocation {
                start_index: 0,
                length: 5,
            },
            0.7,
        );
    }

    #[test]
    fn finder_mixed_change_modification() {
        // Hunk expects "B", file has "X". Finder should still match the block.
        assert_fuzzy_location(
            &["A", "B", "C"],
            &["A", "X", "C"],
            HunkLocation {
                start_index: 0,
                length: 3,
            },
            0.7,
        );
    }
}

#[cfg(test)]
mod parse_single_patch_tests {
    use indoc::indoc;
    use mpatch::{parse_single_patch, SingleParseError};

    const SUCCESS_DIFF: &str = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         line 1
        -line 2
        +line two
         line 3
        ```
    "#};

    #[test]
    fn test_success_case() {
        let patch = parse_single_patch(SUCCESS_DIFF).unwrap();
        assert_eq!(patch.file_path.to_str(), Some("file.txt"));
        assert_eq!(patch.hunks.len(), 1);
    }

    #[test]
    fn test_err_no_patches_found() {
        let diff = "Just some text, no diff block.";
        let result = parse_single_patch(diff);
        assert!(matches!(result, Err(SingleParseError::NoPatchesFound)));
    }

    #[test]
    fn test_err_multiple_patches_in_one_block() {
        let diff = indoc! {r#"
            ```diff
            --- a/file1.txt
            +++ b/file1.txt
            @@ -1 +1 @@
            -a
            +b
            --- a/file2.txt
            +++ b/file2.txt
            @@ -1 +1 @@
            -c
            +d
            ```
        "#};
        let result = parse_single_patch(diff);
        assert!(matches!(
            result,
            Err(SingleParseError::MultiplePatchesFound(2))
        ));
    }

    #[test]
    fn test_err_multiple_patches_in_separate_blocks() {
        let diff = indoc! {r#"
            ```diff
            --- a/file1.txt
            +++ b/file1.txt
            @@ -1 +1 @@
            -a
            +b
            ```

            ```diff
            --- a/file2.txt
            +++ b/file2.txt
            @@ -1 +1 @@
            -c
            +d
            ```
        "#};
        let result = parse_single_patch(diff);
        assert!(matches!(
            result,
            Err(SingleParseError::MultiplePatchesFound(2))
        ));
    }

    #[test]
    fn test_err_parse_error_propagates() {
        let diff = indoc! {r#"
            ```diff
            @@ -1 +1 @@
            -a
            +b
            ```
        "#}; // Missing --- header
        let result = parse_single_patch(diff);
        // parse_diffs skips the block because it lacks a header, so we get NoPatchesFound
        assert!(matches!(result, Err(SingleParseError::NoPatchesFound)));
    }
}

#[test]
fn test_strict_apply_variants() {
    let original_content = "line 1\nline 2\nline 3\n\nline 5\nline 6\nline 7\n";
    let successful_diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         line 1
        -line 2
        +line two
         line 3
        ```
    "#};
    let partial_fail_diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
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

    let successful_patch = &parse_diffs(successful_diff).unwrap()[0];
    let failing_patch = &parse_diffs(partial_fail_diff).unwrap()[0];
    let success_options = ApplyOptions::new();

    // --- Test success cases ---
    let success_result_content =
        try_apply_patch_to_content(successful_patch, Some(original_content), &success_options)
            .unwrap();
    assert!(success_result_content.report.all_applied_cleanly());
    assert_eq!(
        success_result_content.new_content,
        "line 1\nline two\nline 3\n\nline 5\nline 6\nline 7\n"
    );

    // --- Test failure cases ---
    // Use options that will cause the hunk to fail, to test the `try_` function's error path.
    let failing_options = ApplyOptions::exact();

    // Test try_apply_patch_to_content
    let failure_result_content =
        try_apply_patch_to_content(failing_patch, Some(original_content), &failing_options);

    assert!(failure_result_content.is_err());
    if let Err(StrictApplyError::PartialApply { report }) = failure_result_content {
        assert!(!report.all_applied_cleanly());
        assert_eq!(report.failures().len(), 1);
        assert_eq!(report.failures()[0].hunk_index, 2);
    } else {
        panic!(
            "Expected PartialApply error, got {:?}",
            failure_result_content
        );
    }

    // Test try_apply_patch_to_lines
    let original_lines: Vec<_> = original_content.lines().collect();
    let failure_result_lines =
        try_apply_patch_to_lines(failing_patch, Some(&original_lines), &failing_options);
    assert!(matches!(
        failure_result_lines,
        Err(StrictApplyError::PartialApply { .. })
    ));

    // Test try_apply_patch_to_file
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, original_content).unwrap();
    let failure_result_file = try_apply_patch_to_file(failing_patch, dir.path(), failing_options);
    assert!(matches!(
        failure_result_file,
        Err(StrictApplyError::PartialApply { .. })
    ));
}

#[cfg(test)]
mod patch_content_str_tests {
    use super::*;
    use indoc::indoc;
    use mpatch::{patch_content_str, OneShotError, StrictApplyError};

    const ORIGINAL: &str = "line 1\nline 2\nline 3\n";
    const SUCCESS_DIFF: &str = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         line 1
        -line 2
        +line two
         line 3
        ```
    "#};
    const EXPECTED: &str = "line 1\nline two\nline 3\n";

    #[test]
    fn test_success_case() {
        let options = ApplyOptions::new();
        let new_content = patch_content_str(SUCCESS_DIFF, Some(ORIGINAL), &options).unwrap();
        assert_eq!(new_content, EXPECTED);
    }

    #[test]
    fn test_file_creation_success() {
        let creation_diff = indoc! {r#"
            ```diff
            --- a/new.txt
            +++ b/new.txt
            @@ -0,0 +1,2 @@
            +Hello
            +World
            ```
        "#};
        let options = ApplyOptions::new();
        let new_content = patch_content_str(creation_diff, None, &options).unwrap();
        assert_eq!(new_content, "Hello\nWorld\n");
    }

    #[test]
    fn test_err_no_patches_found() {
        let diff = "Just some text, no diff block.";
        let options = ApplyOptions::new();
        let result = patch_content_str(diff, Some(ORIGINAL), &options);
        assert!(matches!(result, Err(OneShotError::NoPatchesFound)));
    }

    #[test]
    fn test_err_multiple_patches_found() {
        let diff = indoc! {r#"
            ```diff
            --- a/file1.txt
            +++ b/file1.txt
            @@ -1 +1 @@
            -a
            +b
            --- a/file2.txt
            +++ b/file2.txt
            @@ -1 +1 @@
            -c
            +d
            ```
        "#};
        let options = ApplyOptions::new();
        let result = patch_content_str(diff, Some(ORIGINAL), &options);
        assert!(matches!(result, Err(OneShotError::MultiplePatchesFound(2))));
    }

    #[test]
    fn test_err_parse_error() {
        let diff = indoc! {r#"
            ```diff
            @@ -1 +1 @@
            -a
            +b
            ```
        "#}; // Missing --- header
        let options = ApplyOptions::new();
        let result = patch_content_str(diff, Some(ORIGINAL), &options);
        // parse_diffs skips the block, so we get NoPatchesFound
        assert!(matches!(result, Err(OneShotError::NoPatchesFound)));
    }

    #[test]
    fn test_err_apply_error() {
        let diff = indoc! {r#"
            ```diff
            --- a/file.txt
            +++ b/file.txt
            @@ -1,3 +1,3 @@
             line 1
            -WRONG CONTEXT
            +line two
             line 3
            ```
        "#};
        let options = ApplyOptions::exact();
        let result = patch_content_str(diff, Some(ORIGINAL), &options);
        assert!(matches!(result, Err(OneShotError::Apply(_))));
        if let Err(OneShotError::Apply(StrictApplyError::PartialApply { report })) = result {
            assert!(!report.all_applied_cleanly());
        } else {
            panic!("Expected a PartialApply error");
        }
    }
}

#[test]
fn test_patch_and_hunk_display_format() {
    // Test Case 1: Standard patch with multiple hunks
    let patch = Patch {
        file_path: "src/main.rs".into(),
        hunks: vec![
            Hunk {
                lines: vec![
                    " fn main() {".to_string(),
                    "-    println!(\"old\");".to_string(),
                    "+    println!(\"new\");".to_string(),
                    " }".to_string(),
                ],
                old_start_line: Some(1),
                new_start_line: Some(1),
            },
            Hunk {
                lines: vec![
                    " // some comment".to_string(),
                    "-// old comment".to_string(),
                    "+// new comment".to_string(),
                ],
                old_start_line: Some(10),
                new_start_line: Some(10),
            },
        ],
        ends_with_newline: true,
    };

    let expected_output = concat!(
        "--- a/src/main.rs\n",
        "+++ b/src/main.rs\n",
        "@@ -1,3 +1,3 @@\n",
        " fn main() {\n",
        "-    println!(\"old\");\n",
        "+    println!(\"new\");\n",
        " }\n",
        "@@ -10,2 +10,2 @@\n",
        " // some comment\n",
        "-// old comment\n",
        "+// new comment\n",
    );

    assert_eq!(
        patch.to_string(),
        expected_output,
        "Test for standard patch failed"
    );

    // Test Case 2: Patch with no newline at end
    let mut patch_no_newline = patch.clone();
    patch_no_newline.ends_with_newline = false;

    // The marker is appended after the final newline of the last hunk.
    let expected_output_no_newline =
        format!("{}{}", expected_output, "\\ No newline at end of file");

    assert_eq!(
        patch_no_newline.to_string(),
        expected_output_no_newline,
        "Test for patch with no newline failed"
    );

    // Test Case 3: Empty patch (no hunks)
    let empty_patch = Patch {
        file_path: "empty.txt".into(),
        hunks: vec![],
        ends_with_newline: true,
    };
    let expected_empty = "--- a/empty.txt\n+++ b/empty.txt\n";
    assert_eq!(
        empty_patch.to_string(),
        expected_empty,
        "Test for empty patch failed"
    );

    // Test Case 4: Empty patch with no newline at end
    let empty_patch_no_newline = Patch {
        file_path: "empty.txt".into(),
        hunks: vec![],
        ends_with_newline: false,
    };
    // The "No newline" marker should only appear if there are hunks.
    assert_eq!(
        empty_patch_no_newline.to_string(),
        expected_empty,
        "Test for empty patch with no newline failed"
    );

    // Test Case 5: Patch for file creation (addition-only hunk)
    let creation_patch = Patch {
        file_path: "new_file.txt".into(),
        hunks: vec![Hunk {
            lines: vec!["+line 1".to_string(), "+line 2".to_string()],
            old_start_line: Some(0),
            new_start_line: Some(1),
        }],
        ends_with_newline: true,
    };
    let expected_creation = concat!(
        "--- a/new_file.txt\n",
        "+++ b/new_file.txt\n",
        "@@ -0,0 +1,2 @@\n",
        "+line 1\n",
        "+line 2\n",
    );
    assert_eq!(
        creation_patch.to_string(),
        expected_creation,
        "Test for creation patch failed"
    );

    // Test Case 6: Direct Hunk Display
    let single_hunk = Hunk {
        lines: vec![
            " context".to_string(),
            "-deleted".to_string(),
            "+added".to_string(),
        ],
        old_start_line: Some(5),
        new_start_line: Some(5),
    };
    let expected_hunk_str = "@@ -5,2 +5,2 @@\n context\n-deleted\n+added\n";
    assert_eq!(
        single_hunk.to_string(),
        expected_hunk_str,
        "Test for direct hunk display failed"
    );
}

#[test]
fn test_apply_result_helpers() {
    use mpatch::{ApplyResult, HunkApplyError, HunkApplyStatus, HunkLocation, MatchType};

    // Case 1: All successful
    let all_success = ApplyResult {
        hunk_results: vec![
            HunkApplyStatus::Applied {
                location: HunkLocation {
                    start_index: 0,
                    length: 1,
                },
                match_type: MatchType::Exact,
                replaced_lines: vec![],
            },
            HunkApplyStatus::SkippedNoChanges,
        ],
    };
    assert!(all_success.all_applied_cleanly());
    assert!(!all_success.has_failures());
    assert_eq!(all_success.success_count(), 2);
    assert_eq!(all_success.failure_count(), 0);

    // Case 2: Mixed success and failure
    let mixed_result = ApplyResult {
        hunk_results: vec![
            HunkApplyStatus::Applied {
                location: HunkLocation {
                    start_index: 0,
                    length: 1,
                },
                match_type: MatchType::Exact,
                replaced_lines: vec![],
            },
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound),
            HunkApplyStatus::SkippedNoChanges,
            HunkApplyStatus::Failed(HunkApplyError::AmbiguousExactMatch(vec![])),
        ],
    };
    assert!(!mixed_result.all_applied_cleanly());
    assert!(mixed_result.has_failures());
    assert_eq!(mixed_result.success_count(), 2);
    assert_eq!(mixed_result.failure_count(), 2);

    // Case 3: All failures
    let all_failures = ApplyResult {
        hunk_results: vec![
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound),
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound),
        ],
    };
    assert!(!all_failures.all_applied_cleanly());
    assert!(all_failures.has_failures());
    assert_eq!(all_failures.success_count(), 0);
    assert_eq!(all_failures.failure_count(), 2);

    // Case 4: Empty result
    let empty_result = ApplyResult {
        hunk_results: vec![],
    };
    assert!(empty_result.all_applied_cleanly());
    assert!(!empty_result.has_failures());
    assert_eq!(empty_result.success_count(), 0);
    assert_eq!(empty_result.failure_count(), 0);
}

#[test]
fn test_parse_conflict_markers() {
    let diff = indoc! {r#"
        ```diff
        fn main() {
        <<<<
            println!("Old");
        ====
            println!("New");
        >>>>
        }
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let patch = &patches[0];

    // Conflict markers don't have file paths, so it defaults to "patch_target"
    assert_eq!(patch.file_path.to_str().unwrap(), "patch_target");

    let hunk = &patch.hunks[0];
    assert_eq!(hunk.context_lines(), vec!["fn main() {", "}"]);
    assert_eq!(hunk.removed_lines(), vec!["    println!(\"Old\");"]);
    assert_eq!(hunk.added_lines(), vec!["    println!(\"New\");"]);
}

#[test]
fn test_conflict_markers_git_style_labels() {
    // Git often adds labels like <<<<<<< HEAD or >>>>>>> branch-name
    let diff = indoc! {r#"
        ```diff
        <<<<<<< HEAD
        Current Code
        =======
        Incoming Code
        >>>>>>> feature/new-stuff
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let hunk = &patches[0].hunks[0];

    assert_eq!(hunk.removed_lines(), vec!["Current Code"]);
    assert_eq!(hunk.added_lines(), vec!["Incoming Code"]);
}

#[test]
fn test_conflict_markers_multiple_blocks_in_one_file() {
    // Conflict markers are parsed as a single large hunk containing context and changes
    let diff = indoc! {r#"
        ```diff
        Context Start
        <<<<
        Old 1
        ====
        New 1
        >>>>
        Middle Context
        <<<<
        Old 2
        ====
        New 2
        >>>>
        Context End
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let hunk = &patches[0].hunks[0];

    // It should capture the flow of the entire file
    let lines = &hunk.lines;
    assert!(lines.contains(&" Context Start".to_string()));
    assert!(lines.contains(&"-Old 1".to_string()));
    assert!(lines.contains(&"+New 1".to_string()));
    assert!(lines.contains(&" Middle Context".to_string()));
    assert!(lines.contains(&"-Old 2".to_string()));
    assert!(lines.contains(&"+New 2".to_string()));
    assert!(lines.contains(&" Context End".to_string()));
}

#[test]
fn test_conflict_markers_pure_addition() {
    let diff = indoc! {r#"
        ```diff
        <<<<
        ====
        New Line
        >>>>
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    assert!(hunk.removed_lines().is_empty());
    assert_eq!(hunk.added_lines(), vec!["New Line"]);
}

#[test]
fn test_conflict_markers_pure_deletion() {
    let diff = indoc! {r#"
        ```diff
        <<<<
        Old Line
        ====
        >>>>
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    assert_eq!(hunk.removed_lines(), vec!["Old Line"]);
    assert!(hunk.added_lines().is_empty());
}

#[test]
fn test_conflict_markers_apply_end_to_end() {
    // Use the high-level patch_content_str to verify it actually works
    let original = indoc! {r#"
        fn main() {
            let x = 1;
            println!("Old logic: {}", x);
            return;
        }
    "#};

    let diff = indoc! {r#"
        ```diff
        fn main() {
            let x = 1;
        <<<<
            println!("Old logic: {}", x);
        ====
            println!("New logic: {}", x + 1);
        >>>>
            return;
        }
        ```
    "#};

    let options = ApplyOptions::new();
    let result = patch_content_str(diff, Some(original), &options).unwrap();

    let expected = indoc! {r#"
        fn main() {
            let x = 1;
            println!("New logic: {}", x + 1);
            return;
        }
    "#};

    assert_eq!(result, expected);
}

#[test]
fn test_conflict_markers_ignore_normal_text() {
    // If a block doesn't contain markers, it shouldn't be parsed as a conflict patch.
    // Since it also doesn't look like a unified diff (no @@, ---, +++), standard parsing
    // returns Ok(empty).
    let diff = indoc! {r#"
        ```diff
        Just some random text
        that is not a diff
        and has no markers.
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_conflict_markers_indented() {
    let diff = indoc! {r#"
        ```diff
        fn main() {
            <<<<
            old_code();
            ====
            new_code();
            >>>>
        }
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];

    // Check that content was parsed correctly despite indentation of markers
    assert!(hunk.removed_lines()[0].contains("old_code"));
    assert!(hunk.added_lines()[0].contains("new_code"));
}

#[test]
fn test_conflict_markers_missing_separator() {
    // <<<< without ==== means pure deletion
    let diff = indoc! {r#"
        ```diff
        <<<<
        delete me
        >>>>
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];
    assert_eq!(hunk.removed_lines(), vec!["delete me"]);
    assert!(hunk.added_lines().is_empty());
}

#[test]
fn test_conflict_markers_missing_start() {
    // ==== without <<<< means pure addition (if we treat ==== as start of new)
    let diff = indoc! {r#"
        ```diff
        ====
        add me
        >>>>
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];
    assert!(hunk.removed_lines().is_empty());
    assert_eq!(hunk.added_lines(), vec!["add me"]);
}

#[test]
fn test_conflict_markers_unclosed() {
    // <<<< without >>>> (EOF implies end)
    let diff = indoc! {r#"
        ```diff
        <<<<
        delete me
        ====
        add me
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];
    assert_eq!(hunk.removed_lines(), vec!["delete me"]);
    assert_eq!(hunk.added_lines(), vec!["add me"]);
}

#[test]
fn test_conflict_markers_false_positive_check() {
    // Ensure `<<` operator isn't treated as marker
    let diff = indoc! {r#"
        ```diff
        fn main() {
            let x = 1 << 2;
        }
        ```
    "#};
    // This should NOT be parsed as a conflict marker patch because it has no markers (<<<< is 4 chars).
    // It also doesn't look like a unified diff.
    // So it should return an empty list of patches.
    let patches = parse_diffs(diff).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_conflict_markers_with_context() {
    let diff = indoc! {r#"
        ```diff
        context before
        <<<<
        old
        ====
        new
        >>>>
        context after
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];
    assert_eq!(hunk.lines[0], " context before");
    assert!(hunk.lines.contains(&"-old".to_string()));
    assert!(hunk.lines.contains(&"+new".to_string()));
    assert_eq!(hunk.lines.last().unwrap(), " context after");
}

#[test]
fn test_conflict_markers_malformed_sequence() {
    let diff = indoc! {r#"
        ```diff
        ====
        middle
        <<<<
        start
        >>>>
        end
        ```
    "#};
    // ==== -> New. "middle" -> +middle.
    // <<<< -> Old. "start" -> -start.
    // >>>> -> Context. "end" ->  end.
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];
    assert_eq!(hunk.added_lines(), vec!["middle"]);
    assert_eq!(hunk.removed_lines(), vec!["start"]);
}

#[test]
fn test_conflict_markers_in_comments_ignored() {
    let diff = indoc! {r#"
        ```diff
        // <<<< this is a comment
        old code
        // ====
        new code
        // >>>>
        ```
    "#};
    // Should be ignored (empty patches) because markers must be at start of line (ignoring whitespace)
    let patches = parse_diffs(diff).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_conflict_markers_with_trailing_text() {
    let diff = indoc! {r#"
        ```diff
        <<<< start of conflict
        old
        ==== middle of conflict
        new
        >>>> end of conflict
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];
    assert_eq!(hunk.removed_lines(), vec!["old"]);
    assert_eq!(hunk.added_lines(), vec!["new"]);
}

#[test]
fn test_conflict_markers_empty_block() {
    let diff = indoc! {r#"
        ```diff
        <<<<
        ====
        >>>>
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    let hunk = &patches[0].hunks[0];
    assert!(!hunk.has_changes());
}

#[test]
fn test_malformed_diff_returns_error_not_ignored() {
    // This looks like a diff (has @@) but is missing headers.
    // It should NOT be ignored, and should NOT be parsed as conflict markers.
    // It should return the standard parsing error.
    let diff = indoc! {r#"
        ```diff
        @@ -1 +1 @@
        -foo
        +bar
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    // With scan-all logic, blocks without headers are skipped/ignored
    assert!(patches.is_empty());
}

// --- Detection Tests ---

#[test]
fn test_detect_markdown_standard() {
    let content = indoc! {r#"
        Here is a change:
        ```diff
        --- a/file.rs
        +++ b/file.rs
        @@ -1 +1 @@
        -old
        +new
        ```
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Markdown);
}

#[test]
fn test_parse_closing_fence_longer_than_opening() {
    // Markdown spec allows closing fence to be longer than opening fence
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -a
        +b
        ````
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file.txt");
}

#[test]
fn test_parse_shorter_closing_fence_ignored() {
    // A fence shorter than the opening fence should be treated as content
    let diff = indoc! {r#"
        ````diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -a
        +b
        ```
        Still inside block
        ````
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    // The inner ``` should be part of the content, but our parser extracts the diff lines.
    // The key is that it didn't stop parsing at the ```.
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["b"]);
}

#[test]
fn test_parse_multiple_blocks_mixed_fences() {
    let diff = indoc! {r#"
        ```diff
        --- a/file1
        +++ b/file1
        @@ -1 +1 @@
        -a
        +b
        ```

        ````diff
        --- a/file2
        +++ b/file2
        @@ -1 +1 @@
        -c
        +d
        ````
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 2);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file1");
    assert_eq!(patches[1].file_path.to_str().unwrap(), "file2");
}

#[test]
fn test_parse_conflict_markers_variable_fence() {
    let diff = indoc! {r#"
        ````
        <<<<
        old
        ====
        new
        >>>>
        ````
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["new"]);
}

#[test]
fn test_parse_fence_trailing_whitespace() {
    // Fences with trailing whitespace should still be recognized
    let diff = "```diff   \n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n```   ";
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
}

#[test]
fn test_nested_diff_block_is_ignored() {
    let diff = indoc! {r#"
        ````
        Here is an example of a patch:
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -old
        +new
        ```
        ````
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert!(patches.is_empty(), "Nested diff block should be ignored");
}

#[test]
fn test_detect_markdown_patch_keyword() {
    let content = indoc! {r#"
        ```patch
        --- a/file
        +++ b/file
        ```
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Markdown);
}

#[test]
fn test_detect_markdown_with_language_hint() {
    let content = indoc! {r#"
        ```rust, diff
        --- a/file
        +++ b/file
        ```
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Markdown);
}

#[test]
fn test_detect_unified_git_header() {
    let content = indoc! {r#"
        diff --git a/src/main.rs b/src/main.rs
        index 88d9554..e0c99b6 100644
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1,3 +1,3 @@
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Unified);
}

#[test]
fn test_detect_unified_standard_headers() {
    let content = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -foo
        +bar
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Unified);
}

#[test]
fn test_detect_unified_hunk_only() {
    // Sometimes users paste just the hunk without file headers
    let content = indoc! {r#"
        @@ -10,4 +10,4 @@
         ctx
        -old
        +new
         ctx
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Unified);
}

#[test]
fn test_detect_conflict_markers_standard() {
    let content = indoc! {r#"
        <<<<
        old code
        ====
        new code
        >>>>
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Conflict);
}

#[test]
fn test_detect_conflict_markers_git_style() {
    let content = indoc! {r#"
        <<<<<<< HEAD
        current change
        =======
        incoming change
        >>>>>>> feature-branch
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Conflict);
}

#[test]
fn test_detect_conflict_markers_missing_middle() {
    // Pure deletion case in conflict markers
    let content = indoc! {r#"
        <<<<
        delete me
        >>>>
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Conflict);
}

#[test]
fn test_detect_conflict_markers_missing_end() {
    // EOF case
    let content = indoc! {r#"
        <<<<
        old
        ====
        new
    "#};
    // The logic requires start && (middle || end)
    assert_eq!(detect_patch(content), PatchFormat::Conflict);
}

// --- False Positive Tests ---

#[test]
fn test_detect_false_positive_bitwise_shift() {
    // Should not be detected as Conflict
    let content = "let x = 1 << 2;";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_false_positive_comparison() {
    // Should not be detected as Conflict
    let content = "if x <= y && a >= b {}";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_false_positive_list_item() {
    // Should not be detected as Unified
    let content = "--- this is just a list item";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_false_positive_hr() {
    // Horizontal rule in markdown
    let content = "---\n\n# Title";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_false_positive_plus_list() {
    // Should not be detected as Unified
    let content = "+++ Just a list item";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_unified_requires_plus_after_minus() {
    // "--- " must be followed by "+++ " on the next line to be detected as Unified via headers
    let content = "--- a/file\nnot a plus line";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

// --- Auto-Parsing Tests ---

#[test]
fn test_parse_auto_markdown() {
    let content = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -old
        +new
        ```
    "#};
    let patches = parse_auto(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file.txt");
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["new"]);
}

#[test]
fn test_parse_auto_raw_diff() {
    let content = indoc! {r#"
        --- a/raw.txt
        +++ b/raw.txt
        @@ -1 +1 @@
        -old
        +new
    "#};
    let patches = parse_auto(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "raw.txt");
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["new"]);
}

#[test]
fn test_parse_auto_conflict_markers() {
    let content = indoc! {r#"
        <<<<
        old
        ====
        new
        >>>>
    "#};
    let patches = parse_auto(content).unwrap();
    assert_eq!(patches.len(), 1);
    // Conflict markers default to "patch_target"
    assert_eq!(patches[0].file_path.to_str().unwrap(), "patch_target");
    assert_eq!(patches[0].hunks[0].removed_lines(), vec!["old"]);
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["new"]);
}

#[test]
fn test_parse_auto_fallback_to_raw() {
    // If detect_patch returns Unknown, parse_auto should try parsing as raw diff.
    // This is useful for fragments that might be missed by strict detection but accepted by the parser.
    // For example, a hunk without headers might be detected as Unified by `detect_patch` now,
    // but let's try a case that might slip through or relies on the fallback.

    // A diff that is just a header without hunks (technically valid parse result = empty)
    let content = "--- a/file\n+++ b/file";
    // detect_patch sees this as Unified.
    assert_eq!(detect_patch(content), PatchFormat::Unified);

    // Let's try something that `detect_patch` misses but `parse_patches` might handle?
    // Actually, `detect_patch` is designed to cover the requirements of `parse_patches`.
    // The fallback is mostly for safety.

    // Case: Unknown format that is NOT a patch
    let content = "Just random text";
    let result = parse_auto(content).unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_patch_content_str_accepts_raw_diff() {
    // This verifies that the high-level helper now accepts raw diffs due to the refactor
    let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -old
        +new
    "#};
    let original = "old\n";
    let options = ApplyOptions::new();

    let result = patch_content_str(diff, Some(original), &options).unwrap();
    assert_eq!(result, "new\n");
}

#[test]
fn test_patch_content_str_accepts_markdown() {
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -old
        +new
        ```
    "#};
    let original = "old\n";
    let options = ApplyOptions::new();

    let result = patch_content_str(diff, Some(original), &options).unwrap();
    assert_eq!(result, "new\n");
}

#[test]
fn test_parse_auto_multiple_raw_patches() {
    let content = indoc! {r#"
        --- a/file1.txt
        +++ b/file1.txt
        @@ -1 +1 @@
        -a
        +b
        --- a/file2.txt
        +++ b/file2.txt
        @@ -1 +1 @@
        -c
        +d
    "#};
    let patches = parse_auto(content).unwrap();
    assert_eq!(patches.len(), 2);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file1.txt");
    assert_eq!(patches[1].file_path.to_str().unwrap(), "file2.txt");
}

#[test]
fn test_cli_simulation_raw_diff_input() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("raw.txt");
    fs::write(&file_path, "old content\n").unwrap();

    let patch_content = indoc! {r#"
        --- a/raw.txt
        +++ b/raw.txt
        @@ -1 +1 @@
        -old content
        +new content
    "#};

    // Verify parse_auto detects and parses it correctly
    let patches = parse_auto(patch_content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "raw.txt");

    // Verify application
    let options = ApplyOptions::new();
    let result = apply_patches_to_dir(&patches, dir.path(), options);
    assert!(result.all_succeeded());

    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "new content\n");
}

#[test]
fn test_cli_simulation_conflict_marker_input() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    // Conflict markers default to "patch_target"
    let file_path = dir.path().join("patch_target");
    fs::write(&file_path, "line 1\nold\nline 3\n").unwrap();

    let patch_content = indoc! {r#"
        <<<<
        old
        ====
        new
        >>>>
    "#};

    // Verify parse_auto detects and parses it correctly
    let patches = parse_auto(patch_content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "patch_target");

    // Verify application
    let options = ApplyOptions::new();
    let result = apply_patches_to_dir(&patches, dir.path(), options);
    assert!(result.all_succeeded());

    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "line 1\nnew\nline 3\n");
}

#[test]
fn test_cli_simulation_markdown_input() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("md.txt");
    fs::write(&file_path, "old\n").unwrap();

    let patch_content = indoc! {r#"
        Here is a fix:
        ```diff
        --- a/md.txt
        +++ b/md.txt
        @@ -1 +1 @@
        -old
        +new
        ```
    "#};

    // Verify parse_auto detects and parses it correctly
    let patches = parse_auto(patch_content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str().unwrap(), "md.txt");

    // Verify application
    let options = ApplyOptions::new();
    let result = apply_patches_to_dir(&patches, dir.path(), options);
    assert!(result.all_succeeded());

    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "new\n");
}

#[test]
fn test_patch_from_texts_uses_raw_parser() {
    // This test verifies that Patch::from_texts works correctly with the optimized
    // raw parser implementation (parse_patches) instead of wrapping in markdown.
    let old_text = "line 1\nline 2\n";
    let new_text = "line 1\nline modified\n";

    let patch = Patch::from_texts("test.txt", old_text, new_text, 3).unwrap();

    assert_eq!(patch.file_path.to_str(), Some("test.txt"));
    assert_eq!(patch.hunks.len(), 1);
    assert_eq!(patch.hunks[0].removed_lines(), vec!["line 2"]);
    assert_eq!(patch.hunks[0].added_lines(), vec!["line modified"]);
}

mod fuzzy_logic_edge_cases {
    use indoc::indoc;
    use mpatch::{apply_patch_to_file, parse_auto, parse_diffs, ApplyOptions};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_fuzzy_insertion_clobbers_context() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");

        // The file has a comment that has been modified locally ("modified").
        let original_content = indoc! {r#"
            fn main() {
                // comment (modified)
                println!("Hello");
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        // The patch expects the comment to be "(original)" and wants to INSERT a line.
        let diff = indoc! {r#"
            ```diff
            --- a/main.rs
            +++ b/main.rs
            @@ -1,3 +1,4 @@
             fn main() {
                 // comment (original)
            +    let x = 1;
                 println!("Hello");
             }
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let patch = &patches[0];

        // Enable fuzzy matching so it matches despite the comment difference.
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        let content = fs::read_to_string(&file_path).unwrap();

        // EXPECTED: The local modification "(modified)" should be preserved.
        let expected_content = indoc! {r#"
            fn main() {
                // comment (modified)
                let x = 1;
                println!("Hello");
            }
        "#};

        assert_eq!(
            content, expected_content,
            "Fuzzy insertion clobbered the local file context!"
        );
    }

    #[test]
    fn test_fuzzy_interleaved_local_edits() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.toml");

        // Local file has an extra line inserted in the middle of the context
        let original_content = indoc! {r#"
            [server]
            host = "localhost"
            # Local comment
            port = 8080
        "#};
        fs::write(&file_path, original_content).unwrap();

        // Patch wants to change port to 9090, unaware of the local comment
        let diff = indoc! {r#"
            ```diff
            --- a/config.toml
            +++ b/config.toml
            @@ -1,3 +1,3 @@
             [server]
             host = "localhost"
            -port = 8080
            +port = 9090
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        let content = fs::read_to_string(&file_path).unwrap();
        let expected = indoc! {r#"
            [server]
            host = "localhost"
            # Local comment
            port = 9090
        "#};
        assert_eq!(content, expected);
    }

    #[test]
    fn test_fuzzy_indentation_context_preserved() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("style.css");

        // File uses tabs
        let original_content = "body {\n\tcolor: red;\n\tbackground: white;\n}\n";
        fs::write(&file_path, original_content).unwrap();

        // Patch uses spaces
        let diff = indoc! {r#"
            ```diff
            --- a/style.css
            +++ b/style.css
            @@ -1,4 +1,4 @@
             body {
                 color: red;
            -    background: white;
            +    background: black;
             }
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        let content = fs::read_to_string(&file_path).unwrap();
        // Context lines (body {, color: red;, }) should keep tabs.
        // The changed line comes from patch, so it will have spaces.
        let expected = "body {\n\tcolor: red;\n    background: black;\n}\n";
        assert_eq!(content, expected);
    }

    #[test]
    fn test_fuzzy_extra_newlines_in_target() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("list.txt");

        let original_content = "item 1\n\nitem 2\n\nitem 3\n";
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/list.txt
            +++ b/list.txt
            @@ -1,3 +1,3 @@
             item 1
            -item 2
            +item two
             item 3
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        // The extra newlines should be preserved as local insertions
        let content = fs::read_to_string(&file_path).unwrap();
        let expected = "item 1\n\nitem two\n\nitem 3\n";
        assert_eq!(content, expected);
    }

    #[test]
    fn test_fuzzy_restore_truncated_context_at_eof() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("truncated.rs");

        // File is missing the closing brace
        let original_content = "fn main() {\n    println!(\"hi\");\n";
        fs::write(&file_path, original_content).unwrap();

        // Patch expects the brace to be there and adds a line after it
        let diff = indoc! {r#"
            ```diff
            --- a/truncated.rs
            +++ b/truncated.rs
            @@ -1,3 +1,4 @@
             fn main() {
                 println!("hi");
             }
            +// end
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        // The missing context "}" should be restored because it's at EOF
        let content = fs::read_to_string(&file_path).unwrap();
        let expected = "fn main() {\n    println!(\"hi\");\n}\n// end\n";
        assert_eq!(content, expected);
    }

    #[test]
    fn test_fuzzy_skip_stale_context_middle() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("stale.txt");

        // File is missing "line B" which is in patch context
        let original_content = "line A\nline C\n";
        fs::write(&file_path, original_content).unwrap();

        // Patch has "line B" in context. Since it's not at EOF, it should be treated as stale and skipped.
        let diff = indoc! {r#"
            ```diff
            --- a/stale.txt
            +++ b/stale.txt
            @@ -1,4 +1,4 @@
             line A
             line B
            -line C
            +line changed
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        // "line B" should NOT be re-inserted.
        let content = fs::read_to_string(&file_path).unwrap();
        let expected = "line A\nline changed\n";
        assert_eq!(content, expected);
    }

    #[test]
    fn test_conflict_markers_adjacent() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("patch_target");

        let original_content = "block1\nblock2\n";
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            <<<<
            block1
            ====
            new1
            >>>>
            <<<<
            block2
            ====
            new2
            >>>>
        "#};

        let patches = parse_auto(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let content = fs::read_to_string(&file_path).unwrap();
        let expected = "new1\nnew2\n";
        assert_eq!(content, expected);
    }

    #[test]
    fn test_hunks_out_of_order() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("order.txt");

        let original_content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/order.txt
            +++ b/order.txt
            @@ -5,1 +5,1 @@
            -line 5
            +line five
            @@ -1,1 +1,1 @@
            -line 1
            +line one
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::exact();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let content = fs::read_to_string(&file_path).unwrap();
        let expected = "line one\nline 2\nline 3\nline 4\nline five\n";
        assert_eq!(content, expected);
    }

    #[test]
    fn test_large_offset_application() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("offset.txt");

        // File has content shifted by 100 lines compared to patch expectation
        let mut content = String::new();
        for _ in 0..100 {
            content.push_str("prefix\n");
        }
        content.push_str("target\n");
        fs::write(&file_path, &content).unwrap();

        // Patch expects "target" at line 1
        let diff = indoc! {r#"
            ```diff
            --- a/offset.txt
            +++ b/offset.txt
            @@ -1,1 +1,1 @@
            -target
            +hit
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::exact(); // Exact match should still find it by scanning
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let file_content = fs::read_to_string(&file_path).unwrap();
        assert!(file_content.ends_with("hit\n"));
    }
}

mod extended_stress_tests {
    use indoc::indoc;
    use mpatch::{apply_patch_to_file, parse_auto, ApplyOptions};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_fuzzy_crlf_mismatch() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("crlf.txt");
        // File uses CRLF
        fs::write(&file_path, "line1\r\nline2\r\nline3\r\n").unwrap();

        // Patch uses LF
        let diff =
            "--- a/crlf.txt\n+++ b/crlf.txt\n@@ -1,3 +1,3 @@\n line1\n-line2\n+line two\n line3\n";

        let patches = parse_auto(diff).unwrap();
        // Exact match should work because mpatch normalizes line endings during parsing/reading
        let options = ApplyOptions::exact();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let content = fs::read_to_string(&file_path).unwrap();
        // Output is normalized to LF by mpatch
        assert_eq!(content, "line1\nline two\nline3\n");
    }

    #[test]
    fn test_fuzzy_unicode_context() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("unicode.txt");
        // File has "Hello 🌍" (Europe-Africa)
        fs::write(&file_path, "Start\nHello 🌍\nEnd\n").unwrap();

        // Patch expects "Hello 🌎" (Americas) and changes it to "Hello 🌏" (Asia-Australia)
        let diff = indoc! {r#"
            --- a/unicode.txt
            +++ b/unicode.txt
            @@ -1,3 +1,3 @@
             Start
            -Hello 🌎
            +Hello 🌏
             End
        "#};

        let patches = parse_auto(diff).unwrap();
        let options = ApplyOptions::new(); // Fuzzy
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Start\nHello 🌏\nEnd\n");
    }

    #[test]
    fn test_fuzzy_repeated_lines_ambiguity_resolution() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("repeat.txt");

        // A file with repeating patterns
        let content = "A\nB\nC\n\nA\nB\nC\n\nA\nB\nC\n";
        fs::write(&file_path, content).unwrap();

        // Patch targets the middle block (line 5)
        // But context is slightly different in patch ("B" -> "B modified") to force fuzzy
        let diff = indoc! {r#"
            --- a/repeat.txt
            +++ b/repeat.txt
            @@ -5,3 +5,3 @@
             A
            -B modified
            +B changed
             C
        "#};

        let patches = parse_auto(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        let new_content = fs::read_to_string(&file_path).unwrap();
        // Should change the middle block
        let expected = "A\nB\nC\n\nA\nB changed\nC\n\nA\nB\nC\n";
        assert_eq!(new_content, expected);
    }

    #[test]
    fn test_apply_patch_with_huge_offset() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("offset.txt");

        let mut content = String::new();
        for _ in 0..500 {
            content.push_str("noise\n");
        }
        content.push_str("target\n");
        for _ in 0..500 {
            content.push_str("noise\n");
        }

        fs::write(&file_path, &content).unwrap();

        // Patch expects target at line 1
        let diff = indoc! {r#"
            --- a/offset.txt
            +++ b/offset.txt
            @@ -1,1 +1,1 @@
            -target
            +hit
        "#};

        let patches = parse_auto(diff).unwrap();
        let options = ApplyOptions::exact();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let new_content = fs::read_to_string(&file_path).unwrap();
        assert!(new_content.contains("hit\n"));
        assert!(!new_content.contains("target\n"));
    }

    #[test]
    fn test_parse_auto_mixed_formats() {
        // A file containing both a markdown block and raw conflict markers
        let content = indoc! {r#"
            Some text
            ```diff
            --- a/file1.txt
            +++ b/file1.txt
            @@ -1 +1 @@
            -a
            +b
            ```
            
            <<<<
            old
            ====
            new
            >>>>
        "#};

        // parse_auto detects format. It prioritizes Markdown if code blocks are present.
        // It should parse the markdown block and ignore the outer conflict markers.

        let patches = parse_auto(content).unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].file_path.to_str().unwrap(), "file1.txt");
    }
}

#[test]
fn test_fuzzy_insertion_clobbers_context() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("main.rs");

    // The file has a comment that has been modified locally ("modified").
    let original_content = indoc! {r#"
        fn main() {
            // comment (modified)
            println!("Hello");
        }
    "#};
    fs::write(&file_path, original_content).unwrap();

    // The patch expects the comment to be "(original)" and wants to INSERT a line.
    let diff = indoc! {r#"
        ```diff
        --- a/main.rs
        +++ b/main.rs
        @@ -1,3 +1,4 @@
         fn main() {
             // comment (original)
        +    let x = 1;
             println!("Hello");
         }
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let patch = &patches[0];

    // Enable fuzzy matching so it matches despite the comment difference.
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());

    let content = fs::read_to_string(&file_path).unwrap();

    // EXPECTED: The local modification "(modified)" should be preserved.
    // ACTUAL: It is overwritten by "(original)" from the patch context.
    let expected_content = indoc! {r#"
        fn main() {
            // comment (modified)
            let x = 1;
            println!("Hello");
        }
    "#};

    assert_eq!(
        content, expected_content,
        "Fuzzy insertion clobbered the local file context!"
    );
}

#[test]
fn test_git_diff_header_is_not_absorbed_into_previous_hunk() {
    let diff = indoc! {r#"
        --- a/file1.txt
        +++ b/file1.txt
        @@ -1,1 +1,1 @@
        -foo
        +bar
        diff --git a/file2.txt b/file2.txt
        index 1234567..89abcdef 100644
        --- a/file2.txt
        +++ b/file2.txt
        @@ -1,1 +1,1 @@
        -baz
        +qux
    "#};

    let patches = parse_patches(diff).unwrap();
    assert_eq!(patches.len(), 2);

    let hunk1 = &patches[0].hunks[0];
    assert_eq!(
        hunk1.lines.len(),
        2,
        "Hunk 1 absorbed git headers as context lines! Lines: {:?}",
        hunk1.lines
    );
    assert!(!hunk1.lines.iter().any(|l| l.contains("diff --git")));
    assert!(!hunk1.lines.iter().any(|l| l.contains("index")));
}

#[test]
fn test_new_file_mode_header_is_not_context() {
    let diff = indoc! {r#"
        --- a/src/lib.rs
        +++ b/src/lib.rs
        @@ -5,1 +5,1 @@
         fn existing() {}
        diff --git a/tests/new_test.rs b/tests/new_test.rs
        new file mode 100644
        index 0000000..1234567
        --- /dev/null
        +++ b/tests/new_test.rs
        @@ -0,0 +1 @@
        +#[test] fn t() {}
    "#};

    let patches = parse_patches(diff).unwrap();
    assert_eq!(patches.len(), 2);

    let hunk1 = &patches[0].hunks[0];
    assert_eq!(
        hunk1.lines.len(),
        1,
        "Hunk 1 absorbed new file headers! Lines: {:?}",
        hunk1.lines
    );
    assert_eq!(hunk1.lines[0], " fn existing() {}");
}

#[test]
fn test_deleted_file_mode_header_is_not_context() {
    let diff = indoc! {r#"
        --- a/keep.txt
        +++ b/keep.txt
        @@ -1 +1 @@
         keep
        diff --git a/delete.txt b/delete.txt
        deleted file mode 100644
        index 1234567..0000000
        --- a/delete.txt
        +++ /dev/null
        @@ -1 +0,0 @@
        -content
    "#};

    let patches = parse_patches(diff).unwrap();
    assert_eq!(patches.len(), 2);

    let hunk1 = &patches[0].hunks[0];
    assert_eq!(
        hunk1.lines.len(),
        1,
        "Hunk 1 absorbed deleted file headers! Lines: {:?}",
        hunk1.lines
    );
    assert_eq!(hunk1.lines[0], " keep");
}

#[test]
fn test_markdown_block_with_git_headers() {
    // Ensure the issue is also reproduced when parsing markdown blocks,
    // as this uses the same underlying line parser.
    let content = indoc! {r#"
        ```diff
        --- a/f1
        +++ b/f1
        @@ -1 +1 @@
        -a
        +b
        diff --git a/f2 b/f2
        index 111..222
        --- a/f2
        +++ b/f2
        @@ -1 +1 @@
        -c
        +d
        ```
    "#};

    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 2);
    let hunk1 = &patches[0].hunks[0];
    assert_eq!(
        hunk1.lines.len(),
        2,
        "Markdown parser absorbed git headers into hunk"
    );
}

#[test]
fn test_extended_git_headers() {
    // Test other git headers like similarity index, rename, etc.
    let diff = indoc! {r#"
        --- a/f1
        +++ b/f1
        @@ -1 +1 @@
         context
        diff --git a/old b/new
        similarity index 100%
        rename from old
        rename to new
        --- a/old
        +++ b/new
        @@ -1 +1 @@
         context
    "#};

    let patches = parse_patches(diff).unwrap();
    assert_eq!(patches.len(), 2);
    let hunk1 = &patches[0].hunks[0];
    assert_eq!(
        hunk1.lines.len(),
        1,
        "Hunk absorbed rename/similarity headers"
    );
    assert_eq!(hunk1.lines[0], " context");
}
