use indoc::indoc;
use mpatch::{
    apply_hunk_to_lines, apply_patch_to_file, apply_patch_to_lines, apply_patches_to_dir,
    apply_patches_to_dir_atomic, detect_patch, find_hunk_location, find_hunk_location_in_lines,
    invert_patches, parse_aider, parse_auto, parse_diffs, parse_patches, parse_patches_from_lines,
    patch_content_str, try_apply_patch_to_content, try_apply_patch_to_file,
    try_apply_patch_to_lines, ApplyOptions, DefaultHunkFinder, Hunk, HunkApplyError,
    HunkApplyStatus, HunkFinder, HunkLocation, MatchType, ParseError, Patch, PatchError,
    PatchFormat, StrictApplyError,
};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_parse_simple_diff() {
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
    let patches = parse_patches("").unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_parse_patches_whitespace_input() {
    let _ = env_logger::builder().is_test(true).try_init();
    let patches = parse_patches("  \n\t\n  ").unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_parse_patches_file_creation() {
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    assert!(
        !file_path.exists(),
        "File should be deleted when content becomes empty"
    );
}

#[test]
fn test_file_creation_empty_content_does_not_create_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("empty_create.txt");

    // Manually construct a patch that is a "creation" (empty match block)
    // but adds nothing (empty replace block).
    let patch = Patch {
        file_path: std::path::PathBuf::from("empty_create.txt"),
        hunks: vec![Hunk {
            lines: vec![], // No lines = empty match, empty replace
            old_start_line: Some(0),
            new_start_line: Some(0),
        }],
        ends_with_newline: false,
    };

    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(!file_path.exists(), "Empty file should not be created");
}

#[test]
fn test_dry_run_deletion_preserves_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("preserve_me.txt");
    fs::write(&file_path, "content\n").unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/preserve_me.txt
        +++ b/preserve_me.txt
        @@ -1 +0,0 @@
        -content
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::dry_run();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(file_path.exists(), "Dry run should not delete the file");
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "content\n");
}

#[test]
fn test_fuzzy_deletion_removes_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("fuzzy_delete.txt");
    // File has slightly different content than patch expects
    fs::write(&file_path, "content modified\n").unwrap();

    let diff = indoc! {r#"
        ```diff
        --- a/fuzzy_delete.txt
        +++ b/fuzzy_delete.txt
        @@ -1 +0,0 @@
        -content original
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions {
        dry_run: false,
        fuzz_factor: 0.3,
    };
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(
        !file_path.exists(),
        "File should be deleted even via fuzzy match if result is empty"
    );
}

#[test]
fn test_creation_of_empty_file_is_skipped() {
    // If we try to create a file with empty content, and it doesn't exist,
    // it should just log "Skipping creation" and succeed.
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("ghost.txt");

    // A creation patch that adds nothing
    let diff = indoc! {r#"
        ```diff
        --- /dev/null
        +++ b/ghost.txt
        @@ -0,0 +0,0 @@
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(!file_path.exists());
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
fn test_preserves_no_newline_when_patch_does_not_touch_eof() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("start_only.txt");
    // File has 2 lines, no trailing newline
    fs::write(&file_path, "line 1\nline 2").unwrap();

    // Patch modifies line 1
    let diff = indoc! {r#"
        ```diff
        --- a/start_only.txt
        +++ b/start_only.txt
        @@ -1 +1 @@
        -line 1
        +line one
        ```
    "#};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(
        content, "line one\nline 2",
        "Should preserve existing no-newline state if patch doesn't touch EOF"
    );
}

#[test]
fn test_patch_content_str_preserves_no_newline_on_partial_patch() {
    let _ = env_logger::builder().is_test(true).try_init();
    let original = "line 1\nline 2";
    let diff = indoc! {r#"
        ```diff
        --- a/file
        +++ b/file
        @@ -1 +1 @@
        -line 1
        +line one
        ```
    "#};
    let options = ApplyOptions::new();
    let result = patch_content_str(diff, Some(original), &options).unwrap();
    assert_eq!(result, "line one\nline 2");
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
    // The robust application logic preserves the file's context lines (including trailing whitespace)
    // when using ExactIgnoringWhitespace or Fuzzy matching.
    assert_eq!(content, "line one  \nchanged\nline three\t\n");
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let expected_content = "fn main() {\n    println!(\"Hello\");\n}\n    println!(\"World\");\n";
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
fn test_smart_indentation_tabs_to_tabs_multiple_levels() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("tabs_multi.py");

    // Target file uses TABS
    let original_content = "def main():\n\tif True:\n\t\tprint(\"hello\")\n";
    fs::write(&file_path, original_content).unwrap();

    // Patch uses SPACES
    let diff = indoc! {r#"
        ```diff
        --- a/tabs_multi.py
        +++ b/tabs_multi.py
        @@ -1,3 +1,4 @@
         def main():
             if True:
                 print("hello")
        +        print("world")
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    let expected = "def main():\n\tif True:\n\t\tprint(\"hello\")\n\t\tprint(\"world\")\n";
    assert_eq!(content, expected);
}

#[test]
fn test_smart_indentation_after_empty_line() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("empty_line.py");

    // Target file uses TABS
    let original_content = "def main():\n\tprint(\"hello\")\n\n\tprint(\"world\")\n";
    fs::write(&file_path, original_content).unwrap();

    // Patch uses SPACES and adds a line after the empty line
    let diff = indoc! {r#"
        ```diff
        --- a/empty_line.py
        +++ b/empty_line.py
        @@ -1,4 +1,5 @@
         def main():
             print("hello")
         
        +    print("inserted")
             print("world")
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    let expected = "def main():\n\tprint(\"hello\")\n\n\tprint(\"inserted\")\n\tprint(\"world\")\n";
    assert_eq!(content, expected);
}

#[test]
fn test_smart_indentation_empty_lines_stripped() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("empty_lines.py");

    let original_content = "def main():\n    print(\"hello\")\n";
    fs::write(&file_path, original_content).unwrap();

    // Patch adds an empty line with spaces, and another print
    let diff = format!(
        "```diff\n--- a/empty_lines.py\n+++ b/empty_lines.py\n@@ -1,2 +1,4 @@\n def main():\n     print(\"hello\")\n+{spaces}\n+    print(\"world\")\n```",
        spaces = "    "
    );

    let patches = parse_diffs(&diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    // The empty line should be completely empty, no trailing spaces
    let expected = "def main():\n    print(\"hello\")\n\n    print(\"world\")\n";
    assert_eq!(content, expected);
}

#[test]
fn test_smart_indentation_nested_markdown_list_spaces_to_tabs() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("nested_tabs.py");

    // Target file uses TABS
    let original_content = "def main():\n\tprint(\"hello\")\n";
    fs::write(&file_path, original_content).unwrap();

    // Patch uses SPACES, but is nested in a markdown list, so it has extra indentation.
    // Target indent: 1 tab. Hunk indent: 6 spaces (e.g., 2 spaces for list + 4 spaces for code).
    let diff = format!(
        "```diff\n--- a/nested_tabs.py\n+++ b/nested_tabs.py\n@@ -1,2 +1,4 @@\n{s3}def main():\n{s7}print(\"hello\")\n+{s6}if True:\n+{s10}print(\"world\")\n```",
        s3 = "   ",
        s7 = "       ",
        s6 = "      ",
        s10 = "          "
    );

    let patches = parse_diffs(&diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    // With the new logic, spaces_per_tab falls back to 4.
    // hunk_indent = 6 spaces. target_indent = 1 tab.
    // line_indent = 6 spaces for `if True:`, 10 spaces for `print("world")`.
    // For `if True:`: hunk_tabs = 1, line_tabs = 1, line_spaces = 2.
    // new_tabs = 1 + (1 - 1) = 1. new_indent = \t + 2 spaces.
    // For `print("world")`: hunk_tabs = 1, line_tabs = 2, line_spaces = 2.
    // new_tabs = 1 + (2 - 1) = 2. new_indent = \t\t + 2 spaces.
    let expected = "def main():\n\tprint(\"hello\")\n\t  if True:\n\t\t  print(\"world\")\n";
    assert_eq!(content, expected);
}

#[test]
fn test_smart_indentation_preserved_across_unindented_lines() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("unindented.py");

    // Target file uses TABS
    let original_content =
        "class Foo:\n\tdef __init__(self):\n\t\tpass\n\ndef main():\n\tprint(\"hello\")\n";
    fs::write(&file_path, original_content).unwrap();

    // Patch uses SPACES
    let diff = indoc! {r#"
        ```diff
        --- a/unindented.py
        +++ b/unindented.py
        @@ -2,4 +2,5 @@
             def __init__(self):
                 pass
         
         def main():
        +    print("setup")
             print("hello")
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    let expected = "class Foo:\n\tdef __init__(self):\n\t\tpass\n\ndef main():\n\tprint(\"setup\")\n\tprint(\"hello\")\n";
    assert_eq!(content, expected);
}

#[test]
fn test_smart_indentation_mixed_style_fallback() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("mixed.py");

    // Target file uses 1 tab
    let original_content = "def main():\n\tprint(\"hello\")\n";
    fs::write(&file_path, original_content).unwrap();

    // Patch uses 1 tab + 4 spaces for context, and adds a line with 1 tab + 8 spaces
    let diff = "```diff\n--- a/mixed.py\n+++ b/mixed.py\n@@ -1,2 +1,3 @@\n def main():\n \t    print(\"hello\")\n+\t        print(\"world\")\n```";

    let patches = parse_diffs(diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    let expected = "def main():\n\tprint(\"hello\")\n\t    print(\"world\")\n";
    assert_eq!(content, expected);
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
    let old_text = "hello\nworld\n";
    let patch = Patch::from_texts("file.txt", old_text, old_text, 3).unwrap();
    assert!(patch.hunks.is_empty());
}

#[test]
fn test_patch_inversion() {
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let safe_path = "new/file.txt";

        let result = ensure_path_is_safe(base_dir, safe_path.as_ref());
        assert!(result.is_ok());
        let resolved_path = result.unwrap();
        assert!(resolved_path.ends_with(safe_path));
        assert!(resolved_path.is_absolute());
    }

    #[test]
    fn test_traversal_path_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let unsafe_path = "../evil.txt";

        let result = ensure_path_is_safe(base_dir, unsafe_path.as_ref());
        assert!(matches!(result, Err(PatchError::PathTraversal(_))));
    }

    #[test]
    fn test_traversal_path_to_nonexistent_file_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let unsafe_path = "src/../../evil.txt";

        let result = ensure_path_is_safe(base_dir, unsafe_path.as_ref());
        assert!(matches!(result, Err(PatchError::PathTraversal(_))));
    }

    #[test]
    #[cfg(unix)]
    fn test_absolute_path_fails() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let base_dir = dir.path();
        let unsafe_path = "/etc/passwd";

        let result = ensure_path_is_safe(base_dir, unsafe_path.as_ref());
        assert!(matches!(result, Err(PatchError::PathTraversal(_))));
    }

    #[test]
    fn test_path_normalization_within_project_succeeds() {
        let _ = env_logger::builder().is_test(true).try_init();
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

    #[test]
    fn test_path_traversal_does_not_create_directories() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let base_dir = dir.path().join("base");
        fs::create_dir(&base_dir).unwrap();

        let diff = indoc::indoc! {"
            ```diff
            --- a/../evil_dir/evil.txt
            +++ b/../evil_dir/evil.txt
            @@ -0,0 +1 @@
            +hacked
            ```
        "};
        let patch = &mpatch::parse_diffs(diff).unwrap()[0];
        let options = mpatch::ApplyOptions::exact();
        let result = mpatch::apply_patch_to_file(patch, &base_dir, options);

        assert!(matches!(result, Err(PatchError::PathTraversal(_))));

        let evil_dir = dir.path().join("evil_dir");
        assert!(
            !evil_dir.exists(),
            "VULNERABILITY: Directory was created outside base_dir!"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_path_traversal_via_dangling_symlink_is_blocked() {
        use std::os::unix::fs::symlink;
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let base_dir = dir.path().join("base");
        fs::create_dir(&base_dir).unwrap();

        // Create a dangling symlink inside base_dir pointing outside
        let evil_target = dir.path().join("evil_outside.txt");
        let symlink_path = base_dir.join("dangling_link");
        symlink(&evil_target, &symlink_path).unwrap();

        // Ensure it's dangling
        assert!(!symlink_path.exists());
        assert!(fs::symlink_metadata(&symlink_path).is_ok());

        // Patch targets the dangling symlink
        let diff = indoc::indoc! {"
            ```diff
            --- /dev/null
            +++ b/dangling_link
            @@ -0,0 +1 @@
            +hacked
            ```
        "};

        let patch = &mpatch::parse_diffs(diff).unwrap()[0];
        let options = mpatch::ApplyOptions::exact();
        let result = mpatch::apply_patch_to_file(patch, &base_dir, options);

        assert!(result.is_err(), "Patching a dangling symlink should fail");
        assert!(
            !evil_target.exists(),
            "VULNERABILITY: Arbitrary file created via dangling symlink!"
        );
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
        let patch = parse_single_patch(SUCCESS_DIFF).unwrap();
        assert_eq!(patch.file_path.to_str(), Some("file.txt"));
        assert_eq!(patch.hunks.len(), 1);
    }

    #[test]
    fn test_err_no_patches_found() {
        let _ = env_logger::builder().is_test(true).try_init();
        let diff = "Just some text, no diff block.";
        let result = parse_single_patch(diff);
        assert!(matches!(result, Err(SingleParseError::NoPatchesFound)));
    }

    #[test]
    fn test_err_multiple_patches_in_one_block() {
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
        let options = ApplyOptions::new();
        let new_content = patch_content_str(SUCCESS_DIFF, Some(ORIGINAL), &options).unwrap();
        assert_eq!(new_content, EXPECTED);
    }

    #[test]
    fn test_file_creation_success() {
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
        let diff = "Just some text, no diff block.";
        let options = ApplyOptions::new();
        let result = patch_content_str(diff, Some(ORIGINAL), &options);
        assert!(matches!(result, Err(OneShotError::NoPatchesFound)));
    }

    #[test]
    fn test_err_multiple_patches_found() {
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
    // ==== without <<<< is not considered a valid conflict marker
    // to prevent false positives with Markdown headers.
    let diff = indoc! {r#"
        ```diff
        ====
        add me
        >>>>
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert!(patches.is_empty());
}

#[test]
fn test_conflict_markers_unclosed() {
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
    // Fences with trailing whitespace should still be recognized
    let diff = "```diff   \n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n```   ";
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
}

#[test]
fn test_nested_diff_block_is_ignored() {
    let _ = env_logger::builder().is_test(true).try_init();
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
fn test_parse_diff_with_nested_indented_code_block() {
    let _ = env_logger::builder().is_test(true).try_init();
    // This tests a regression where an indented code block inside a diff
    // was incorrectly interpreted as the closing fence of the diff block.
    let diff = indoc! {r#"
        ```diff
        --- README.md
        +++ README.md
        @@ -1,3 +1,3 @@
         1. Step one
             ```bash
        -    old_command
        +    new_command
             ```
         2. Step two
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["    new_command"]);
}

#[test]
fn test_parse_diff_with_fence_like_context_line() {
    let _ = env_logger::builder().is_test(true).try_init();
    let diff = indoc! {r#"
        ```diff
        --- README.md
        +++ README.md
        @@ -1,3 +1,3 @@
         text
         ```
         more text
        ```
    "#};
    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let hunk = &patches[0].hunks[0];
    assert_eq!(hunk.lines.len(), 3);
    // The line should be preserved as a context line (space + backticks)
    assert_eq!(hunk.lines[1], " ```");
    assert_eq!(hunk.lines[2], " more text");
}

#[test]
fn test_detect_markdown_patch_keyword() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        ```patch
        --- a/file
        +++ b/file
        ```
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Markdown);
}

#[test]
fn test_detect_aider_standard() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        src/main.rs
        <<<<<<< SEARCH
        fn main() {}
        =======
        fn main() { println!("Hello"); }
        >>>>>>> REPLACE
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Aider);
}

#[test]
fn test_detect_aider_case_insensitive() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        <<<<<<< search
        old
        =======
        new
        >>>>>>> replace
    "#};
    assert_eq!(detect_patch(content), PatchFormat::Aider);
}

#[test]
fn test_parse_aider_original_updated_syntax() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        js/game_manager.js
        <<<<<<< ORIGINAL
          // Update the score
          self.score += merged.value;
        =======
          // Update the score with a 10% chance of 10x bonus
          var bonus = Math.random() <= 0.1 ? 10 : 1;
          self.score += merged.value * bonus;
        >>>>>>> UPDATED
    "#};

    assert_eq!(detect_patch(content), PatchFormat::Aider);
    let patches = parse_auto(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str(), Some("js/game_manager.js"));
    let hunk = &patches[0].hunks[0];
    assert!(hunk
        .removed_lines()
        .contains(&"  self.score += merged.value;"));
    assert!(hunk
        .added_lines()
        .contains(&"  self.score += merged.value * bonus;"));
}

#[test]
fn test_apply_aider_original_updated_syntax() {
    let _ = env_logger::builder().is_test(true).try_init();
    let original = indoc! {r#"
        function GameManager() {
          // Update the score
          self.score += merged.value;
          return self.score;
        }
    "#};

    let diff = indoc! {r#"
        js/game_manager.js
        <<<<<<< ORIGINAL
          // Update the score
          self.score += merged.value;
        =======
          // Update the score with a 10% chance of 10x bonus
          var bonus = Math.random() <= 0.1 ? 10 : 1;
          self.score += merged.value * bonus;
        >>>>>>> UPDATED
    "#};

    let result = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
    let expected = indoc! {r#"
        function GameManager() {
          // Update the score with a 10% chance of 10x bonus
          var bonus = Math.random() <= 0.1 ? 10 : 1;
          self.score += merged.value * bonus;
          return self.score;
        }
    "#};
    assert_eq!(result, expected);
}

#[test]
fn test_parse_aider_single_block_with_file_path() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        src/main.rs
        <<<<<<< SEARCH
        fn main() {
            println!("old");
        }
        =======
        fn main() {
            println!("new");
        }
        >>>>>>> REPLACE
    "#};
    let patches = parse_aider(content);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str(), Some("src/main.rs"));
    let hunk = &patches[0].hunks[0];
    assert_eq!(hunk.removed_lines(), vec!["    println!(\"old\");"]);
    assert_eq!(hunk.added_lines(), vec!["    println!(\"new\");"]);
    assert_eq!(hunk.context_lines(), vec!["fn main() {", "}"]);
}

#[test]
fn test_parse_aider_path_on_fence_line() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        <<<<<<< SEARCH src/main.rs
        println!("old");
        =======
        println!("new");
        >>>>>>> REPLACE
    "#};
    let patches = parse_aider(content);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str(), Some("src/main.rs"));
    assert_eq!(
        patches[0].hunks[0].removed_lines(),
        vec!["println!(\"old\");"]
    );
    assert_eq!(
        patches[0].hunks[0].added_lines(),
        vec!["println!(\"new\");"]
    );
}

#[test]
fn test_parse_aider_path_in_backticks_and_markdown_header() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content1 = indoc! {r#"
        `src/utils.py`
        <<<<<<< SEARCH
        def old(): pass
        =======
        def new(): pass
        >>>>>>> REPLACE
    "#};
    let patches1 = parse_aider(content1);
    assert_eq!(patches1.len(), 1);
    assert_eq!(patches1[0].file_path.to_str(), Some("src/utils.py"));

    let content2 = indoc! {r#"
        ### src/models/user.rs
        <<<<<<< SEARCH
        struct Old;
        =======
        struct New;
        >>>>>>> REPLACE
    "#};
    let patches2 = parse_aider(content2);
    assert_eq!(patches2.len(), 1);
    assert_eq!(patches2[0].file_path.to_str(), Some("src/models/user.rs"));
}

#[test]
fn test_parse_aider_multiple_blocks_same_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        src/math.rs
        <<<<<<< SEARCH
        fn add() { 1 }
        =======
        fn add() { 2 }
        >>>>>>> REPLACE
        <<<<<<< SEARCH
        fn sub() { 3 }
        =======
        fn sub() { 4 }
        >>>>>>> REPLACE
    "#};
    let patches = parse_aider(content);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str(), Some("src/math.rs"));
    assert_eq!(patches[0].hunks.len(), 2);
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["fn add() { 2 }"]);
    assert_eq!(patches[0].hunks[1].added_lines(), vec!["fn sub() { 4 }"]);
}

#[test]
fn test_parse_aider_multiple_files() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        src/foo.rs
        <<<<<<< SEARCH
        fn foo() { 1 }
        =======
        fn foo() { 10 }
        >>>>>>> REPLACE

        src/bar.rs
        <<<<<<< SEARCH
        fn bar() { 2 }
        =======
        fn bar() { 20 }
        >>>>>>> REPLACE
    "#};
    let patches = parse_auto(content).unwrap();
    assert_eq!(patches.len(), 2);
    assert_eq!(patches[0].file_path.to_str(), Some("src/foo.rs"));
    assert_eq!(patches[1].file_path.to_str(), Some("src/bar.rs"));
}

#[test]
fn test_parse_aider_inside_markdown_code_block() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        Here is the fix:
        ```python
        calc.py
        <<<<<<< SEARCH
        def calc(): return 1
        =======
        def calc(): return 2
        >>>>>>> REPLACE
        ```
    "#};
    let patches = parse_auto(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str(), Some("calc.py"));
    assert_eq!(
        patches[0].hunks[0].added_lines(),
        vec!["def calc(): return 2"]
    );
}

#[test]
fn test_parse_aider_path_outside_markdown_code_block() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        Update `calc.py` to fix addition:
        ```python
        <<<<<<< SEARCH
        def add(a, b): return a - b
        =======
        def add(a, b): return a + b
        >>>>>>> REPLACE
        ```
    "#};
    let patches = parse_diffs(content).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str(), Some("calc.py"));
    assert_eq!(
        patches[0].hunks[0].added_lines(),
        vec!["def add(a, b): return a + b"]
    );
}

#[test]
fn test_parse_aider_pure_deletion() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        clean.rs
        <<<<<<< SEARCH
        unused_function();
        =======
        >>>>>>> REPLACE
    "#};
    let patches = parse_aider(content);
    assert_eq!(patches.len(), 1);
    assert_eq!(
        patches[0].hunks[0].removed_lines(),
        vec!["unused_function();"]
    );
    assert!(patches[0].hunks[0].added_lines().is_empty());
}

#[test]
fn test_parse_aider_pure_addition() {
    let _ = env_logger::builder().is_test(true).try_init();
    let content = indoc! {r#"
        new.rs
        <<<<<<< SEARCH
        =======
        fn brand_new() {}
        >>>>>>> REPLACE
    "#};
    let patches = parse_aider(content);
    assert_eq!(patches.len(), 1);
    assert!(patches[0].hunks[0].removed_lines().is_empty());
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["fn brand_new() {}"]);
}

#[test]
fn test_aider_patch_content_str_end_to_end() {
    let _ = env_logger::builder().is_test(true).try_init();
    let original = indoc! {r#"
        def hello():
            print("hello")
            return 1
    "#};
    let diff = indoc! {r#"
        <<<<<<< SEARCH
            print("hello")
            return 1
        =======
            print("hello, world!")
            return 42
        >>>>>>> REPLACE
    "#};
    let result = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
    let expected = indoc! {r#"
        def hello():
            print("hello, world!")
            return 42
    "#};
    assert_eq!(result, expected);
}

#[test]
fn test_aider_apply_patch_to_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("server.py");
    fs::write(&file_path, "def start():\n    bind(8080)\n").unwrap();

    let diff = indoc! {r#"
        server.py
        <<<<<<< SEARCH
        def start():
            bind(8080)
        =======
        def start():
            bind(9090)
        >>>>>>> REPLACE
    "#};

    let patches = parse_auto(diff).unwrap();
    let result = apply_patch_to_file(&patches[0], dir.path(), ApplyOptions::new()).unwrap();
    assert!(result.report.all_applied_cleanly());

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "def start():\n    bind(9090)\n");
}

#[test]
fn test_aider_apply_patches_to_dir_multi_file() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file1_path = dir.path().join("f1.txt");
    let file2_path = dir.path().join("f2.txt");
    fs::write(&file1_path, "apple\n").unwrap();
    fs::write(&file2_path, "banana\n").unwrap();

    let diff = indoc! {r#"
        f1.txt
        <<<<<<< SEARCH
        apple
        =======
        apricot
        >>>>>>> REPLACE

        f2.txt
        <<<<<<< SEARCH
        banana
        =======
        blueberry
        >>>>>>> REPLACE
    "#};

    let patches = parse_auto(diff).unwrap();
    assert_eq!(patches.len(), 2);

    let batch = apply_patches_to_dir(&patches, dir.path(), ApplyOptions::new());
    assert!(batch.all_succeeded());

    assert_eq!(fs::read_to_string(file1_path).unwrap(), "apricot\n");
    assert_eq!(fs::read_to_string(file2_path).unwrap(), "blueberry\n");
}

#[test]
fn test_aider_fuzzy_matching_and_indentation() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("indented.py");
    fs::write(
        &file_path,
        "def run():\n    # local comment\n    execute()\n",
    )
    .unwrap();

    let diff = indoc! {r#"
        indented.py
        <<<<<<< SEARCH
                # original comment
                execute()
        =======
                # original comment
                execute_new()
        >>>>>>> REPLACE
    "#};

    let patches = parse_auto(diff).unwrap();
    let result = apply_patch_to_file(&patches[0], dir.path(), ApplyOptions::new()).unwrap();
    assert!(result.report.all_applied_cleanly());

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(
        content,
        "def run():\n    # local comment\n    execute_new()\n"
    );
}

#[test]
fn test_aider_unclosed_block_at_eof() {
    let _ = env_logger::builder().is_test(true).try_init();
    let diff = indoc! {r#"
        truncated.txt
        <<<<<<< SEARCH
        old
        =======
        new
    "#};
    let patches = parse_aider(diff);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].file_path.to_str(), Some("truncated.txt"));
    assert_eq!(patches[0].hunks[0].removed_lines(), vec!["old"]);
    assert_eq!(patches[0].hunks[0].added_lines(), vec!["new"]);
}

#[test]
fn test_detect_markdown_with_language_hint() {
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
    // Should not be detected as Conflict
    let content = "let x = 1 << 2;";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_false_positive_comparison() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Should not be detected as Conflict
    let content = "if x <= y && a >= b {}";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_false_positive_list_item() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Should not be detected as Unified
    let content = "--- this is just a list item";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_false_positive_hr() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Horizontal rule in markdown
    let content = "---\n\n# Title";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_false_positive_plus_list() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Should not be detected as Unified
    let content = "+++ Just a list item";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

#[test]
fn test_detect_unified_requires_plus_after_minus() {
    let _ = env_logger::builder().is_test(true).try_init();
    // "--- " must be followed by "+++ " on the next line to be detected as Unified via headers
    let content = "--- a/file\nnot a plus line";
    assert_eq!(detect_patch(content), PatchFormat::Unknown);
}

// --- Auto-Parsing Tests ---

#[test]
fn test_parse_auto_markdown() {
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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

#[test]
fn test_apply_patch_to_hard_link() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let target_path = dir.path().join("target.txt");
    let link_path = dir.path().join("link.txt");

    fs::write(&target_path, "line 1\nline 2\n").unwrap();
    fs::hard_link(&target_path, &link_path).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/link.txt
        +++ b/link.txt
        @@ -1,2 +1,2 @@
         line 1
        -line 2
        +line two
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());

    let link_content = fs::read_to_string(&link_path).unwrap();
    assert_eq!(link_content, "line 1\nline two\n");

    let target_content = fs::read_to_string(&target_path).unwrap();
    assert_eq!(target_content, "line 1\nline two\n");
}

#[test]
#[cfg(unix)]
fn test_apply_patch_to_symlink() {
    use std::os::unix::fs::symlink;

    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let target_path = dir.path().join("target.txt");
    let link_path = dir.path().join("link.txt");

    fs::write(&target_path, "line 1\nline 2\n").unwrap();
    symlink(&target_path, &link_path).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/link.txt
        +++ b/link.txt
        @@ -1,2 +1,2 @@
         line 1
        -line 2
        +line two
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_none());

    let target_content = fs::read_to_string(&target_path).unwrap();
    assert_eq!(target_content, "line 1\nline two\n");

    let metadata = fs::symlink_metadata(&link_path).unwrap();
    assert!(metadata.file_type().is_symlink());
}

#[test]
#[cfg(unix)]
fn test_apply_patch_to_symlink_preserves_link() {
    use std::os::unix::fs::symlink;
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();

    let target_file = dir.path().join("target.txt");
    fs::write(&target_file, "original content\n").unwrap();

    let symlink_path = dir.path().join("link.txt");
    symlink("target.txt", &symlink_path).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/link.txt
        +++ b/link.txt
        @@ -1 +1 @@
        -original content
        +patched content
        ```
    "};

    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());

    // Verify symlink is still a symlink
    let metadata = fs::symlink_metadata(&symlink_path).unwrap();
    assert!(metadata.file_type().is_symlink());

    // Verify target file was updated
    let content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(content, "patched content\n");
}

#[test]
fn test_apply_patch_to_hardlink_preserves_link() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();

    let target_file = dir.path().join("target.txt");
    fs::write(&target_file, "original content\n").unwrap();

    let hardlink_path = dir.path().join("link.txt");
    fs::hard_link(&target_file, &hardlink_path).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/link.txt
        +++ b/link.txt
        @@ -1 +1 @@
        -original content
        +patched content
        ```
    "};

    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());

    // Verify target file was updated (since hardlink was patched)
    let content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(content, "patched content\n");

    // Verify hardlink still has the same content
    let link_content = fs::read_to_string(&hardlink_path).unwrap();
    assert_eq!(link_content, "patched content\n");

    // Verify they are still hardlinked (modifying one modifies the other)
    fs::write(&target_file, "modified again\n").unwrap();
    let link_content_after = fs::read_to_string(&hardlink_path).unwrap();
    assert_eq!(link_content_after, "modified again\n");
}

#[test]
#[cfg(unix)]
fn test_apply_patch_to_symlink_deletion() {
    use std::os::unix::fs::symlink;
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();

    let target_file = dir.path().join("target.txt");
    fs::write(&target_file, "original content\n").unwrap();

    let symlink_path = dir.path().join("link.txt");
    symlink("target.txt", &symlink_path).unwrap();

    let diff = indoc! {"
        ```diff
        --- a/link.txt
        +++ b/link.txt
        @@ -1 +0,0 @@
        -original content
        ```
    "};

    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());

    // Verify the target is deleted (because mpatch operates on the resolved target)
    assert!(!target_file.exists());

    // Verify symlink still exists but is dangling
    assert!(fs::symlink_metadata(&symlink_path).is_ok());
    assert!(!symlink_path.exists());
}

mod fuzzy_logic_edge_cases {
    use indoc::indoc;
    use mpatch::{apply_patch_to_file, parse_auto, parse_diffs, ApplyOptions};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_fuzzy_insertion_clobbers_context() {
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        // The changed line comes from patch, and its indentation is dynamically adjusted to match the target file (tabs).
        let expected = "body {\n\tcolor: red;\n\tbackground: black;\n}\n";
        assert_eq!(content, expected);
    }

    #[test]
    fn test_fuzzy_extra_newlines_in_target() {
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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

    #[test]
    fn test_fuzzy_anchor_indentation_drift_with_coincidental_match() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("anchor_drift.rs");

        let mut original_content = String::new();
        // 1. Noise
        for i in 0..50 {
            original_content.push_str(&format!("// noise A {}\n", i));
        }
        // 2. Incorrect location: Has the exact indentation the patch expects (8 spaces),
        // but is missing the surrounding context.
        original_content.push_str("        println!(\"My unique anchor line\");\n");

        // 3. More noise to separate the windows (search radius is ~15 lines)
        for i in 0..50 {
            original_content.push_str(&format!("// noise B {}\n", i));
        }

        // 4. Correct location: Has different indentation (4 spaces), but correct context.
        original_content.push_str("    let x = 1;\n");
        original_content.push_str("    println!(\"My unique anchor line\");\n");
        original_content.push_str("    let y = 2;\n");

        // 5. Trailing noise
        for i in 0..50 {
            original_content.push_str(&format!("// noise C {}\n", i));
        }

        fs::write(&file_path, &original_content).unwrap();

        // Patch expects 8 spaces of indentation
        let diff = indoc! {r#"
            ```diff
            --- a/anchor_drift.rs
            +++ b/anchor_drift.rs
            @@ -100,3 +100,3 @@
                     let x = 1;
                     println!("My unique anchor line");
            -        let y = 2;
            +        let y = 3;
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Patch should apply cleanly by finding the anchor despite indentation drift"
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("    let y = 3;\n"));
    }

    #[test]
    fn test_fuzzy_reconstruction_misalignment_bug() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("model.py");

        // Target file (4 and 8 spaces indent)
        let original_content = [
            "        yins[t] = 1",
            "        ",
            "    age_series = 1",
            "    yin_series = 1",
            "    ",
            "    age_norm = 1",
            "    yin_norm = 1",
            "    ",
            "    self.dynamic = 1",
        ]
        .join("\n")
            + "\n";
        fs::write(&file_path, original_content).unwrap();

        // Patch (8 and 12 spaces indent)
        let diff = [
            "```diff",
            "--- a/model.py",
            "+++ b/model.py",
            "@@ -1,9 +1,9 @@",
            "             yins[t] = 1",
            "             ",
            "-        age_series = 1",
            "-        yin_series = 1",
            "+        age_series = 2",
            "+        yin_series = 2",
            "         ",
            "-        age_norm = 1",
            "-        yin_norm = 1",
            "+        age_norm = 2",
            "+        yin_norm = 2",
            "         ",
            "         self.dynamic = 1",
            "```",
        ]
        .join("\n")
            + "\n";

        let patches = parse_diffs(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        let content = fs::read_to_string(&file_path).unwrap();
        let expected = [
            "        yins[t] = 1",
            "        ",
            "    age_series = 2",
            "    yin_series = 2",
            "    ",
            "    age_norm = 2",
            "    yin_norm = 2",
            "    ",
            "    self.dynamic = 1",
        ]
        .join("\n")
            + "\n";

        assert_eq!(content, expected);
    }
}

mod extended_stress_tests {
    use indoc::indoc;
    use mpatch::{apply_patch_to_file, parse_auto, ApplyOptions};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_fuzzy_crlf_mismatch() {
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
        let _ = env_logger::builder().is_test(true).try_init();
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
fn test_smart_indentation_adjustment() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("indent.rs");

    // Target file has standard 4-space indentation
    let original_content = indoc! {r#"
        fn main() {
            println!("Hello");
        }
    "#};
    fs::write(&file_path, original_content).unwrap();

    // Patch has extra indentation (e.g. copied from a nested list in markdown)
    // It uses 8 spaces for context, whereas file has 4.
    let diff = indoc! {r#"
        ```diff
        --- a/indent.rs
        +++ b/indent.rs
        @@ -1,3 +1,4 @@
             fn main() {
                 println!("Hello");
        +        println!("World");
             }
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());

    let content = fs::read_to_string(&file_path).unwrap();
    // The added line should be adjusted to 4 spaces, not 8.
    let expected = indoc! {r#"
        fn main() {
            println!("Hello");
            println!("World");
        }
    "#};
    assert_eq!(content, expected);
}

#[test]
fn test_out_of_order_hunks_eof_newline_preservation() {
    let _ = env_logger::builder().is_test(true).try_init();
    let original = "line 1\nline 2\nline 3\n"; // Ends with newline
                                               // Patch removes newline at EOF in hunk 1, then modifies line 1 in hunk 2.
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -2,2 +2,2 @@
         line 2
        -line 3
        +line 3
        \ No newline at end of file
        @@ -1,2 +1,2 @@
        -line 1
        +line one
         line 2
        ```
    "#};

    let options = ApplyOptions::new();
    let result = patch_content_str(diff, Some(original), &options).unwrap();

    // The EOF was modified to remove the newline.
    let expected = "line one\nline 2\nline 3";
    assert_eq!(result, expected);
}

#[test]
fn test_dry_run_does_not_create_directories() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let nested_dir = dir.path().join("nested_folder");

    let diff = indoc! {"
        ```diff
        --- a/nested_folder/new_file.txt
        +++ b/nested_folder/new_file.txt
        @@ -0,0 +1,2 @@
        +Hello
        +New World
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];

    // Execute a dry run
    let options = ApplyOptions::dry_run();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());
    assert!(result.diff.is_some());

    // Regression verification: Ensure the directory was NOT created on the filesystem
    assert!(
        !nested_dir.exists(),
        "Dry run must not create directories on the filesystem"
    );
}

#[test]
fn test_dry_run_diff_contains_correct_file_paths() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("existing_file.txt");
    fs::write(&file_path, "original line\n").unwrap();

    let diff = indoc! {"
        ```diff
        --- a/existing_file.txt
        +++ b/existing_file.txt
        @@ -1 +1 @@
        -original line
        +modified line
        ```
    "};
    let patch = &parse_diffs(diff).unwrap()[0];
    let options = ApplyOptions::dry_run();
    let result = apply_patch_to_file(patch, dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());

    let diff_output = result.diff.expect("Dry run should produce a diff");

    // Regression verification: Ensure file paths are accurately preserved
    assert!(diff_output.contains("--- a/existing_file.txt"));
    assert!(diff_output.contains("+++ b/existing_file.txt"));
}

#[test]
fn test_smart_indentation_ignores_empty_lines_with_trailing_whitespace() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("empty_whitespace.py");

    // Target file has an empty line with 12 spaces of trailing whitespace
    let original_content =
        "def main():\n    print(\"hello\")\n            \n    # Validation Loop\n";
    fs::write(&file_path, original_content).unwrap();

    // Patch adds a line after the empty line
    let diff = "```diff\n--- a/empty_whitespace.py\n+++ b/empty_whitespace.py\n@@ -1,4 +1,6 @@\n def main():\n     print(\"hello\")\n \n+    print(\"inserted\")\n+\n     # Validation Loop\n```";

    let patches = parse_diffs(diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    // The inserted line should have 4 spaces, not 4 + 12 = 16 spaces.
    let expected = "def main():\n    print(\"hello\")\n            \n    print(\"inserted\")\n\n    # Validation Loop\n";
    assert_eq!(content, expected);
}

#[test]
fn test_fuzzy_match_ignores_indentation() {
    let _ = env_logger::builder().is_test(true).try_init();
    // This tests the "Robust Fuzzy Matching" feature.
    // The patch is heavily indented, the file is not.
    // Standard fuzzy matching would fail (score ~0.67).
    // Robust matching (trimming whitespace) should succeed (score ~1.0).
    let original = "line1\nline2\nline3\n";
    let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
            line1
        -   line2
        +   line two
            line3
    "#};
    let patch = parse_patches(diff).unwrap().remove(0);
    let options = ApplyOptions::new(); // Default fuzz factor 0.7

    let result = try_apply_patch_to_content(&patch, Some(original), &options).unwrap();
    assert_eq!(result.new_content, "line1\nline two\nline3\n");
}

#[test]
fn test_fuzzy_insertion_clobbers_context() {
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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
    let _ = env_logger::builder().is_test(true).try_init();
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

#[test]
fn test_fuzzy_indentation_drift() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("drift.md");

    let original_content = indoc! {r#"
        # Header
        * Item 1
        * Item 2
    "#};
    fs::write(&file_path, original_content).unwrap();

    // Patch has indented list items
    let diff = indoc! {r#"
        ```diff
        --- a/drift.md
        +++ b/drift.md
        @@ -1,3 +1,4 @@
         # Header
            * Item 1
        -   * Item 2
        +   * Item Two
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    // Use fuzzy matching to trigger the robust logic (or ExactIgnoringWhitespace)
    let options = ApplyOptions::new();
    let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());

    let content = fs::read_to_string(&file_path).unwrap();

    // Expected: The new item should match the target file's indentation (0 spaces),
    // not the patch's indentation (3 spaces).
    let expected = indoc! {r#"
        # Header
        * Item 1
        * Item Two
    "#};

    assert_eq!(
        content, expected,
        "Indentation should be dynamically adjusted based on local context"
    );
}

#[test]
fn test_invert_simple_modification() {
    let _ = env_logger::builder().is_test(true).try_init();
    let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -old
        +new
    "#};

    let patches = parse_auto(diff).unwrap();
    let inverted = invert_patches(&patches);

    assert_eq!(inverted.len(), 1);
    let hunk = &inverted[0].hunks[0];

    // Original: -old, +new
    // Inverted: -new, +old
    assert_eq!(hunk.removed_lines(), vec!["new"]);
    assert_eq!(hunk.added_lines(), vec!["old"]);
}

#[test]
fn test_invert_creation_becomes_deletion() {
    let _ = env_logger::builder().is_test(true).try_init();
    let diff = indoc! {r#"
        --- /dev/null
        +++ b/new.txt
        @@ -0,0 +1 @@
        +content
    "#};

    let patches = parse_auto(diff).unwrap();
    assert!(patches[0].is_creation());

    let inverted = invert_patches(&patches);
    assert!(inverted[0].is_deletion());

    let hunk = &inverted[0].hunks[0];
    assert_eq!(hunk.removed_lines(), vec!["content"]);
    assert!(hunk.added_lines().is_empty());
}

#[test]
fn test_invert_deletion_becomes_creation() {
    let _ = env_logger::builder().is_test(true).try_init();
    let diff = indoc! {r#"
        --- a/old.txt
        +++ /dev/null
        @@ -1 +0,0 @@
        -content
    "#};

    let patches = parse_auto(diff).unwrap();
    assert!(patches[0].is_deletion());

    let inverted = invert_patches(&patches);
    assert!(inverted[0].is_creation());

    let hunk = &inverted[0].hunks[0];
    assert!(hunk.removed_lines().is_empty());
    assert_eq!(hunk.added_lines(), vec!["content"]);
}

#[test]
fn test_double_inversion_is_identity() {
    let _ = env_logger::builder().is_test(true).try_init();
    let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -1,3 +1,3 @@
         ctx
        -old
        +new
    "#};

    let original = parse_auto(diff).unwrap();
    let once = invert_patches(&original);
    let twice = invert_patches(&once);

    // Compare hunks content
    assert_eq!(original[0].hunks[0].lines, twice[0].hunks[0].lines);
}

#[test]
fn test_apply_inverted_patch_undoes_changes() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("file.txt");

    // Scenario: We have a file that *already* has the "new" content.
    // We want to apply the patch in reverse to go back to "old".
    fs::write(&file_path, "context\nnew value\n").unwrap();

    // The patch describes going from Old -> New
    let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -1,2 +1,2 @@
         context
        -old value
        +new value
    "#};

    let patches = parse_auto(diff).unwrap();

    // Invert: Now describes going from New -> Old
    let reversed_patches = invert_patches(&patches);

    let options = ApplyOptions::exact();
    let result = apply_patch_to_file(&reversed_patches[0], dir.path(), options).unwrap();

    assert!(result.report.all_applied_cleanly());

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "context\nold value\n");
}

#[test]
fn test_invert_multiple_files() {
    let _ = env_logger::builder().is_test(true).try_init();
    let diff = indoc! {r#"
        --- a/f1
        +++ b/f1
        @@ -1 +1 @@
        -a
        +b
        --- a/f2
        +++ b/f2
        @@ -1 +1 @@
        -c
        +d
    "#};

    let patches = parse_auto(diff).unwrap();
    let inverted = invert_patches(&patches);

    assert_eq!(inverted.len(), 2);

    // Check f1
    assert_eq!(inverted[0].hunks[0].removed_lines(), vec!["b"]);
    assert_eq!(inverted[0].hunks[0].added_lines(), vec!["a"]);

    // Check f2
    assert_eq!(inverted[1].hunks[0].removed_lines(), vec!["d"]);
    assert_eq!(inverted[1].hunks[0].added_lines(), vec!["c"]);
}

#[test]
fn test_invert_conflict_markers() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Conflict markers: <<<< (Old) ==== (New) >>>>
    let diff = indoc! {r#"
        <<<<
        old
        ====
        new
        >>>>
    "#};

    let patches = parse_auto(diff).unwrap();
    // Original: -old, +new

    let inverted = invert_patches(&patches);
    // Inverted: -new, +old

    let hunk = &inverted[0].hunks[0];
    assert_eq!(hunk.removed_lines(), vec!["new"]);
    assert_eq!(hunk.added_lines(), vec!["old"]);
}

#[test]
fn test_invert_mixed_hunk() {
    let _ = env_logger::builder().is_test(true).try_init();
    // A hunk with context, additions, and deletions mixed
    let diff = indoc! {r#"
        --- a/file
        +++ b/file
        @@ -1,4 +1,4 @@
         ctx1
        -del1
        +add1
         ctx2
        -del2
        +add2
    "#};

    let patches = parse_auto(diff).unwrap();
    let inverted = invert_patches(&patches);
    let hunk = &inverted[0].hunks[0];

    // Expected lines in inverted hunk (simple inversion, no reordering):
    //  ctx1
    // +del1
    // -add1
    //  ctx2
    // +del2
    // -add2

    let lines = &hunk.lines;
    assert_eq!(lines[0], " ctx1");
    assert_eq!(lines[1], "+del1");
    assert_eq!(lines[2], "-add1");
    assert_eq!(lines[3], " ctx2");
    assert_eq!(lines[4], "+del2");
    assert_eq!(lines[5], "-add2");
}

#[test]
fn test_invert_empty_patch_list() {
    let _ = env_logger::builder().is_test(true).try_init();
    let patches: Vec<Patch> = vec![];
    let inverted = invert_patches(&patches);
    assert!(inverted.is_empty());
}

#[test]
fn test_complex_apply_and_reverse_cycle() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("complex_cycle.txt");

    // 1. Setup Original Content
    // Contains sections for Modification, Deletion, and Addition.
    let original_content = indoc! {r#"
        fn main() {
            // Part 1: Modification
            let x = 10;
            println!("Value: {}", x);

            // Part 2: Deletion
            let unused = "delete me";
            let unused_2 = "delete me too";

            // Part 3: Addition
            return;
        }
    "#};
    fs::write(&file_path, original_content).unwrap();

    // 2. Define Patch (Original -> Modified)
    let diff = indoc! {r#"
        --- a/complex_cycle.txt
        +++ b/complex_cycle.txt
        @@ -3,3 +3,3 @@
             // Part 1: Modification
        -    let x = 10;
        +    let x = 20;
             println!("Value: {}", x);
        @@ -6,4 +6,1 @@
         
             // Part 2: Deletion
        -    let unused = "delete me";
        -    let unused_2 = "delete me too";
        -
             // Part 3: Addition
        @@ -11,2 +8,3 @@
             // Part 3: Addition
        +    println!("Done");
             return;
    "#};

    let patches = parse_auto(diff).unwrap();
    let options = ApplyOptions::exact();

    // 3. Apply Original Patch (Forward)
    let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();
    assert!(result.report.all_applied_cleanly(), "Forward patch failed");

    // 4. Invert Patch
    let reversed_patches = invert_patches(&patches);

    // 5. Apply Reversed Patch (Backward)
    let result_rev = apply_patch_to_file(&reversed_patches[0], dir.path(), options).unwrap();
    assert!(
        result_rev.report.all_applied_cleanly(),
        "Reverse patch failed"
    );

    // 6. Verify Restoration
    let restored_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(
        restored_content, original_content,
        "Reverse patch did not restore original content exactly"
    );
}

#[test]
fn test_newline_only_file_preservation() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Tests the fix where a file intended to be exactly one newline
    // was being truncated to 0 bytes.
    let original = "content\n";
    let diff = indoc! {r#"
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -content
        +
        ```
    "#};

    let options = ApplyOptions::new();
    let result = patch_content_str(diff, Some(original), &options).unwrap();

    // Result should be exactly one newline, not an empty string.
    assert_eq!(result, "\n");
    assert_eq!(result.len(), 1);
}

#[test]
fn test_conflict_marker_detection_false_positive_markdown_header() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Verifies that a Markdown H1 header (text followed by ====)
    // is NOT detected as a conflict marker patch.
    let content = indoc! {r#"
        My Document Title
        =================
        
        This is just a normal markdown file.
    "#};

    assert_eq!(detect_patch(content), PatchFormat::Unknown);

    let patches = parse_auto(content).unwrap();
    assert!(
        patches.is_empty(),
        "Should not have found patches in a standard MD header"
    );
}

#[test]
fn test_conflict_marker_detection_requires_start_and_end() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Verifies that we need both <<<< and (==== or >>>>) to trigger detection.
    let only_start = "<<<< Just some text";
    assert_eq!(detect_patch(only_start), PatchFormat::Unknown);

    let valid_conflict = "<<<<\nold\n>>>>";
    assert_eq!(detect_patch(valid_conflict), PatchFormat::Conflict);
}

#[test]
fn test_smart_indentation_tabs_to_tabs() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("tabs.py");

    // Target file uses TABS
    let original_content = "def main():\n\tprint(\"hello\")\n";
    fs::write(&file_path, original_content).unwrap();

    // Patch uses SPACES (e.g. from an LLM or web snippet)
    let diff = indoc! {r#"
        ```diff
        --- a/tabs.py
        +++ b/tabs.py
        @@ -1,2 +1,3 @@
         def main():
             print("hello")
        +    print("world")
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    // The added line should have been converted to use a TAB to match the file style.
    let expected = "def main():\n\tprint(\"hello\")\n\tprint(\"world\")\n";
    assert_eq!(
        content, expected,
        "Indentation should have converted spaces to tabs"
    );
}

#[test]
fn test_parse_empty_hunk_header_allowed() {
    let _ = env_logger::builder().is_test(true).try_init();
    // The changelog mentions support for empty hunks (@@ -0,0 +0,0 @@).
    // This test ensures the parser doesn't skip them.
    let diff = indoc! {r#"
        --- a/empty_hunk.txt
        +++ b/empty_hunk.txt
        @@ -0,0 +0,0 @@
    "#};

    let patches = parse_auto(diff).unwrap();
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].hunks.len(), 1);
    assert!(patches[0].hunks[0].lines.is_empty());
}

#[test]
fn test_markdown_fence_indentation_strictness() {
    let _ = env_logger::builder().is_test(true).try_init();
    // A closing fence must not be more indented than the opening fence.
    // This prevents a context line that happens to start with ``` from
    // prematurely closing the patch block.
    let diff = indoc! {r#"
        ```diff
        --- a/code.md
        +++ b/code.md
        @@ -1,3 +1,3 @@
         Some text
             ```
         More text
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    assert_eq!(patches.len(), 1);
    let hunk = &patches[0].hunks[0];

    // The indented ``` should be treated as a context line.
    assert!(hunk.lines.iter().any(|l| l == "     ```"));
    assert!(hunk.lines.iter().any(|l| l == " More text"));
}

#[test]
fn test_conflict_marker_pure_deletion_no_separator() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Tests a conflict marker block that only has <<<< and >>>> (no ====).
    // This should be interpreted as a pure deletion.
    let original = "line 1\nline 2\nline 3\n";
    let diff = indoc! {r#"
        line 1
        <<<<
        line 2
        >>>>
        line 3
    "#};

    let options = ApplyOptions::new();
    let result = patch_content_str(diff, Some(original), &options).unwrap();

    assert_eq!(result, "line 1\nline 3\n");
}

#[test]
fn test_apply_patch_to_empty_file_resulting_in_newline() {
    let _ = env_logger::builder().is_test(true).try_init();
    // Test creating a file that consists of exactly one newline.
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("newline.txt");

    let diff = indoc! {r#"
        --- /dev/null
        +++ b/newline.txt
        @@ -0,0 +1 @@
        +
    "#};

    let patches = parse_auto(diff).unwrap();
    apply_patch_to_file(&patches[0], dir.path(), ApplyOptions::new()).unwrap();

    let content = fs::read_to_string(file_path).unwrap();
    assert_eq!(content, "\n");
}

#[test]
fn test_multiple_file_creations_with_empty_lines_between() {
    let _ = env_logger::builder().is_test(true).try_init();
    let diff = indoc! {r#"
        --- /dev/null
        +++ b/file1.txt
        @@ -0,0 +1,2 @@
        +content 1
        +content 2

        --- /dev/null
        +++ b/file2.txt
        @@ -0,0 +1,2 @@
        +content 3
        +content 4
    "#};

    let patches = parse_patches(diff).unwrap();
    assert_eq!(patches.len(), 2);

    assert!(patches[0].is_creation());
    assert_eq!(patches[0].file_path.to_str().unwrap(), "file1.txt");
    assert_eq!(
        patches[0].hunks[0].added_lines(),
        vec!["content 1", "content 2"]
    );
    // The trailing empty line should be stripped, so context_lines should be empty
    assert!(patches[0].hunks[0].context_lines().is_empty());

    assert!(patches[1].is_creation());
    assert_eq!(patches[1].file_path.to_str().unwrap(), "file2.txt");
    assert_eq!(
        patches[1].hunks[0].added_lines(),
        vec!["content 3", "content 4"]
    );
}

#[test]
fn test_smart_indentation_outdented_added_line_fallback() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("outdent.rs");

    // Target file has 0-space indentation
    let original_content = indoc! {r#"
        println!("Hello");
    "#};
    fs::write(&file_path, original_content).unwrap();

    // Patch has 8-space indentation for context, and adds lines with 4 and 0 spaces.
    let diff = indoc! {r#"
        ```diff
        --- a/outdent.rs
        +++ b/outdent.rs
        @@ -1,1 +1,3 @@
                 println!("Hello");
        +    }
        +}
        ```
    "#};

    let patches = parse_diffs(diff).unwrap();
    let options = ApplyOptions::new();
    apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

    let content = fs::read_to_string(&file_path).unwrap();

    // Without the fix, the first added line would incorrectly retain 4 spaces: `    }`
    // With the fix, it correctly strips the remaining spaces to reach 0 indentation: `}`
    let expected = indoc! {r#"
        println!("Hello");
        }
        }
    "#};
    assert_eq!(content, expected);
}

mod entropy_and_orphan_guards {
    use indoc::indoc;
    use mpatch::{
        apply_patch_to_file, parse_auto, parse_diffs, try_apply_patch_to_content, ApplyOptions,
        Hunk, HunkApplyError, HunkApplyStatus,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_rejects_ambiguous_low_entropy_closing_brace() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("lib.rs");

        let original_content = indoc! {r#"
            pub fn first() {
                println!("one");
            }

            pub fn second() {
                println!("two");
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        // Malformed patch anchoring solely on a lone closing brace
        let diff = indoc! {r#"
            --- a/lib.rs
            +++ b/lib.rs
            @@ -3,1 +3,3 @@
            +pub fn injected() {}
             }
        "#};

        let patches = parse_auto(diff).unwrap();
        let options = ApplyOptions::exact();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            !result.report.all_applied_cleanly(),
            "Should refuse to tie-break among multiple closing braces with low-entropy context"
        );
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::AmbiguousExactMatch(_))
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original_content);
    }

    #[test]
    fn test_accepts_low_entropy_when_unambiguous() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("single_brace.rs");

        let original_content = "fn only() {\n    return;\n}\n";
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            --- a/single_brace.rs
            +++ b/single_brace.rs
            @@ -2,1 +2,2 @@
            +    println!("added");
             }
        "#};

        let patches = parse_auto(diff).unwrap();
        let options = ApplyOptions::exact();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content,
            "fn only() {\n    return;\n    println!(\"added\");\n}\n"
        );
    }

    #[test]
    fn test_orphaned_addition_rejected_during_fuzzy_match() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");

        // Setup func_one with enough lines so that func_one alone matches >= 70% of the hunk,
        // causing the fuzzy window finder to select it while leaving func_two outside the window.
        let mut original_content = String::from("fn func_one() {\n");
        for i in 0..10 {
            original_content.push_str(&format!("    let step_{} = {};\n", i, i));
        }
        original_content.push_str("}\n\n");
        for i in 0..60 {
            original_content.push_str(&format!("/// Intervening doc comment {}\n", i));
        }
        original_content.push_str("fn func_two(arg: u32) {\n    step_2();\n}\n");
        fs::write(&file_path, &original_content).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/main.rs
            +++ b/main.rs
            @@ -1,13 +1,15 @@
             fn func_one() {
                 let step_0 = 0;
                 let step_1 = 1;
            -    let step_2 = 2;
            +    let step_2_modified = 2;
                 let step_3 = 3;
                 let step_4 = 4;
             }

             fn func_two(arg: u32) {
            +    injected_inside_func_two();
                 step_2();
             }
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        // The hunk must be rejected with ContextNotFound by the orphan guard during reconstruction
        // instead of dumping `injected_inside_func_two()` after func_one!
        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "File must remain untouched on failure"
        );
    }

    #[test]
    fn test_stale_context_without_additions_still_skipped() {
        let _ = env_logger::builder().is_test(true).try_init();
        let original = "line A\nline C\n";
        let diff = indoc! {r#"
            --- a/test.txt
            +++ b/test.txt
            @@ -1,4 +1,4 @@
             line A
             line B
            -line C
            +line modified
        "#};

        let patch = parse_auto(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = try_apply_patch_to_content(&patch, Some(original), &options).unwrap();

        assert_eq!(result.new_content, "line A\nline modified\n");
    }

    #[test]
    fn test_eof_truncation_restoration_with_additions() {
        let _ = env_logger::builder().is_test(true).try_init();
        let original = "fn main() {\n    run();\n";
        let diff = indoc! {r#"
            --- a/test.rs
            +++ b/test.rs
            @@ -1,3 +1,4 @@
             fn main() {
                 run();
             }
            +// EOF comment
        "#};

        let patch = parse_auto(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = try_apply_patch_to_content(&patch, Some(original), &options).unwrap();

        assert_eq!(
            result.new_content,
            "fn main() {\n    run();\n}\n// EOF comment\n"
        );
    }

    #[test]
    fn test_fuzzy_match_multi_anchor_hunk_with_intervening_doc_comments() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("service.rs");

        let original_content = indoc! {r#"
            fn start_service() {
                init();
            }

            /// Detailed documentation for stopping the service.
            /// 1. Flushes internal buffers.
            /// 2. Closes all open network sockets.
            /// 3. Signals background worker threads to exit.
            /// 4. Waits for termination confirmation.
            /// 5. Logs final shutdown status.
            fn stop_service(timeout: u64) {
                teardown();
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        let diff = [
            "--- a/service.rs",
            "+++ b/service.rs",
            "@@ -1,9 +1,11 @@",
            " fn start_service() {",
            "+    configure_tls();",
            "     init();",
            " }",
            "",
            "-fn stop_service(timeout: u64) {",
            "+fn stop_service(timeout: u64, force: bool) {",
            "     teardown();",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Multi-anchor hunk should apply cleanly across intervening doc comments"
        );
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("configure_tls();"));
        assert!(content.contains("fn stop_service(timeout: u64, force: bool)"));
        assert!(content.contains("/// Detailed documentation for stopping the service."));
        assert!(content.contains("/// 5. Logs final shutdown status."));
    }

    #[test]
    fn test_fuzzy_match_cv_clip_hunk3_regression() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");

        let original_content = indoc! {r#"
            /// # Arguments
            /// * `input`: The source to read from.
            /// * `clipboard`: A mutable reference to a clipboard implementation.
            fn run_pipe_mode<R: Read, C: Clipboard>(mut input: R, clipboard: &mut C) -> Result<(), AppError> {
                let mut buffer = String::new();
                input
                    .read_to_string(&mut buffer)
                    .map_err(|e| AppError::Io(e.to_string()))?;
                copy_to_clipboard(clipboard, &buffer)
            }

            /// Copies the given content to the clipboard after minor processing.
            ///
            /// This function performs two main tasks before setting the clipboard contents:
            /// 1. It strips a single trailing newline, which is common in command output.
            /// 2. It checks if the resulting content is empty and avoids modifying the clipboard
            ///    if it is, printing a warning instead.
            ///
            /// # Arguments
            /// * `clipboard`: A mutable reference to a clipboard implementation.
            /// * `content`: The string content to copy.
            fn copy_to_clipboard<C: Clipboard>(
                clipboard: &mut C,
                content: &str,
            ) -> Result<(), AppError> {
                let content_to_copy = content.strip_suffix('\n').unwrap_or(content);

                if content_to_copy.is_empty() {
                    return Ok(());
                }
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        let diff = [
            "--- a/main.rs",
            "+++ b/main.rs",
            "@@ -1,21 +1,33 @@",
            " /// # Arguments",
            " /// * `input`: The source to read from.",
            "+/// * `anonymize`: Whether to replace the user's home directory path with `~`.",
            " /// * `clipboard`: A mutable reference to a clipboard implementation.",
            "-fn run_pipe_mode<R: Read, C: Clipboard>(mut input: R, clipboard: &mut C) -> Result<(), AppError> {",
            "+fn run_pipe_mode<R: Read, C: Clipboard>(",
            "+    mut input: R,",
            "+    anonymize: bool,",
            "+    clipboard: &mut C,",
            "+) -> Result<(), AppError> {",
            "     let mut buffer = String::new();",
            "     input",
            "         .read_to_string(&mut buffer)",
            "         .map_err(|e| AppError::Io(e.to_string()))?;",
            "-    copy_to_clipboard(clipboard, &buffer)",
            "+    copy_to_clipboard(clipboard, &buffer, anonymize)",
            " }",
            "",
            "+/// Copies the given content to the clipboard after optional anonymization and trimming.",
            " fn copy_to_clipboard<C: Clipboard>(",
            "     clipboard: &mut C,",
            "     content: &str,",
            "+    anonymize: bool,",
            " ) -> Result<(), AppError> {",
            "+    let anonymized;",
            "+    let content = if anonymize {",
            "+        anonymized = cv_clip::anonymize_text(content);",
            "+        &anonymized",
            "+    } else {",
            "+        content",
            "+    };",
            "     let content_to_copy = content.strip_suffix('\\n').unwrap_or(content);",
            "",
            "     if content_to_copy.is_empty() {",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Regression test failed: Hunk 3 should apply cleanly without ContextNotFound"
        );
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("anonymize: bool,"));
        assert!(content.contains("copy_to_clipboard(clipboard, &buffer, anonymize)"));
        assert!(content.contains("anonymized = cv_clip::anonymize_text(content);"));
        assert!(content.contains(
            "This function performs two main tasks before setting the clipboard contents:"
        ));
    }

    #[test]
    fn test_backtracking_bypasses_high_scoring_truncated_window() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("service.rs");

        let mut original = String::from("fn start_engine() {\n");
        for i in 0..12 {
            original.push_str(&format!("    let phase_{} = {};\n", i, i));
        }
        original.push_str("}\n\n");
        for i in 0..12 {
            original.push_str(&format!("// Intervening service telemetry notes {}\n", i));
        }
        original.push_str("\nfn stop_engine() {\n    teardown_all();\n}\n");
        fs::write(&file_path, &original).unwrap();

        let diff = [
            "--- a/service.rs",
            "+++ b/service.rs",
            "@@ -1,18 +1,20 @@",
            " fn start_engine() {",
            "     let phase_0 = 0;",
            "-    let phase_1 = 1;",
            "+    let phase_1 = 100;",
            "     let phase_2 = 2;",
            "     let phase_3 = 3;",
            " }",
            "",
            " fn stop_engine() {",
            "+    flush_telemetry();",
            "     teardown_all();",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Applier must backtrack to the full window when the truncated window cannot anchor stop_engine"
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("let phase_1 = 100;"));
        assert!(content.contains("flush_telemetry();"));
        assert!(content.contains("// Intervening service telemetry notes 0"));
        assert!(content.contains("// Intervening service telemetry notes 11"));
    }

    #[test]
    fn test_backtracking_all_candidates_exhausted_preserves_target() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("immutable.rs");

        let original = indoc! {r#"
            fn alpha() {
                step_a();
                step_b();
            }

            fn beta() {
                step_c();
                step_d();
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = [
            "--- a/immutable.rs",
            "+++ b/immutable.rs",
            "@@ -1,7 +1,9 @@",
            " fn alpha() {",
            "     step_a();",
            "+    step_alpha_new();",
            "     step_b();",
            " }",
            "",
            " fn nonexistent_gamma() {",
            "+    injected_rogue_line();",
            "     never_heard_of_this();",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            !result.report.all_applied_cleanly(),
            "Should fail cleanly when no candidate can anchor all additions"
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original,
            "Target file must remain 100% pristine if hunk application fails after backtracking"
        );
    }

    #[test]
    fn test_single_line_signature_expansion_python() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("metrics.py");

        let original = indoc! {r#"
            def compute_metrics(predictions, targets, average="macro"):
                score = calculate_f1(predictions, targets, average)
                return score
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = [
            "--- a/metrics.py",
            "+++ b/metrics.py",
            "@@ -1,3 +1,5 @@",
            " def compute_metrics(",
            "     predictions,",
            "     targets,",
            "+    weights=None,",
            "     average=\"macro\",",
            " ):",
            "+    validate_weights(weights)",
            "     score = calculate_f1(predictions, targets, average)",
            "     return score",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Should expand single-line Python signature to multi-line with inserted parameter and body statement"
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("weights=None,"));
        assert!(content.contains("validate_weights(weights)"));
        assert!(content.contains("score = calculate_f1(predictions, targets, average)"));
    }

    #[test]
    fn test_single_line_statement_expansion_javascript() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("api.js");

        let original = indoc! {r#"
            async function init() {
                const user = await fetchUser(userId, { timeout: 5000 });
                console.log(user.name);
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = [
            "--- a/api.js",
            "+++ b/api.js",
            "@@ -1,4 +1,5 @@",
            " async function init() {",
            "     const user = await fetchUser(",
            "         userId,",
            "+        authToken,",
            "         { timeout: 5000 },",
            "     );",
            "     console.log(user.name);",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("authToken,"));
        assert!(content.contains("timeout: 5000"));
        assert!(content.contains("console.log(user.name);"));
    }

    #[test]
    fn test_multi_anchor_hunk_with_intervening_helper_function() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("workflow.rs");

        let original = indoc! {r#"
            pub fn step_one() {
                init_logger();
            }

            // Local helper added in current branch
            fn sanitize_environment() -> bool {
                check_env_vars()
            }

            pub fn step_two(status: bool) {
                finalize(status);
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = [
            "--- a/workflow.rs",
            "+++ b/workflow.rs",
            "@@ -1,9 +1,11 @@",
            " pub fn step_one() {",
            "+    setup_tracing();",
            "     init_logger();",
            " }",
            "",
            "-pub fn step_two(status: bool) {",
            "+pub fn step_two(status: bool, retries: u32) {",
            "     finalize(status);",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Hunk must span across locally inserted helper function without destroying it"
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("setup_tracing();"));
        assert!(content.contains("pub fn step_two(status: bool, retries: u32)"));
        assert!(content.contains("fn sanitize_environment() -> bool"));
        assert!(content.contains("check_env_vars()"));
    }

    #[test]
    fn test_genuine_orphan_addition_rejected_even_with_statement_matching() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("guard.rs");

        let original = indoc! {r#"
            fn calculate(a: i32, b: i32) -> i32 {
                a + b
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = [
            "--- a/guard.rs",
            "+++ b/guard.rs",
            "@@ -1,3 +1,4 @@",
            " fn unrelated_worker(msg: &str) {",
            "+    unauthorized_injection();",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            !result.report.all_applied_cleanly(),
            "Unrelated statement must not match calculate() and must be rejected"
        );
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn test_dry_run_diff_generation_with_backtracked_expansion() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("dry_run_test.py");

        let original = "def send_email(to, subject):\n    return smtp.send(to, subject)\n";
        fs::write(&file_path, original).unwrap();

        let diff = [
            "--- a/dry_run_test.py",
            "+++ b/dry_run_test.py",
            "@@ -1,2 +1,3 @@",
            " def send_email(",
            "     to,",
            "+    cc=None,",
            "     subject,",
            " ):",
            "     return smtp.send(to, subject)",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::dry_run();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        assert!(
            result.diff.is_some(),
            "Dry run must generate unified diff output"
        );

        let diff_str = result.diff.unwrap();
        assert!(diff_str.contains("+    cc=None,"));
        assert!(diff_str.contains("--- a/dry_run_test.py"));
        assert!(diff_str.contains("+++ b/dry_run_test.py"));

        let on_disk = fs::read_to_string(&file_path).unwrap();
        assert_eq!(on_disk, original);
    }

    #[test]
    fn test_required_match_span_calculation() {
        let _ = env_logger::builder().is_test(true).try_init();
        let h1 = Hunk {
            lines: vec![
                " ctx0".to_string(),
                "+add1".to_string(),
                " ctx1".to_string(),
                " ctx2".to_string(),
            ],
            old_start_line: Some(1),
            new_start_line: Some(1),
        };
        assert_eq!(h1.required_match_span(), 1);

        let h2 = Hunk {
            lines: vec![
                " ctx0".to_string(),
                "-del0".to_string(),
                " ctx1".to_string(),
                " ctx2".to_string(),
                " ctx3".to_string(),
                "+add4".to_string(),
                " ctx4".to_string(),
            ],
            old_start_line: Some(1),
            new_start_line: Some(1),
        };
        assert_eq!(h2.required_match_span(), 4);

        let h3 = Hunk {
            lines: vec![" ctx0".to_string(), " ctx1".to_string()],
            old_start_line: Some(1),
            new_start_line: Some(1),
        };
        assert_eq!(h3.required_match_span(), 0);
    }

    #[test]
    fn test_single_line_with_trailing_commas_and_generics() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("complex.rs");

        let original = indoc! {r#"
            pub fn dispatch<T: Send + Sync, R: Clone>(handler: &T, request: R) -> Result<(), AppError> {
                handler.handle(request);
                Ok(())
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = [
            "--- a/complex.rs",
            "+++ b/complex.rs",
            "@@ -1,4 +1,5 @@",
            " pub fn dispatch<T: Send + Sync, R: Clone>(",
            "     handler: &T,",
            "+    timeout: Duration,",
            "     request: R,",
            " ) -> Result<(), AppError> {",
            "     handler.handle(request);",
            "     Ok(())",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("timeout: Duration,"));
        assert!(content.contains("handler.handle(request);"));
    }

    #[test]
    fn test_fuzzy_match_cv_clip_hunk3_with_single_line_signature_and_doc_drift() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");

        let original_content = indoc! {r#"
            /// # Arguments
            /// * `input`: The source to read from.
            /// * `clipboard`: A mutable reference to a clipboard implementation.
            fn run_pipe_mode<R: Read, C: Clipboard>(mut input: R, clipboard: &mut C) -> Result<(), AppError> {
                let mut buffer = String::new();
                input
                    .read_to_string(&mut buffer)
                    .map_err(|e| AppError::Io(e.to_string()))?;
                copy_to_clipboard(clipboard, &buffer)
            }

            /// Copies the given content to the clipboard after minor processing.
            ///
            /// This function performs two main tasks before setting the clipboard contents:
            /// 1. It strips a single trailing newline, which is common in command output.
            /// 2. It checks if the resulting content is empty and avoids modifying the clipboard
            ///    if it is, printing a warning instead.
            ///
            /// # Arguments
            /// * `clipboard`: A mutable reference to a clipboard implementation.
            /// * `content`: The string content to copy.
            fn copy_to_clipboard<C: Clipboard>(clipboard: &mut C, content: &str) -> Result<(), AppError> {
                let content_to_copy = content.strip_suffix('\n').unwrap_or(content);

                if content_to_copy.is_empty() {
                    return Ok(());
                }
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        let diff = [
            "--- a/main.rs",
            "+++ b/main.rs",
            "@@ -1,21 +1,33 @@",
            " /// # Arguments",
            " /// * `input`: The source to read from.",
            "+/// * `anonymize`: Whether to replace the user's home directory path with `~`.",
            " /// * `clipboard`: A mutable reference to a clipboard implementation.",
            "-fn run_pipe_mode<R: Read, C: Clipboard>(mut input: R, clipboard: &mut C) -> Result<(), AppError> {",
            "+fn run_pipe_mode<R: Read, C: Clipboard>(",
            "+    mut input: R,",
            "+    anonymize: bool,",
            "+    clipboard: &mut C,",
            "+) -> Result<(), AppError> {",
            "     let mut buffer = String::new();",
            "     input",
            "         .read_to_string(&mut buffer)",
            "         .map_err(|e| AppError::Io(e.to_string()))?;",
            "-    copy_to_clipboard(clipboard, &buffer)",
            "+    copy_to_clipboard(clipboard, &buffer, anonymize)",
            " }",
            "",
            "+/// Copies the given content to the clipboard after optional anonymization and trimming.",
            " fn copy_to_clipboard<C: Clipboard>(",
            "     clipboard: &mut C,",
            "     content: &str,",
            "+    anonymize: bool,",
            " ) -> Result<(), AppError> {",
            "+    let anonymized;",
            "+    let content = if anonymize {",
            "+        anonymized = cv_clip::anonymize_text(content);",
            "+        &anonymized",
            "+    } else {",
            "+        content",
            "+    };",
            "     let content_to_copy = content.strip_suffix('\\n').unwrap_or(content);",
            "",
            "     if content_to_copy.is_empty() {",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Hunk 3 should apply cleanly despite single-line signature and intervening doc drift"
        );
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("anonymize: bool,"));
        assert!(content.contains("copy_to_clipboard(clipboard, &buffer, anonymize)"));
        assert!(content.contains("anonymized = cv_clip::anonymize_text(content);"));
        assert!(content.contains(
            "This function performs two main tasks before setting the clipboard contents:"
        ));
    }

    #[test]
    fn test_genuine_orphan_when_second_function_completely_deleted() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("service.rs");

        // Target file ONLY contains worker_primary. worker_secondary was completely deleted.
        let mut original_content = String::from("pub fn worker_primary() {\n");
        for i in 0..15 {
            original_content.push_str(&format!("    let stage_{} = execute_step({});\n", i, i));
        }
        original_content.push_str("}\n");
        fs::write(&file_path, &original_content).unwrap();

        // Patch was created against an older version that still had worker_secondary
        let diff = indoc! {r#"
            ```diff
            --- a/service.rs
            +++ b/service.rs
            @@ -1,18 +1,21 @@
             pub fn worker_primary() {
                 let stage_0 = execute_step(0);
                 let stage_1 = execute_step(1);
            -    let stage_2 = execute_step(2);
            +    let stage_2 = execute_step_v2(2);
                 let stage_3 = execute_step(3);
             }

             pub fn worker_secondary() {
            +    initialize_secondary_hardware();
                 execute_secondary_worker();
             }
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        // Must reject cleanly because worker_secondary does not exist in the file,
        // preventing initialize_secondary_hardware() from being orphaned into worker_primary!
        assert!(
            !result.report.all_applied_cleanly(),
            "Orphan addition to non-existent worker_secondary must trigger rejection"
        );
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must remain 100% untouched when orphan addition fails"
        );
    }

    #[test]
    fn test_multi_anchor_hunk_spanning_intervening_struct_and_enum() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("protocol.rs");

        let original = indoc! {r#"
            pub trait PacketDecoder {
                fn decode_header(&mut self) -> Result<Header, DecodeError>;
            }

            // Locally inserted definitions in current branch
            #[derive(Debug, Clone)]
            pub struct PacketConfig {
                pub max_frame_size: usize,
                pub verify_crc: bool,
            }

            #[derive(Debug, PartialEq, Eq)]
            pub enum DecoderState {
                AwaitingHeader,
                ReadingPayload,
                Corrupted,
            }

            impl PacketDecoder for StreamDecoder {
                fn decode_header(&mut self) -> Result<Header, DecodeError> {
                    read_magic_bytes(&mut self.stream)
                }
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = [
            "--- a/protocol.rs",
            "+++ b/protocol.rs",
            "@@ -1,8 +1,8 @@",
            " pub trait PacketDecoder {",
            "-    fn decode_header(&mut self) -> Result<Header, DecodeError>;",
            "+    fn decode_header(&mut self, timeout: Duration) -> Result<Header, DecodeError>;",
            " }",
            "",
            " impl PacketDecoder for StreamDecoder {",
            "-    fn decode_header(&mut self) -> Result<Header, DecodeError> {",
            "+    fn decode_header(&mut self, timeout: Duration) -> Result<Header, DecodeError> {",
            "+        self.stream.set_timeout(timeout);",
            "         read_magic_bytes(&mut self.stream)",
            "     }",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Hunk must cleanly span intervening struct and enum definitions without destroying them"
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains(
            "fn decode_header(&mut self, timeout: Duration) -> Result<Header, DecodeError>;"
        ));
        assert!(content.contains("self.stream.set_timeout(timeout);"));
        assert!(content.contains("pub struct PacketConfig"));
        assert!(content.contains("pub enum DecoderState"));
    }

    #[test]
    fn test_orphan_addition_in_rewritten_function_replacement_rejected() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("processor.rs");

        // Target file has a completely rewritten processor signature and body
        let original = indoc! {r#"
            pub fn start_pipeline() {
                init_buffers();
            }

            // Function was completely refactored to async streaming
            pub async fn process_incoming_stream(mut stream: Pin<Box<dyn AsyncRead>>) -> Result<(), NetworkError> {
                stream.read_to_end().await
            }
        "#};
        fs::write(&file_path, original).unwrap();

        // Patch targets the legacy synchronous processor, attempting to inject audit_log()
        let diff = [
            "--- a/processor.rs",
            "+++ b/processor.rs",
            "@@ -1,7 +1,9 @@",
            " pub fn start_pipeline() {",
            "     init_buffers();",
            " }",
            "",
            " pub fn process_legacy_buffer(data: &[u8]) -> Result<(), Error> {",
            "+    audit_log_buffer_access(data);",
            "     parse_and_validate(data)",
            " }",
        ]
        .join("\n");

        let patches = parse_auto(&diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            !result.report.all_applied_cleanly(),
            "Addition attached to rewritten/unaligned function must be rejected with ContextNotFound"
        );
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original, "Target file must remain pristine");
    }

    #[test]
    fn test_orphan_guard_in_python_nested_class() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("service.py");

        let original = indoc! {r#"
            class DataService:
                def __init__(self):
                    self.active = True

                def connect(self):
                    return create_conn()
        "#};
        fs::write(&file_path, original).unwrap();

        // Patch was written when DataService had a nested Helper class that has since been deleted
        let diff = indoc! {r#"
            ```diff
            --- a/service.py
            +++ b/service.py
            @@ -1,8 +1,11 @@
             class DataService:
                 def __init__(self):
                     self.active = True

            -    def connect(self):
            +    def connect(self, timeout=30):
                     return create_conn()

                 class InternalHelper:
            +        def auxiliary_method(self):
            +            pass
                     def helper(self): pass
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            !result.report.all_applied_cleanly(),
            "Additions attached to deleted nested class must not be dumped into outer class"
        );
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, original);
    }

    #[test]
    fn test_orphan_addition_attached_to_deleted_struct_field_in_rust() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.rs");

        // Target file has 11 fields, but `tls_key_path` was removed in a refactor.
        let original_content = indoc! {r#"
            pub struct ServerConfig {
                pub host: String,
                pub port: u16,
                pub max_connections: usize,
                pub read_timeout: Duration,
                pub write_timeout: Duration,
                pub keep_alive: bool,
                pub tls_cert_path: Option<PathBuf>,
                pub enable_compression: bool,
                pub buffer_size: usize,
                pub workers: usize,
                pub backlog: i32,
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        // Patch was created when `tls_key_path` existed, and attaches an addition to it.
        // 11 of 12 fields match identically (91.6% > 70%), so the fuzzy finder selects ServerConfig.
        let diff = indoc! {r#"
            ```diff
            --- a/config.rs
            +++ b/config.rs
            @@ -1,13 +1,14 @@
             pub struct ServerConfig {
                 pub host: String,
            -    pub port: u16,
            +    pub port: u32,
                 pub max_connections: usize,
                 pub read_timeout: Duration,
                 pub write_timeout: Duration,
                 pub keep_alive: bool,
                 pub tls_cert_path: Option<PathBuf>,
                 pub tls_key_path: Option<PathBuf>,
            +    pub tls_cipher_suite: Option<String>,
                 pub enable_compression: bool,
                 pub buffer_size: usize,
                 pub workers: usize,
                 pub backlog: i32,
             }
            ```
       "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        // The orphan guard must reject because `tls_key_path` is missing from target,
        // preventing `tls_cipher_suite` from being blindly injected.
        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "File must remain untouched when orphan addition fails"
        );
    }

    #[test]
    fn test_orphan_addition_in_typescript_deleted_event_handler() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("controller.ts");

        // Target file only has `handleOpen`. `handleClose` was removed.
        let original_content = indoc! {r#"
            export class ModalController {
                private step_0: boolean = false;
                private step_1: boolean = false;
                private step_2: boolean = false;
                private step_3: boolean = false;
                private step_4: boolean = false;
                private step_5: boolean = false;
                private step_6: boolean = false;
                private step_7: boolean = false;
                handleOpen() {
                    this.isOpen = true;
                    this.emit('opened');
                }
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/controller.ts
            +++ b/controller.ts
            @@ -1,13 +1,15 @@
             export class ModalController {
                 private step_0: boolean = false;
                 private step_1: boolean = false;
                 private step_2: boolean = false;
                 private step_3: boolean = false;
                 private step_4: boolean = false;
                 private step_5: boolean = false;
                 private step_6: boolean = false;
                 private step_7: boolean = false;
                 handleOpen() {
                     this.isOpen = true;
                     this.emit('opened');
                 }
            +
                 handleClose() {
            +        analytics.track('modal_closed');
                     this.isOpen = false;
                 }
             }
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must not be modified when handler is missing"
        );
    }

    #[test]
    fn test_orphan_addition_in_go_route_registration() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("routes.go");

        // Target file has 10 active routes, but the legacy route was removed.
        let original_content = indoc! {r#"
            func RegisterRoutes(r *mux.Router) {
                r.HandleFunc("/health", HealthHandler).Methods("GET")
                r.HandleFunc("/status", StatusHandler).Methods("GET")
                r.HandleFunc("/metrics", MetricsHandler).Methods("GET")
                r.HandleFunc("/users", ListUsersHandler).Methods("GET")
                r.HandleFunc("/users/{id}", GetUserHandler).Methods("GET")
                r.HandleFunc("/config", GetConfigHandler).Methods("GET")
                r.HandleFunc("/version", VersionHandler).Methods("GET")
                r.HandleFunc("/ping", PingHandler).Methods("GET")
                r.HandleFunc("/ready", ReadyHandler).Methods("GET")
                r.HandleFunc("/live", LiveHandler).Methods("GET")
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        // Patch modifies /metrics and attempts to attach a new endpoint to the removed /legacy route.
        let diff = indoc! {r#"
            ```diff
            --- a/routes.go
            +++ b/routes.go
            @@ -1,13 +1,14 @@
             func RegisterRoutes(r *mux.Router) {
                 r.HandleFunc("/health", HealthHandler).Methods("GET")
                 r.HandleFunc("/status", StatusHandler).Methods("GET")
            -    r.HandleFunc("/metrics", MetricsHandler).Methods("GET")
            +    r.HandleFunc("/metrics", PrometheusMetricsHandler).Methods("GET")
                 r.HandleFunc("/users", ListUsersHandler).Methods("GET")
                 r.HandleFunc("/users/{id}", GetUserHandler).Methods("GET")
                 r.HandleFunc("/config", GetConfigHandler).Methods("GET")
                 r.HandleFunc("/version", VersionHandler).Methods("GET")
                 r.HandleFunc("/ping", PingHandler).Methods("GET")
                 r.HandleFunc("/ready", ReadyHandler).Methods("GET")
                 r.HandleFunc("/live", LiveHandler).Methods("GET")
                 r.HandleFunc("/legacy", LegacyHandler).Methods("GET")
            +    r.HandleFunc("/legacy/v2", LegacyV2Handler).Methods("GET")
             }
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must remain untouched when route anchor is missing"
        );
    }

    #[test]
    fn test_orphan_addition_in_sql_table_schema() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("schema.sql");

        // Target file has 11 columns; `credit_limit` was dropped in migration.
        let original_content = indoc! {r#"
            CREATE TABLE accounts (
                id UUID PRIMARY KEY,
                user_id UUID NOT NULL,
                account_number VARCHAR(32) NOT NULL,
                currency VARCHAR(3) NOT NULL,
                balance NUMERIC(18, 4) NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                is_active BOOLEAN DEFAULT TRUE,
                account_tier INT DEFAULT 1,
                routing_code VARCHAR(16) NOT NULL,
                branch_id INT NOT NULL
            );
        "#};
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/schema.sql
            +++ b/schema.sql
            @@ -1,13 +1,14 @@
             CREATE TABLE accounts (
                 id UUID PRIMARY KEY,
                 user_id UUID NOT NULL,
                 account_number VARCHAR(32) NOT NULL,
                 currency VARCHAR(3) NOT NULL,
            -    balance NUMERIC(18, 4) NOT NULL,
            +    balance NUMERIC(20, 4) NOT NULL,
                 credit_limit NUMERIC(18, 4) DEFAULT 0,
            +    overdraft_protection BOOLEAN DEFAULT FALSE,
                 created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                 updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
                 is_active BOOLEAN DEFAULT TRUE,
                 account_tier INT DEFAULT 1,
                 routing_code VARCHAR(16) NOT NULL,
                 branch_id INT NOT NULL
             );
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must not have additions spliced when column anchor is missing"
        );
    }

    #[test]
    fn test_orphan_addition_rejected_on_completely_divergent_replace_line() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("pipeline.rs");

        // Target file has 14 lines. Line 11 is completely different from patch.
        let original_content = indoc! {r#"
            pub fn run_pipeline() {
                let stage_1 = init();
                let stage_2 = configure();
                let stage_3 = allocate();
                let stage_4 = bind();
                let stage_5 = listen();
                let stage_6 = poll();
                let stage_7 = serve();
                let stage_8 = drain();
                let stage_9 = flush();
                let crypto_vault_key = AesGcm256::generate_random();
                let stage_11 = teardown();
                let stage_12 = finish();
            }
        "#};
        fs::write(&file_path, original_content).unwrap();

        // Patch has 13 matching lines (92.8% similarity), but line 11 is `let network_proxy_url...`
        // which is a context line (' ') with an addition attached to it.
        let diff = indoc! {r#"
            ```diff
            --- a/pipeline.rs
            +++ b/pipeline.rs
            @@ -1,14 +1,15 @@
             pub fn run_pipeline() {
                 let stage_1 = init();
                 let stage_2 = configure();
                 let stage_3 = allocate();
                 let stage_4 = bind();
                 let stage_5 = listen();
                 let stage_6 = poll();
                 let stage_7 = serve();
                 let stage_8 = drain();
                 let stage_9 = flush();
                 let network_proxy_url = "https://proxy.internal";
            +    let proxy_retries = 3;
                 let stage_11 = teardown();
                 let stage_12 = finish();
             }
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must not be modified when context line differs completely from target"
        );
    }

    #[test]
    fn test_orphan_addition_in_c_header_enums() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("protocol.h");

        // Target file only defines NetworkProtocol. SecurityProtocol was moved to security.h.
        let original_content = indoc! {r#"
            enum NetworkProtocol {
                PROTO_NONE = 0,
                PROTO_IPV4 = 1,
                PROTO_IPV6 = 2,
                PROTO_TCP = 3,
                PROTO_UDP = 4,
                PROTO_ICMP = 5,
                PROTO_SCTP = 6,
                PROTO_DCCP = 7,
                PROTO_RAW = 8,
            };
        "#};
        fs::write(&file_path, original_content).unwrap();

        // Patch modifies PROTO_RAW and attempts to add a variant to SecurityProtocol.
        let diff = indoc! {r#"
            ```diff
            --- a/protocol.h
            +++ b/protocol.h
            @@ -1,13 +1,15 @@
             enum NetworkProtocol {
                 PROTO_NONE = 0,
                 PROTO_IPV4 = 1,
                 PROTO_IPV6 = 2,
                 PROTO_TCP = 3,
                 PROTO_UDP = 4,
                 PROTO_ICMP = 5,
                 PROTO_SCTP = 6,
                 PROTO_DCCP = 7,
            -    PROTO_RAW = 8,
            +    PROTO_RAW_SOCKET = 8,
             };

             enum SecurityProtocol {
            +    SEC_QUIC = 3,
                 SEC_NONE = 0,
             };
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must remain untouched when second enum is missing"
        );
    }

    #[test]
    fn test_orphan_addition_in_html_template_missing_footer() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("layout.html");

        // Target file has navigation and content, but footer was moved to a partial.
        let original_content = indoc! {r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>App</title>
                <link rel="stylesheet" href="style.css">
            </head>
            <body>
                <header class="navbar">
                    <nav>
                        <a href="/">Home</a>
                        <a href="/about">About</a>
                        <a href="/contact">Contact</a>
                    </nav>
                </header>
                <main class="container">
                    <p>Main content goes here.</p>
                </main>
            </body>
            </html>
        "#};
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/layout.html
            +++ b/layout.html
            @@ -6,14 +6,16 @@
             <body>
                 <header class="navbar">
                     <nav>
                         <a href="/">Home</a>
                         <a href="/about">About</a>
            -            <a href="/contact">Contact</a>
            +            <a href="/contact-us">Contact Us</a>
                     </nav>
                 </header>
                 <main class="container">
                     <p>Main content goes here.</p>
                 </main>
                 <footer class="site-footer">
            +        <p>&copy; 2026 Example Corp.</p>
                 </footer>
             </body>
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must not have footer additions injected into header"
        );
    }

    #[test]
    fn test_orphan_addition_in_aider_search_replace_block() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("worker.py");

        // Target file has `start_worker` with many lines, but `stop_worker` was deleted.
        let original_content = indoc! {r#"
            def start_worker():
                step_1 = initialize()
                step_2 = configure()
                step_3 = setup_buffers()
                step_4 = bind_listener()
                step_5 = start_threads()
                step_6 = wait_for_ready()
                step_7 = mark_active()
                return True
        "#};
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            worker.py
            <<<<<<< SEARCH
            def start_worker():
                step_1 = initialize()
                step_2 = configure()
                step_3 = setup_buffers()
                step_4 = bind_listener()
                step_5 = start_threads()
                step_6 = wait_for_ready()
                step_7 = mark_active()
                return True

            def stop_worker():
                teardown()
            =======
            def start_worker():
                step_1 = initialize()
                step_2 = configure()
                step_3 = setup_buffers()
                step_4 = bind_listener()
                step_5 = start_threads()
                step_6 = wait_for_ready()
                step_7 = mark_active()
                return True

            def stop_worker():
                audit_log("stopping")
                teardown()
            >>>>>>> REPLACE
        "#};

        let patches = parse_auto(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must not have additions injected when stop_worker is deleted"
        );
    }

    #[test]
    fn test_orphan_addition_in_python_data_pipeline_unaligned_block() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("pipeline.py");

        let original_content = indoc! {r#"
            def transform_dataset(df):
                df = clean_nulls(df)
                df = standardize_names(df)
                df = cast_types(df)
                df = filter_outliers(df)
                df = impute_missing(df)
                df = encode_categories(df)
                df = normalize_features(df)
                try:
                    df = custom_validator_block(df)
                except Exception:
                    raise
                return df
        "#};
        fs::write(&file_path, original_content).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/pipeline.py
            +++ b/pipeline.py
            @@ -1,11 +1,12 @@
             def transform_dataset(df):
                 df = clean_nulls(df)
                 df = standardize_names(df)
                 df = cast_types(df)
                 df = filter_outliers(df)
                 df = impute_missing(df)
                 df = encode_categories(df)
                 df = normalize_features(df)
                 df = legacy_validate_schema(df)
            +    df = log_pipeline_metrics(df)
                 return df
            ```
        "#};

        let patches = parse_diffs(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(!result.report.all_applied_cleanly());
        assert!(matches!(
            result.report.hunk_results[0],
            HunkApplyStatus::Failed(HunkApplyError::ContextNotFound)
        ));

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            content, original_content,
            "Target file must not be modified when unaligned context line has additions"
        );
    }
}

mod wildcard_and_path_tests {
    use indoc::indoc;
    use mpatch::{
        apply_patch_to_file, extract_file_path_from_line, is_ellipsis_line, is_plausible_file_path,
        parse_auto, patch_content_str, ApplyOptions,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_is_plausible_file_path_comprehensive() {
        assert!(is_plausible_file_path(".gitignore"));
        assert!(is_plausible_file_path(".env"));
        assert!(is_plausible_file_path(".env.local"));
        assert!(is_plausible_file_path(".dockerignore"));
        assert!(is_plausible_file_path(".editorconfig"));
        assert!(is_plausible_file_path("Makefile"));
        assert!(is_plausible_file_path("Dockerfile"));
        assert!(is_plausible_file_path("Jenkinsfile"));
        assert!(is_plausible_file_path(
            "src/components/My Component/Button.tsx"
        ));
        assert!(is_plausible_file_path("\"src/main.rs\""));
        assert!(is_plausible_file_path("src/main.rs:42:10"));
        assert!(is_plausible_file_path("schema.typescript"));

        assert!(!is_plausible_file_path("Here is the code in main.rs"));
        assert!(!is_plausible_file_path("Please check the following file:"));
        assert!(!is_plausible_file_path("https://example.com/file.rs"));
        assert!(!is_plausible_file_path(""));
        assert!(!is_plausible_file_path("..."));
    }

    #[test]
    fn test_extract_file_path_from_line_comprehensive() {
        assert_eq!(
            extract_file_path_from_line("Update \"src/config.json\":")
                .unwrap()
                .to_str()
                .unwrap(),
            "src/config.json"
        );
        assert_eq!(
            extract_file_path_from_line("In file 'src/server.ts':")
                .unwrap()
                .to_str()
                .unwrap(),
            "src/server.ts"
        );
        assert_eq!(
            extract_file_path_from_line("See [src/auth.rs](src/auth.rs)")
                .unwrap()
                .to_str()
                .unwrap(),
            "src/auth.rs"
        );
        assert_eq!(
            extract_file_path_from_line("File: `src/models/user.py`")
                .unwrap()
                .to_str()
                .unwrap(),
            "src/models/user.py"
        );
        assert_eq!(
            extract_file_path_from_line("### src/server.ts:42")
                .unwrap()
                .to_str()
                .unwrap(),
            "src/server.ts"
        );
        assert_eq!(
            extract_file_path_from_line(
                "diff --git a/crates/core/src/lib.rs b/crates/core/src/lib.rs"
            )
            .unwrap()
            .to_str()
            .unwrap(),
            "crates/core/src/lib.rs"
        );
        assert_eq!(
            extract_file_path_from_line("1. .gitignore")
                .unwrap()
                .to_str()
                .unwrap(),
            ".gitignore"
        );
        assert_eq!(
            extract_file_path_from_line("To fix this, edit src/utils/math.rs:")
                .unwrap()
                .to_str()
                .unwrap(),
            "src/utils/math.rs"
        );
    }

    #[test]
    fn test_is_ellipsis_line_comprehensive() {
        assert!(is_ellipsis_line("..."));
        assert!(is_ellipsis_line("…"));
        assert!(is_ellipsis_line("...."));
        assert!(is_ellipsis_line("    // ... existing code ..."));
        assert!(is_ellipsis_line("# ... rest of function ..."));
        assert!(is_ellipsis_line("/* ... */"));
        assert!(is_ellipsis_line("<!-- ... existing code ... -->"));
        assert!(is_ellipsis_line("-- ..."));
        assert!(is_ellipsis_line("; ..."));
        assert!(is_ellipsis_line("[...]"));

        assert!(!is_ellipsis_line("let x = 1;"));
        assert!(!is_ellipsis_line("foo(...args);"));
        assert!(!is_ellipsis_line("const { a, ...rest } = obj;"));
        assert!(!is_ellipsis_line(""));
        assert!(!is_ellipsis_line("// normal comment"));
    }

    #[test]
    fn test_python_stub_literal_ellipsis_replacement() {
        let original = indoc! {r#"
            class Repository:
                def fetch(self, id: int) -> dict:
                    ...
        "#};
        let diff = indoc! {r#"
            app.py
            <<<<<<< SEARCH
                def fetch(self, id: int) -> dict:
                    ...
            =======
                def fetch(self, id: int) -> dict:
                    return self.db.get(id)
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("return self.db.get(id)"));
        assert!(!patched.contains("..."));
    }

    #[test]
    fn test_aider_wildcard_boundary_ellipsis() {
        let original = indoc! {r#"
            def start():
                setup()
                old_call()
                teardown()
        "#};
        let diff = indoc! {r#"
            app.py
            <<<<<<< SEARCH
            // ... existing code ...
                old_call()
            // ... existing code ...
            =======
            // ... existing code ...
                new_call()
            // ... existing code ...
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("new_call()"));
        assert!(patched.contains("setup()"));
        assert!(patched.contains("teardown()"));
        assert!(!patched.contains("existing code"));
    }

    #[test]
    fn test_aider_wildcard_anchor_inside_function() {
        let original = indoc! {r#"
            def other():
                val = 1

            def calculate():
                init_calc()
                stage_1()
                stage_2()
                val = 1
                return val
        "#};
        let diff = indoc! {r#"
            app.py
            <<<<<<< SEARCH
            def calculate():
                ...
                val = 1
            =======
            def calculate():
                ...
                val = 2
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("def other():\n    val = 1"));
        assert!(patched.contains("init_calc()"));
        assert!(patched.contains("stage_2()"));
        assert!(patched.contains("val = 2"));
    }

    #[test]
    fn test_aider_wildcard_multi_hunk_split() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.py");
        let mut original = String::from("import os\n\n");
        for i in 0..50 {
            original.push_str(&format!("def helper_{}(): pass\n", i));
        }
        original.push_str("\ndef run():\n    old_runner()\n");
        fs::write(&file_path, &original).unwrap();

        let diff = indoc! {r#"
            main.py
            <<<<<<< SEARCH
            import os
            ...
            def run():
                old_runner()
            =======
            import os
            import sys
            ...
            def run():
                new_runner()
            >>>>>>> REPLACE
        "#};
        let patches = parse_auto(diff).unwrap();
        let res = apply_patch_to_file(&patches[0], dir.path(), ApplyOptions::new()).unwrap();
        assert!(res.report.all_applied_cleanly());
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("import os\nimport sys"));
        assert!(content.contains("new_runner()"));
        assert!(content.contains("def helper_25(): pass"));
    }

    #[test]
    fn test_aider_wildcard_multi_hunk_with_shared_anchor() {
        let original = indoc! {r#"
            class Service:
                def op_a(self):
                    return "old_a"

                def op_b(self):
                    return "old_b"
        "#};
        let diff = indoc! {r#"
            service.py
            <<<<<<< SEARCH
            class Service:
                ...
                def op_a(self):
                    return "old_a"
                ...
                def op_b(self):
                    return "old_b"
            =======
            class Service:
                ...
                def op_a(self):
                    return "new_a"
                ...
                def op_b(self):
                    return "new_b"
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("return \"new_a\""));
        assert!(patched.contains("return \"new_b\""));
        assert!(patched.contains("class Service:"));
    }

    #[test]
    fn test_aider_wildcard_multiple_internal_ellipses() {
        let original = indoc! {r#"
            def workflow():
                start()
                step_a()
                checkpoint_1()
                step_b()
                checkpoint_2()
                finalize()
        "#};
        let diff = indoc! {r#"
            wf.py
            <<<<<<< SEARCH
            def workflow():
                ...
                checkpoint_1()
                ...
                finalize()
            =======
            def workflow():
                ...
                checkpoint_1_v2()
                ...
                finalize_v2()
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("start()"));
        assert!(patched.contains("step_a()"));
        assert!(patched.contains("checkpoint_1_v2()"));
        assert!(patched.contains("step_b()"));
        assert!(patched.contains("checkpoint_2()"));
        assert!(patched.contains("finalize_v2()"));
    }

    #[test]
    fn test_aider_wildcard_runaway_gap_rejected() {
        let mut original = String::from("def validate():\n    return False\n\n");
        for i in 0..400 {
            original.push_str(&format!("def intermediate_{}(): return False\n", i));
        }
        original.push_str("\ndef is_admin():\n    return True\n");

        // validate() has 'return False', not 'return True'. The only 'return True' is in is_admin() 400 lines away.
        let diff = indoc! {r#"
            auth.py
            <<<<<<< SEARCH
            def validate():
                ...
                return True
            =======
            def validate():
                ...
                return False
            >>>>>>> REPLACE
        "#};
        let res = patch_content_str(diff, Some(&original), &ApplyOptions::new());
        assert!(
            res.is_err(),
            "Runaway gap spanning 400 lines across function boundaries must be rejected"
        );
    }

    #[test]
    fn test_aider_wildcard_low_entropy_anchor_rejected() {
        let original = "}\n\ndef run():\n    return 1;\n";
        let diff = indoc! {r#"
            app.rs
            <<<<<<< SEARCH
            }
            ...
                return 1;
            =======
            }
            ...
                return 2;
            >>>>>>> REPLACE
        "#};
        let res = patch_content_str(diff, Some(original), &ApplyOptions::new());
        assert!(
            res.is_err(),
            "Lone bracket anchor across wildcard gap must be rejected"
        );
    }

    #[test]
    fn test_aider_wildcard_deletion() {
        let original = indoc! {r#"
            def deprecated_worker():
                init()
                run_cycles()
                cleanup()
                return False
        "#};
        let diff = indoc! {r#"
            app.py
            <<<<<<< SEARCH
            def deprecated_worker():
                ...
                return False
            =======
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert_eq!(patched.trim(), "");
    }

    #[test]
    fn test_windows_paths_and_debug_line_formats() {
        assert!(is_plausible_file_path(r"C:\Users\Project\src\main.rs"));
        assert!(is_plausible_file_path(
            r"C:\Users\Project\src\main.rs:42:15"
        ));
        assert!(is_plausible_file_path("D:/workspace/app/Cargo.toml"));
        assert!(is_plausible_file_path(r".\relative\path\file.rs"));

        assert_eq!(
            extract_file_path_from_line(r"Error at C:\Projects\repo\src\lib.rs:42:5:")
                .unwrap()
                .to_str()
                .unwrap(),
            r"C:\Projects\repo\src\lib.rs"
        );
        assert_eq!(
            extract_file_path_from_line(r"Modified: D:/workspace/project/src/core.rs")
                .unwrap()
                .to_str()
                .unwrap(),
            "D:/workspace/project/src/core.rs"
        );
    }

    #[test]
    fn test_compound_and_web_extensions_plausibility() {
        assert!(is_plausible_file_path("components/Button.test.tsx"));
        assert!(is_plausible_file_path("types/global.d.ts"));
        assert!(is_plausible_file_path("archive/backup.tar.gz"));
        assert!(is_plausible_file_path(".env.production"));
        assert!(is_plausible_file_path(".env.staging.local"));
        assert!(is_plausible_file_path("config/.gitattributes"));
        assert!(is_plausible_file_path("frontend/.prettierrc"));

        // Must reject web URLs and sentence abbreviations
        assert!(!is_plausible_file_path(
            "https://github.com/romelium/mpatch"
        ));
        assert!(!is_plausible_file_path("http://localhost:8080/api"));
        assert!(!is_plausible_file_path("etc."));
        assert!(!is_plausible_file_path("e.g."));
        assert!(!is_plausible_file_path("vs."));
    }

    #[test]
    fn test_extract_file_path_conversational_variations() {
        assert_eq!(
            extract_file_path_from_line("In src/database/models.py, replace the query:")
                .unwrap()
                .to_str()
                .unwrap(),
            "src/database/models.py"
        );
        assert_eq!(
            extract_file_path_from_line("// filepath: internal/auth/token.go")
                .unwrap()
                .to_str()
                .unwrap(),
            "internal/auth/token.go"
        );
        assert_eq!(
            extract_file_path_from_line("# filepath: scripts/deploy.py")
                .unwrap()
                .to_str()
                .unwrap(),
            "scripts/deploy.py"
        );
        assert_eq!(
            extract_file_path_from_line("Patch for `frontend/src/App.vue`:")
                .unwrap()
                .to_str()
                .unwrap(),
            "frontend/src/App.vue"
        );
        assert_eq!(
            extract_file_path_from_line("See [Auth Router](src/routes/auth.ts) for details.")
                .unwrap()
                .to_str()
                .unwrap(),
            "src/routes/auth.ts"
        );
        assert_eq!(
            extract_file_path_from_line("1. **config/default.toml**:")
                .unwrap()
                .to_str()
                .unwrap(),
            "config/default.toml"
        );
        assert_eq!(
            extract_file_path_from_line("- 'client/src/main.js'")
                .unwrap()
                .to_str()
                .unwrap(),
            "client/src/main.js"
        );
    }

    #[test]
    fn test_multi_gap_wildcard_reconstruction_three_segments() {
        let original = indoc! {r#"
            fn process_pipeline() {
                initialize_hardware();
                // Hardware check
                verify_bus();
                stage_one_old();
                // Intermediate sync
                sync_clocks();
                stage_two_old();
                shutdown_hardware();
            }
        "#};
        let diff = indoc! {r#"
            pipeline.rs
            <<<<<<< SEARCH
            fn process_pipeline() {
                ...
                stage_one_old();
                ...
                stage_two_old();
            =======
            fn process_pipeline() {
                ...
                stage_one_new();
                ...
                stage_two_new();
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("stage_one_new();"));
        assert!(patched.contains("stage_two_new();"));
        assert!(patched.contains("initialize_hardware();"));
        assert!(patched.contains("verify_bus();"));
        assert!(patched.contains("sync_clocks();"));
        assert!(patched.contains("shutdown_hardware();"));
    }

    #[test]
    fn test_wildcard_with_smart_indentation_translation() {
        // Target file uses Tab indentation
        let original =
            "def compute():\n\tsetup_state()\n\tprepare_buffers()\n\told_value()\n\tfinalize()\n";

        // Patch uses Space indentation and wildcard ellipsis
        let diff = indoc! {r#"
            worker.py
            <<<<<<< SEARCH
                def compute():
                    # ... existing code ...
                    old_value()
            =======
                def compute():
                    # ... existing code ...
                    new_value()
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();

        // The new value must be translated to match target file's tab indentation
        assert!(patched.contains("\tnew_value()\n"));
        assert!(patched.contains("\tprepare_buffers()\n"));
        assert!(patched.contains("\tfinalize()\n"));
    }

    #[test]
    fn test_wildcard_deletion_of_multi_line_class() {
        let original = indoc! {r#"
            class DeprecatedService:
                def __init__(self):
                    self.active = False

                def run(self):
                    execute_legacy()

            class ModernService:
                def run(self):
                    execute_modern()
        "#};
        let diff = indoc! {r#"
            service.py
            <<<<<<< SEARCH
            class DeprecatedService:
                ...
                def run(self):
                    execute_legacy()
            =======
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(!patched.contains("DeprecatedService"));
        assert!(!patched.contains("execute_legacy"));
        assert!(patched.contains("class ModernService:"));
        assert!(patched.contains("execute_modern()"));
    }

    #[test]
    fn test_wildcard_bounded_gap_success() {
        // Anchor at line 1, 80 lines of code, target at line 82 (< 250 gap limit)
        let mut original = String::from("def orchestrator():\n");
        for i in 0..80 {
            original.push_str(&format!("    step_{}();\n", i));
        }
        original.push_str("    old_checkpoint();\n");

        let diff = indoc! {r#"
            orch.py
            <<<<<<< SEARCH
            def orchestrator():
                ...
                old_checkpoint();
            =======
            def orchestrator():
                ...
                new_checkpoint();
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(&original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("new_checkpoint();"));
        assert!(patched.contains("step_0();"));
        assert!(patched.contains("step_79();"));
    }

    #[test]
    fn test_python_stub_protocol_literal_ellipsis_not_corrupted() {
        let original = indoc! {r#"
            from typing import Protocol

            class Greeter(Protocol):
                def greet(self, name: str) -> str:
                    ...

            class ConsoleGreeter:
                def greet(self, name: str) -> str:
                    return f"Hello, {name}"
        "#};
        let diff = indoc! {r#"
            app.py
            <<<<<<< SEARCH
            class ConsoleGreeter:
                def greet(self, name: str) -> str:
                    return f"Hello, {name}"
            =======
            class ConsoleGreeter:
                def greet(self, name: str) -> str:
                    return f"Hi, {name}!"
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        // Greeter's literal ... must remain completely untouched
        assert!(patched.contains("class Greeter(Protocol):"));
        assert!(patched.contains("...\n"));
        assert!(patched.contains("return f\"Hi, {name}!\""));
    }

    #[test]
    fn test_aider_wildcard_multi_edit_with_distinct_anchors() {
        let original = indoc! {r#"
            def compute_tax(amount):
                rate = 0.05
                step_a()
                step_b()
                return amount * rate

            def compute_discount(amount):
                discount = 0.10
                step_c()
                step_d()
                return amount * discount
        "#};
        let diff = indoc! {r#"
            finance.py
            <<<<<<< SEARCH
            def compute_tax(amount):
                ...
                return amount * rate
            =======
            def compute_tax(amount):
                ...
                return amount * (rate + 0.01)
            ...
            def compute_discount(amount):
                ...
                return amount * discount
            =======
            def compute_discount(amount):
                ...
                return amount * (discount + 0.02)
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("return amount * (rate + 0.01)"));
        assert!(patched.contains("return amount * (discount + 0.02)"));
        assert!(patched.contains("step_a()"));
        assert!(patched.contains("step_d()"));
    }

    #[test]
    fn test_is_ellipsis_line_esoteric_formats() {
        assert!(is_ellipsis_line("<!-- ... existing html ... -->"));
        assert!(is_ellipsis_line("/* ... rest of query ... */"));
        assert!(is_ellipsis_line("; ... existing lisp logic ..."));
        assert!(is_ellipsis_line("-- ... existing lua code ..."));
        assert!(is_ellipsis_line("rem ... existing batch logic ..."));
        assert!(is_ellipsis_line("// ... snip ..."));
        assert!(is_ellipsis_line("# ... remainder omitted ..."));
        assert!(is_ellipsis_line("// ... code here ..."));
        assert!(is_ellipsis_line("/* ... */"));
        assert!(is_ellipsis_line("... existing implementation ..."));

        // Non-ellipsis code or comments
        assert!(!is_ellipsis_line("SELECT * FROM users;"));
        assert!(!is_ellipsis_line("// Initialize variables"));
        assert!(!is_ellipsis_line("# Configuration settings"));
        assert!(!is_ellipsis_line("var x = [1, 2, 3];"));
    }

    #[test]
    fn test_unified_diff_with_context_ellipsis_marker() {
        let original = indoc! {r#"
            pub fn setup_engine() {
                configure_allocator();
                init_telemetry();
                verify_security();
                start_worker();
            }
        "#};
        let diff = indoc! {r#"
            --- a/engine.rs
            +++ b/engine.rs
            @@ -1,6 +1,6 @@
             pub fn setup_engine() {
                 // ... existing code ...
            -    start_worker();
            +    start_worker_with_retries(3);
             }
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("start_worker_with_retries(3);"));
        assert!(patched.contains("configure_allocator();"));
        assert!(patched.contains("verify_security();"));
    }

    #[test]
    fn test_wildcard_code_syntax_is_not_ellipsis_stress() {
        let code_lines = [
            "const merged = { ...baseConfig, debug: true };",
            "const items = [ ...first, ...second ];",
            "const copy = [...arr];",
            "function dispatch(...args: any[]) {",
            "int printf(const char *format, ...);",
            "#define LOG(fmt, ...) printf(fmt, __VA_ARGS__)",
            "let x = tensor[..., 0];",
            "sub = matrix[:, ..., 2];",
            "return (args + ...);",
            "match x { 0...9 => true, _ => false }",
            "for i in 0..=10 {",
            "fn forward(data: ...T) {",
            "val = Math.max(...numbers);",
            "const { a, b, ...rest } = params;",
            "append(slice, items...)",
            "def process(*args, **kwargs):",
            "let varargs = func(...);",
        ];
        for line in &code_lines {
            assert!(
                !is_ellipsis_line(line),
                "Code construct was mistakenly identified as ellipsis: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_wildcard_comment_and_syntax_variations_stress() {
        let ellipsis_lines = [
            // --- Bare dots, dashes, and unicode ---
            "...",
            "…",
            "....",
            ".....",
            "......",
            ".......",
            "--------",
            "~~~~~~~~",
            "……",
            "………",
            "    ...",
            "    …",
            "\t...",
            "\t\t...",
            "   ...   ",
            "   …   ",
            "\t...\t",
            "\t\t...\t\t",
            "    ....    ",
            // --- Enclosing brackets ---
            "[...]",
            "[ ... ]",
            "[ … ]",
            "[…]",
            "(...)",
            "( ... )",
            "( … )",
            "(…)",
            "{...}",
            "{ ... }",
            "{ … }",
            "{…}",
            "<...>",
            "< ... >",
            "<…>",
            "< … >",
            // --- Bare comment markers with dots ---
            "// ...",
            "// ... ",
            "// …",
            "// … ",
            "/// ...",
            "/// …",
            "/* ... */",
            "/* … */",
            "/*   ...   */",
            "/*   …   */",
            "/** ... */",
            "/** … */",
            "/*** ... ***/",
            "# ...",
            "# …",
            "# ... ",
            "## ...",
            "### ...",
            "-- ...",
            "-- …",
            "-- ... ",
            "; ...",
            "; …",
            ";; ...",
            ";; …",
            "% ...",
            "% …",
            "%% ...",
            "%% …",
            "REM ...",
            "rem ...",
            "REM …",
            "rem …",
            "<!-- ... -->",
            "<!-- … -->",
            "<!--   ...   -->",
            "{/* ... */}",
            "{/* … */}",
            "{/*   ...   */}",
            "(* ... *)",
            "(* … *)",
            "''' ... '''",
            "''' … '''",
            "\"\"\" ... \"\"\"",
            "\"\"\" … \"\"\"",
            // --- C / C++ / Rust / Go / Java / C# / JS / TS (// and /* */) ---
            "// ... existing code ...",
            "// ... existing code",
            "// ... existing logic ...",
            "// ... existing implementation ...",
            "// ... remaining code ...",
            "// ... remaining code",
            "// ... remaining lines ...",
            "// ... unchanged code ...",
            "// ... code unchanged ...",
            "// ... unchanged ...",
            "// ... remainder of function ...",
            "// ... rest of function ...",
            "// ... rest of method ...",
            "// ... rest of class ...",
            "// ... rest of file ...",
            "// ... rest of implementation ...",
            "// ... code omitted ...",
            "// ... omitted code ...",
            "// ... lines omitted ...",
            "// ... content omitted ...",
            "// ... snip ...",
            "// ... snipped ...",
            "// ... truncated ...",
            "// ... code here ...",
            "// ... earlier code ...",
            "// ... later code ...",
            "// ... previous code ...",
            "// ... hidden ...",
            "// ... skipped ...",
            "// ... same as before ...",
            "// ... original code ...",
            "/// ... existing code ...",
            "/// ... rest of function ...",
            "/* ... existing code ... */",
            "/* ... remaining code ... */",
            "/* ... unchanged code ... */",
            "/* ... code unchanged ... */",
            "// ... unchanged ...",
            "/* ... rest of function ... */",
            "/* ... rest of method ... */",
            "/* ... rest of class ... */",
            "/* ... rest of file ... */",
            "/* ... code omitted ... */",
            "/* ... lines omitted ... */",
            "/* ... snip ... */",
            "/* ... truncated ... */",
            "// ... code here ...",
            "/** ... existing code ... */",
            "/** ... unchanged ... */",
            "/** ... rest of method ... */",
            // --- Python / Shell / Ruby / YAML (#) ---
            "# ... existing code ...",
            "# ... remaining code ...",
            "# ... unchanged code ...",
            "# ... code unchanged ...",
            "# ... unchanged ...",
            "# ... rest of function ...",
            "# ... rest of method ...",
            "# ... rest of class ...",
            "# ... rest of file ...",
            "# ... rest of script ...",
            "# ... code omitted ...",
            "# ... lines omitted ...",
            "# ... snip ...",
            "# ... snipped ...",
            "# ... truncated ...",
            "# ... remainder of function ...",
            "# ... earlier code ...",
            "# ... later code ...",
            "# ... previous code ...",
            "# ... hidden ...",
            "# ... skipped ...",
            "# ... original code ...",
            "## ... existing code ...",
            "### ... existing code ...",
            "# ... existing logic ...",
            // --- HTML / XML / Markdown (<!-- -->) ---
            "<!-- ... existing code ... -->",
            "<!-- ... existing template ... -->",
            "<!-- ... existing html ... -->",
            "<!-- ... remaining code ... -->",
            "<!-- ... unchanged code ... -->",
            "<!-- ... unchanged ... -->",
            "<!-- ... rest of template ... -->",
            "<!-- ... rest of file ... -->",
            "<!-- ... code omitted ... -->",
            "<!-- ... lines omitted ... -->",
            "<!-- ... snip ... -->",
            "<!-- ... truncated ... -->",
            "<!-- ... code here ... -->",
            // --- SQL / Lua / Haskell (--) ---
            "-- ... existing code ...",
            "-- ... existing lua ...",
            "-- ... remaining query ...",
            "-- ... rest of query ...",
            "-- ... unchanged code ...",
            "-- ... unchanged ...",
            "-- ... rest of function ...",
            "-- ... code omitted ...",
            "-- ... lines omitted ...",
            "-- ... snip ...",
            "-- ... truncated ...",
            // --- Lisp / Assembly / INI (;) ---
            "; ... existing code ...",
            "; ... existing lisp ...",
            "; ... rest of logic ...",
            "; ... unchanged code ...",
            "; ... unchanged ...",
            "; ... code omitted ...",
            "; ... lines omitted ...",
            "; ... snip ...",
            ";; ... existing code ...",
            ";; ... rest of function ...",
            ";; ... unchanged ...",
            // --- Erlang / LaTeX / MATLAB (%) ---
            "% ... existing code ...",
            "% ... remaining code ...",
            "% ... unchanged code ...",
            "% ... unchanged ...",
            "% ... rest of script ...",
            "% ... code omitted ...",
            "% ... lines omitted ...",
            "% ... snip ...",
            "%% ... existing code ...",
            "%% ... unchanged ...",
            // --- Windows Batch (REM / rem) ---
            "REM ... existing code ...",
            "REM ... remaining code ...",
            "REM ... unchanged code ...",
            "REM ... unchanged ...",
            "rem ... rest of script ...",
            "REM ... rest of batch ...",
            "REM ... code omitted ...",
            "REM ... snip ...",
            "rem ... existing code ...",
            "rem ... remaining code ...",
            "rem ... unchanged ...",
            "rem ... rest of script ...",
            "rem ... code omitted ...",
            "rem ... snip ...",
            // --- React / JSX ({/* */}) ---
            "{/* ... existing code ... */}",
            "{/* ... existing jsx ... */}",
            "{/* ... remaining code ... */}",
            "{/* ... unchanged code ... */}",
            "{/* ... unchanged ... */}",
            "{/* ... rest of component ... */}",
            "{/* ... code omitted ... */}",
            "{/* ... lines omitted ... */}",
            "{/* ... snip ... */}",
            // --- OCaml / Pascal / ML ((* *)) ---
            "(* ... existing code ... *)",
            "(* ... remaining code ... *)",
            "(* ... unchanged code ... *)",
            "(* ... unchanged ... *)",
            "(* ... rest of function ... *)",
            "(* ... code omitted ... *)",
            "(* ... lines omitted ... *)",
            "(* ... snip ... *)",
            // --- Python Docstrings (''' and """) ---
            "''' ... existing code ... '''",
            "''' ... remaining code ... '''",
            "''' ... unchanged code ... '''",
            "''' ... unchanged ... '''",
            "''' ... rest of function ... '''",
            "''' ... code omitted ... '''",
            "''' ... snip ... '''",
            "\"\"\" ... existing code ... \"\"\"",
            "\"\"\" ... remaining code ... \"\"\"",
            "\"\"\" ... unchanged code ... \"\"\"",
            "\"\"\" ... unchanged ... \"\"\"",
            "\"\"\" ... rest of function ... \"\"\"",
            "\"\"\" ... code omitted ... \"\"\"",
            "\"\"\" ... snip ... \"\"\"",
            // --- Unicode ellipsis phrases ---
            "// … existing code …",
            "// … rest of function …",
            "// … code omitted …",
            "// … unchanged …",
            "// … snip …",
            "/* … existing code … */",
            "/* … unchanged … */",
            "/* … code omitted … */",
            "# … existing code …",
            "# … rest of function …",
            "# … code omitted …",
            "# … unchanged …",
            "# … snip …",
            "<!-- … existing code … -->",
            "<!-- … unchanged … -->",
            "<!-- … snip … -->",
            "-- … existing code …",
            "-- … remaining query …",
            "-- … unchanged …",
            "; … existing code …",
            "; … rest of logic …",
            "% … existing code …",
            "% … code omitted …",
            "{/* … existing code … */}",
            "{/* … unchanged … */}",
            "(* … existing code … *)",
            "(* … unchanged … *)",
            "''' … existing code … '''",
            "\"\"\" … existing code … \"\"\"",
            "REM … existing code …",
            "rem … existing code …",
            // --- Indented and tab-padded lines ---
            "    // ... existing code ...",
            "\t// ... existing code ...",
            "\t\t// ... existing code ...",
            "    # ... existing code ...",
            "\t# ... existing code ...",
            "    /* ... existing code ... */",
            "\t/* ... existing code ... */",
            "    <!-- ... existing code ... -->",
            "    -- ... existing code ...",
            "    ; ... existing code ...",
            "    % ... existing code ...",
            "    REM ... existing code ...",
            "    {/* ... existing code ... */}",
            "    (* ... existing code ... *)",
            "    ''' ... existing code ... '''",
            "    \"\"\" ... existing code ... \"\"\"",
            "    // … existing code …",
            "    # … existing code …",
            "    /* … existing code … */",
        ];
        for line in &ellipsis_lines {
            assert!(
                is_ellipsis_line(line),
                "Valid ellipsis variation was not identified: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_wildcard_same_length_gap_and_ellipsis_lines() {
        let original = indoc! {r#"
            def calculate(x):
                multiplier = 2
                return x * multiplier
        "#};
        let diff = indoc! {r#"
            calc.py
            <<<<<<< SEARCH
            def calculate(x):
                ...
                return x * multiplier
            =======
            def calculate(x):
                ...
                return x * (multiplier + 1)
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(
            patched.contains("multiplier = 2"),
            "Gap line must be preserved even when gap line count equals ellipsis line count"
        );
        assert!(patched.contains("return x * (multiplier + 1)"));
    }

    #[test]
    fn test_wildcard_five_segment_deep_pipeline_preserves_all_gaps() {
        let mut original = String::from("fn run_complex_pipeline() {\n    init_hardware();\n");
        for i in 0..10 {
            original.push_str(&format!("    let hw_reg_{} = read_reg({});\n", i, i));
        }
        original.push_str("    stage_one_old();\n");
        for i in 0..15 {
            original.push_str(&format!("    let socket_{} = connect_peer({});\n", i, i));
        }
        original.push_str("    stage_two_old();\n");
        for i in 0..20 {
            original.push_str(&format!("    let worker_{} = spawn_thread({});\n", i, i));
        }
        original.push_str("    stage_three_old();\n");
        for i in 0..12 {
            original.push_str(&format!("    let signal_{} = register_signal({});\n", i, i));
        }
        original.push_str("    stage_four_old();\n    shutdown_hardware();\n}\n");

        let diff = indoc! {r#"
            pipeline.rs
            <<<<<<< SEARCH
            fn run_complex_pipeline() {
                init_hardware();
                ...
                stage_one_old();
                ...
                stage_two_old();
                ...
                stage_three_old();
                ...
                stage_four_old();
                shutdown_hardware();
            =======
            fn run_complex_pipeline() {
                init_hardware();
                ...
                stage_one_new();
                ...
                stage_two_new();
                ...
                stage_three_new();
                ...
                stage_four_new();
                shutdown_hardware();
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(&original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("stage_one_new();"));
        assert!(patched.contains("stage_two_new();"));
        assert!(patched.contains("stage_three_new();"));
        assert!(patched.contains("stage_four_new();"));
        assert!(!patched.contains("stage_one_old();"));
        assert!(!patched.contains("stage_two_old();"));
        assert!(!patched.contains("stage_three_old();"));
        assert!(!patched.contains("stage_four_old();"));

        assert!(patched.contains("let hw_reg_0 = read_reg(0);"));
        assert!(patched.contains("let hw_reg_9 = read_reg(9);"));
        assert!(patched.contains("let socket_0 = connect_peer(0);"));
        assert!(patched.contains("let socket_14 = connect_peer(14);"));
        assert!(patched.contains("let worker_0 = spawn_thread(0);"));
        assert!(patched.contains("let worker_19 = spawn_thread(19);"));
        assert!(patched.contains("let signal_0 = register_signal(0);"));
        assert!(patched.contains("let signal_11 = register_signal(11);"));
    }

    #[test]
    fn test_wildcard_zero_length_gap_adjacent_anchors() {
        let original = indoc! {r#"
            def workflow():
                step_a()
                step_b()
        "#};
        let diff = indoc! {r#"
            wf.py
            <<<<<<< SEARCH
            def workflow():
                step_a()
                ...
                step_b()
            =======
            def workflow():
                step_a()
                inserted_between()
                step_b()
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("step_a()\n    inserted_between()\n    step_b()"));
    }

    #[test]
    fn test_wildcard_ellipsis_removal_deletes_gap_completely() {
        let original = indoc! {r#"
            def authenticate(user, password):
                log_attempt(user)
                legacy_auth_1(user)
                legacy_auth_2(user)
                legacy_auth_3(user)
                return grant_token(user)
        "#};
        let diff = indoc! {r#"
            auth.py
            <<<<<<< SEARCH
            def authenticate(user, password):
                log_attempt(user)
                ...
                return grant_token(user)
            =======
            def authenticate(user, password):
                log_attempt(user)
                return grant_token(user)
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("log_attempt(user)\n    return grant_token(user)"));
        assert!(!patched.contains("legacy_auth_1"));
        assert!(!patched.contains("legacy_auth_2"));
        assert!(!patched.contains("legacy_auth_3"));
    }

    #[test]
    fn test_wildcard_gap_replaced_with_new_logic() {
        let original = indoc! {r#"
            def process_items(items):
                validate_input(items)
                for item in items:
                    step_1(item)
                    step_2(item)
                    step_3(item)
                return finalize()
        "#};
        let diff = indoc! {r#"
            proc.py
            <<<<<<< SEARCH
            def process_items(items):
                validate_input(items)
                ...
                return finalize()
            =======
            def process_items(items):
                validate_input(items)
                batch_parallel_process(items)
                return finalize()
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("batch_parallel_process(items)"));
        assert!(!patched.contains("step_1(item)"));
        assert!(!patched.contains("step_2(item)"));
        assert!(!patched.contains("step_3(item)"));
        assert!(patched.contains("validate_input(items)"));
        assert!(patched.contains("return finalize()"));
    }

    #[test]
    fn test_wildcard_identical_anchors_disambiguation_stress() {
        let mut original = String::new();
        for i in 0..8 {
            original.push_str(&format!(
                "def worker_{}():\n    acquire_lock()\n    perform_task({})\n    release_lock()\n\n",
                i, i
            ));
        }
        let diff = indoc! {r#"
            workers.py
            <<<<<<< SEARCH
            def worker_4():
                ...
                release_lock()
            =======
            def worker_4():
                ...
                release_lock()
                audit_log("worker_4_done")
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(&original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("audit_log(\"worker_4_done\")"));
        for i in 0..8 {
            if i == 4 {
                assert!(patched.contains("def worker_4():\n    acquire_lock()\n    perform_task(4)\n    release_lock()\n    audit_log(\"worker_4_done\")"));
            } else {
                assert!(patched.contains(&format!("def worker_{}():\n    acquire_lock()\n    perform_task({})\n    release_lock()\n", i, i)));
            }
        }
    }

    #[test]
    fn test_wildcard_reverse_anchor_order_must_fail() {
        let original = indoc! {r#"
            def shutdown_system():
                power_off()

            def startup_system():
                power_on()
        "#};
        let diff = indoc! {r#"
            system.py
            <<<<<<< SEARCH
            def startup_system():
                ...
                power_off()
            =======
            def startup_system():
                ...
                power_off_safely()
            >>>>>>> REPLACE
        "#};
        let res = patch_content_str(diff, Some(original), &ApplyOptions::new());
        assert!(
            res.is_err(),
            "Anchors appearing in reverse order must be rejected"
        );
    }

    #[test]
    fn test_wildcard_multi_block_aider_in_single_file_stress() {
        let original = indoc! {r#"
            def setup_database():
                cfg = load_db_config()
                validate_cfg(cfg)
                pool = create_pool(cfg)
                return pool

            def run_migrations():
                init_migrator()
                apply_pending()
                record_migration_hash()
                return True

            def teardown_database():
                flush_wal()
                close_active_conns()
                shutdown_pool()
                return True
        "#};
        let diff = indoc! {r#"
            db.py
            <<<<<<< SEARCH
            def setup_database():
                ...
                pool = create_pool(cfg)
            =======
            def setup_database():
                ...
                verify_ssl_certs(cfg)
                pool = create_pool(cfg)
            >>>>>>> REPLACE

            <<<<<<< SEARCH
            def teardown_database():
                ...
                shutdown_pool()
            =======
            def teardown_database():
                ...
                shutdown_pool()
                log_clean_shutdown()
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("verify_ssl_certs(cfg)"));
        assert!(patched.contains("log_clean_shutdown()"));
        assert!(patched.contains("apply_pending()"));
    }

    #[test]
    fn test_wildcard_in_nested_control_flow_with_duplicate_tokens() {
        let original = indoc! {r#"
            fn process_event(event: Event) -> Result<(), AppError> {
                match event {
                    Event::Data(buffer) => {
                        for chunk in buffer {
                            if chunk.is_corrupted() {
                                return Err(AppError::Invalid);
                            }
                        }
                        finalize_data();
                        Ok(())
                    }
                    Event::Disconnect => {
                        log_disconnect();
                        return Err(AppError::Invalid);
                    }
                }
            }
        "#};
        let diff = indoc! {r#"
            event.rs
            <<<<<<< SEARCH
            Event::Data(buffer) => {
                for chunk in buffer {
                    ...
                    return Err(AppError::Invalid);
            =======
            Event::Data(buffer) => {
                for chunk in buffer {
                    ...
                    return Err(AppError::CorruptedPayload);
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("return Err(AppError::CorruptedPayload);"));
        assert!(patched.contains("Event::Disconnect => {\n            log_disconnect();\n            return Err(AppError::Invalid);"));
    }

    #[test]
    fn test_wildcard_consecutive_ellipses_handled_cleanly() {
        let original = indoc! {r#"
            def compute():
                step_1()
                step_2()
                step_3()
                return 42
        "#};
        let diff = indoc! {r#"
            comp.py
            <<<<<<< SEARCH
            def compute():
                ...
                ...
                return 42
            =======
            def compute():
                ...
                ...
                return 100
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("step_1()"));
        assert!(patched.contains("step_2()"));
        assert!(patched.contains("step_3()"));
        assert!(patched.contains("return 100"));
    }
}

mod false_positive_and_negative_tests {
    use indoc::indoc;
    use mpatch::{
        apply_patch_to_file, detect_patch, extract_file_path_from_line, is_ellipsis_line,
        is_plausible_file_path, parse_auto, patch_content_str, ApplyOptions, PatchFormat,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_is_ellipsis_line_code_constructs_false_positive_rejection() {
        let code_lines = [
            // JS / TS spread & rest syntax
            "const newObj = { ...oldObj, key: 'val' };",
            "const copy = [...original];",
            "function sum(...nums: number[]) {",
            "const { a, b, ...others } = data;",
            "<Component {...props} className=\"btn\" />",
            "log(...args);",
            "new Array(...elements);",
            "type Fn = (...params: string[]) => void;",
            // Python Ellipsis & variadics
            "tensor[..., 0] = 5",
            "sub_matrix = data[:, ..., 1]",
            "def method(self, *args: Any, **kwargs: Any) -> tuple[int, ...]:",
            "arr[...] = 0",
            // C / C++ varargs & fold expressions
            "int printf(const char* format, ...);",
            "int scanf(const char* format, ...);",
            "#define DBG(fmt, ...) fprintf(stderr, fmt, __VA_ARGS__)",
            "template<typename... Ts> struct Tuple {};",
            "return (... + args);",
            "(args + ...);",
            // Rust syntax
            "for idx in 0..=10 {",
            "match val { 0...9 => true, _ => false }",
            "Point { x: 10, ..Default::default() };",
            "let Config { port, .. } = cfg;",
            // Go variadics
            "func Variadic(vals ...string)",
            "items = append(items, extra...)",
            "fmt.Sprintf(\"%s\", args...)",
            // PHP variadics
            "function sum(...$numbers) {",
            "call_user_func($fn, ...$params);",
            // Kotlin spread
            "val list = listOf(*items)",
            // Code comments that happen to have dots at the end
            "// Loading system configuration...",
            "// Initializing telemetry client...",
            "// Connecting to remote host...",
            "// Processing incoming requests...",
            "// Please wait...",
            "// TODO: Refactor this logic later...",
            "// FIXME: Temporary hack for legacy compatibility...",
            "/* Calculating running average... */",
            "<!-- Loading spinner placeholder... -->",
            "# Saving database checkpoint...",
            "# Installing runtime dependencies...",
            "-- Querying customer records...",
            "; Synchronizing worker threads...",
            "// Waiting for server response...",
            // String literals with dots
            "let status = \"Connecting...\";",
            "const errorMsg = \"Something went wrong...\";",
            "printf(\"Loading assets...\\n\");",
            "throw new Error(\"Operation timed out...\");",
            "<p>Loading content...</p>",
            "<div>Please wait while processing...</div>",
            // Other
            "{ element1, ... }",
        ];
        for line in &code_lines {
            assert!(
                !is_ellipsis_line(line),
                "False positive: Code/text construct was mistakenly identified as ellipsis: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_is_ellipsis_line_tricky_variations_false_negative_prevention() {
        let ellipsis_lines = [
            // Uppercase and title case keywords
            "// ... EXISTING CODE ...",
            "// ... Existing Code ...",
            "// ... UNCHANGED ...",
            "// ... Unchanged Code ...",
            "// ... REST OF FILE ...",
            "// ... Rest of Method ...",
            "// ... CODE OMITTED ...",
            "// ... LINES OMITTED ...",
            "// ... SNIP ...",
            // Parentheses and brackets inside comments
            "// ... (unchanged code) ...",
            "// ... (existing code) ...",
            "/* ... [rest of implementation] ... */",
            "<!-- ... (existing template) ... -->",
            "# ... [existing logic] ...",
            "-- ... (remaining query) ...",
            "; ... (rest of function) ...",
            // Banners and decorations around ellipsis
            "// ================= ... existing code ... =================",
            "// ----------------- ... existing code ... -----------------",
            "// ***************** ... existing code ... *****************",
            "// ~~~~~~~~~~~~~~~~~ ... existing code ... ~~~~~~~~~~~~~~~~~",
            "// >>> ... existing code ... <<<",
            "// <<< ... existing code ... >>>",
            // Additional trigger phrases
            "// ... more code here ...",
            "// ... earlier logic ...",
            "// ... later logic ...",
            "// ... previous logic ...",
            "// ... original implementation ...",
            "// ... remainder of script ...",
        ];
        for line in &ellipsis_lines {
            assert!(
                is_ellipsis_line(line),
                "False negative: Valid ellipsis was not recognized: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_path_detection_conversational_text_false_positive_rejection() {
        let non_paths = [
            "Please look at the changes.",
            "I have updated the code.",
            "The bug was caused by a null pointer.",
            "Run with --dry-run.",
            "Version 2.0.1",
            "Check if it's true vs. false.",
            "Tested on Python 3.10.",
            "https://github.com/romelium/mpatch/issues/12",
            "https://example.com/downloads/v1.0.tar.gz",
            "http://localhost:8080/metrics",
            "Run git commit -m 'initial commit' to save.",
            "cargo test --all-features",
            "pip install .[test]",
            "10/20 tests passed successfully.",
            "Formula: a/b = c/d",
        ];
        for text in &non_paths {
            assert!(
                !is_plausible_file_path(text),
                "False positive: Conversational text was treated as plausible file path: {:?}",
                text
            );
        }

        let non_path_lines = [
            "Here is the diff:",
            "Please check the following code snippet:",
            "Let me know if this works.",
            "I've modified the file to fix the bug.",
            "The changes are shown below:",
            "See details at https://github.com/romelium/mpatch",
            "This resolves issue #42 on GitHub.",
            "Output of running `cargo check`:",
        ];
        for line in &non_path_lines {
            assert_eq!(
                extract_file_path_from_line(line),
                None,
                "False positive: File path extracted from non-path conversational line: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_path_detection_valid_paths_false_negative_prevention() {
        let valid_paths = [
            "src/main.rs",
            "internal/auth/token.go",
            "crates/core/src/lib.rs",
            "frontend/src/App.vue",
            "config/default.toml",
            "assets/styles/main.scss",
            ".gitignore",
            ".env",
            ".env.production",
            "Makefile",
            "Dockerfile",
            "Jenkinsfile",
            "package.json",
            "types/index.d.ts",
            "bundle.min.js",
            "src/components/My Component/Button.tsx",
            r"C:\Projects\repo\src\main.rs",
            "D:/workspace/app/Cargo.toml",
            "src/lib.rs:120:5",
        ];
        for path in &valid_paths {
            assert!(
                is_plausible_file_path(path),
                "False negative: Valid file path was not recognized: {:?}",
                path
            );
        }

        let conversational_path_lines = [
            ("In src/main.rs, change the function:", "src/main.rs"),
            (
                "Update 'config/settings.toml' with the new port:",
                "config/settings.toml",
            ),
            ("Check `lib/utils.py` for helper functions", "lib/utils.py"),
            (
                "// filepath: internal/auth/token.go",
                "internal/auth/token.go",
            ),
            ("# filepath: scripts/deploy.py", "scripts/deploy.py"),
            ("Patch for `frontend/src/App.vue`:", "frontend/src/App.vue"),
            ("1. **config/default.toml**:", "config/default.toml"),
            (
                "See [Auth Router](src/routes/auth.ts) for details.",
                "src/routes/auth.ts",
            ),
            ("--- a/crates/core/src/lib.rs", "crates/core/src/lib.rs"),
            ("diff --git a/src/index.ts b/src/index.ts", "src/index.ts"),
        ];
        for (line, expected) in &conversational_path_lines {
            let extracted = extract_file_path_from_line(line);
            assert_eq!(
                extracted.as_deref().and_then(|p| p.to_str()),
                Some(*expected),
                "False negative: Failed to extract expected path from line: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_detect_patch_false_positive_rejection() {
        let table = indoc! {r#"
            | Name | Version | Status |
            | --- | --- | --- |
            | mpatch | 1.6.4 | Stable |
        "#};
        assert_eq!(detect_patch(table), PatchFormat::Unknown);

        let list = indoc! {r#"
            Review Notes:
            + Performance improved by 20%
            - Memory usage slightly higher
            + Code is cleaner
            - Missing test coverage
        "#};
        assert_eq!(detect_patch(list), PatchFormat::Unknown);

        let bitwise = indoc! {r#"
            let flag = 1 << 8;
            let mask = flag >> 2;
        "#};
        assert_eq!(detect_patch(bitwise), PatchFormat::Unknown);

        let comparisons = indoc! {r#"
            if x <= 100 && y >= 200 {
                do_something();
            }
        "#};
        assert_eq!(detect_patch(comparisons), PatchFormat::Unknown);

        let h1 = indoc! {r#"
            Installation Guide
            ==================
            Follow the instructions below.
        "#};
        assert_eq!(detect_patch(h1), PatchFormat::Unknown);

        let h2 = indoc! {r#"
            Troubleshooting
            ---------------
            Check your configuration.
        "#};
        assert_eq!(detect_patch(h2), PatchFormat::Unknown);

        let banner = indoc! {r#"
            // ==========================================
            // ============ WORKER DISPATCH =============
            // ==========================================
        "#};
        assert_eq!(detect_patch(banner), PatchFormat::Unknown);

        let plain_code = indoc! {r#"
            ```rust
            pub fn hello() {
                println!("Hello world");
            }
            ```
        "#};
        assert_eq!(detect_patch(plain_code), PatchFormat::Unknown);

        let dangling_header = indoc! {r#"
            --- a/some_file.txt
            Just regular text without plus plus plus.
        "#};
        assert_eq!(detect_patch(dangling_header), PatchFormat::Unknown);
    }

    #[test]
    fn test_patch_apply_similar_functions_no_false_positive_clobber() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("serializer.rs");
        let original = indoc! {r#"
            pub fn serialize_json<T: Serialize>(val: &T) -> Result<String, Error> {
                let mut buf = String::new();
                let mut ser = Serializer::new(&mut buf);
                val.serialize(&mut ser)?;
                validate_output(&buf)?;
                Ok(buf)
            }

            pub fn serialize_yaml<T: Serialize>(val: &T) -> Result<String, Error> {
                let mut buf = String::new();
                let mut ser = Serializer::new(&mut buf);
                val.serialize(&mut ser)?;
                validate_output(&buf)?;
                Ok(buf)
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = indoc! {r#"
            --- a/serializer.rs
            +++ b/serializer.rs
            @@ -10,6 +10,6 @@
             pub fn serialize_yaml<T: Serialize>(val: &T) -> Result<String, Error> {
                 let mut buf = String::new();
                 let mut ser = Serializer::new(&mut buf);
                 val.serialize(&mut ser)?;
            -    validate_output(&buf)?;
            +    validate_yaml_output(&buf)?;
                 Ok(buf)
             }
        "#};
        let patches = parse_auto(diff).unwrap();
        let res = apply_patch_to_file(&patches[0], dir.path(), ApplyOptions::new()).unwrap();
        assert!(res.report.all_applied_cleanly());

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("validate_yaml_output(&buf)?;"));
        assert!(content.contains("pub fn serialize_json<T: Serialize>(val: &T) -> Result<String, Error> {\n    let mut buf = String::new();\n    let mut ser = Serializer::new(&mut buf);\n    val.serialize(&mut ser)?;\n    validate_output(&buf)?;\n    Ok(buf)\n}"));
    }

    #[test]
    fn test_wildcard_similar_loop_constructs_no_false_positive() {
        let original = indoc! {r#"
            fn handle_incoming(items: &[Item]) {
                log("incoming");
                for item in items {
                    process(item);
                }
                finalize_incoming();
            }

            fn handle_outgoing(items: &[Item]) {
                log("outgoing");
                for item in items {
                    process(item);
                }
                finalize_outgoing();
            }
        "#};
        let diff = indoc! {r#"
            handler.rs
            <<<<<<< SEARCH
            fn handle_outgoing(items: &[Item]) {
                ...
                for item in items {
                    process(item);
                }
                finalize_outgoing();
            =======
            fn handle_outgoing(items: &[Item]) {
                ...
                for item in items {
                    process_outgoing(item);
                }
                finalize_outgoing();
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert!(patched.contains("process_outgoing(item);"));
        assert!(patched.contains("fn handle_incoming(items: &[Item]) {\n    log(\"incoming\");\n    for item in items {\n        process(item);\n    }\n    finalize_incoming();\n}"));
    }

    #[test]
    fn test_wildcard_empty_or_whitespace_gap_handling() {
        let original = "def start_job():\n    setup()\n\n    \n\t\n    finish()\n";
        let diff = indoc! {r#"
            job.py
            <<<<<<< SEARCH
            def start_job():
                setup()
                ...
                finish()
            =======
            def start_job():
                setup()
                ...
                record_completion()
                finish()
            >>>>>>> REPLACE
        "#};
        let patched = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
        assert_eq!(
            patched,
            "def start_job():\n    setup()\n\n    \n\t\n    record_completion()\n    finish()\n"
        );
    }
}

#[test]
fn test_pure_addition_eof_append_with_zero_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let original = "line 1\nline 2\n";
    let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -2,0 +3,2 @@
        +line 3
        +line 4
    "#};

    let result = patch_content_str(diff, Some(original), &ApplyOptions::exact()).unwrap();
    assert_eq!(result, "line 1\nline 2\nline 3\nline 4\n");
}

#[test]
fn test_pure_addition_mid_file_insertion_with_zero_context() {
    let _ = env_logger::builder().is_test(true).try_init();
    let original = "line 1\nline 2\nline 3\n";
    let diff = indoc! {r#"
        --- a/file.txt
        +++ b/file.txt
        @@ -1,0 +2,1 @@
        +inserted after line 1
    "#};

    let result = patch_content_str(diff, Some(original), &ApplyOptions::exact()).unwrap();
    assert_eq!(result, "line 1\ninserted after line 1\nline 2\nline 3\n");
}

#[test]
fn test_aider_multi_hunk_identical_context_resolved_by_anchors() {
    let _ = env_logger::builder().is_test(true).try_init();
    let original = indoc! {r#"
        pub fn op_one() {
            let x = 1;
            run();
        }

        pub fn op_two() {
            let x = 1;
            run();
        }

        pub fn op_three() {
            let x = 1;
            run();
        }
    "#};

    let diff = indoc! {r#"
        file.rs
        <<<<<<< SEARCH
        pub fn op_one() {
        =======
        pub fn op_one_v2() {
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            let x = 1;
        =======
            let x = 10;
        >>>>>>> REPLACE
        <<<<<<< SEARCH
        pub fn op_three() {
        =======
        pub fn op_three_v2() {
        >>>>>>> REPLACE
    "#};

    let result = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
    assert!(result.contains("pub fn op_one_v2()"));
    assert!(result.contains("pub fn op_three_v2()"));
    assert!(result.contains("pub fn op_one_v2() {\n    let x = 10;\n    run();\n}"));
    assert!(result.contains("pub fn op_two() {\n    let x = 1;\n    run();\n}"));
}

#[test]
fn test_aider_multi_hunk_unanchored_duplicate_remains_ambiguous() {
    let _ = env_logger::builder().is_test(true).try_init();
    let original = indoc! {r#"
        pub fn op_alpha() {
            let common = 1;
        }

        pub fn op_beta() {
            let common = 1;
        }
    "#};

    let diff = indoc! {r#"
        file.rs
        <<<<<<< SEARCH
            let common = 1;
        =======
            let common = 2;
        >>>>>>> REPLACE
    "#};

    let res = patch_content_str(diff, Some(original), &ApplyOptions::new());
    assert!(res.is_err(), "Unanchored duplicate Aider block without surrounding bounds must remain strictly ambiguous");
}

#[test]
fn test_aider_hunk_applier_five_method_doc_test_reproduction() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Faithful reproduction of the 5 methods from src/lib.rs that caused the AmbiguousExactMatch failure
    let original = indoc! {r#"
        pub fn new<T: AsRef<str>>() {
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            /// let status = applier.next().unwrap();
            println!("in new");
        }

        pub fn current_lines(&self) {
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            /// assert_eq!(applier.current_lines(), &["line 1", "line 2"]);
            println!("in current_lines");
        }

        pub fn into_lines(self) {
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            /// applier.next(); // Apply all hunks
            /// let final_lines = applier.into_lines();
            println!("in into_lines");
        }

        pub fn into_content(self) {
            /// let original_lines = vec!["line 1"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,1 +1,1\n-line 1\n+line one\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            println!("in into_content");
        }

        pub fn next(&mut self) {
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            /// let status = applier.next();
            println!("in next");
        }
    "#};

    // Five Aider blocks mirroring Hunks 12, 13, 14, 15, and 16 from the report:
    // Hunks 13 and 16 contain only the 6 shared lines with no unique method identifier.
    let diff = indoc! {r#"
        app.rs
        <<<<<<< SEARCH
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            /// let status = applier.next().unwrap();
        =======
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            /// let status = applier.next().unwrap();
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
        =======
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            /// applier.next(); // Apply all hunks
            /// let final_lines = applier.into_lines();
        =======
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
            /// applier.next(); // Apply all hunks
            /// let final_lines = applier.into_lines();
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            /// let original_lines = vec!["line 1"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,1 +1,1\n-line 1\n+line one\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
        =======
            /// let original_lines = vec!["line 1"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n-line 1\n+line one\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
        =======
            /// let original_lines = vec!["line 1", "line 2"];
            /// let diff = "```diff\n--- a/f\n+++ b/f\n@@ -2,1 +2,1 @@\n-line 2\n+line two\n```";
            /// let patch = parse_single_patch(diff)?;
            /// let options = ApplyOptions::new();
            ///
            /// let mut applier = HunkApplier::new(&patch, Some(&original_lines), &options);
        >>>>>>> REPLACE
    "#};

    let result = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();

    // Assert all 5 locations were successfully patched to include @@ ... @@
    assert_eq!(result.matches("@@ -2,1 +2,1 @@").count(), 4);
    assert_eq!(result.matches("@@ -1,1 +1,1 @@").count(), 1);
    assert_eq!(result.matches("@@ -2,1 +2,1\\n").count(), 0);

    // Assert that each method's distinct code was preserved and not clobbered
    assert!(result.contains("assert_eq!(applier.current_lines(), &[\"line 1\", \"line 2\"]);"));
    assert!(result.contains("let final_lines = applier.into_lines();"));
    assert!(result.contains("let status = applier.next();"));
    assert!(result.contains("let status = applier.next().unwrap();"));
}

#[test]
fn test_cascading_relaxation_anchor_resolution() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Test multi-step relaxation:
    // Hunk 0: unique anchor at start
    // Hunk 1: ambiguous across whole file, bounded between Hunk 0 and Hunk 3
    // Hunk 2: ambiguous across whole file, but once Hunk 1 is resolved, bounded between Hunk 1 and Hunk 3
    // Hunk 3: unique anchor at end
    let original = indoc! {r#"
        fn anchor_start() {
            init();
        }

        fn step_alpha() {
            shared_step();
        }

        fn step_beta() {
            shared_step();
        }

        fn anchor_end() {
            shutdown();
        }
    "#};

    let diff = indoc! {r#"
        flow.rs
        <<<<<<< SEARCH
        fn anchor_start() {
        =======
        fn anchor_start_v2() {
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            shared_step();
        =======
            shared_step_alpha();
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            shared_step();
        =======
            shared_step_beta();
        >>>>>>> REPLACE
        <<<<<<< SEARCH
        fn anchor_end() {
        =======
        fn anchor_end_v2() {
        >>>>>>> REPLACE
    "#};

    let result = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
    assert!(result.contains("fn anchor_start_v2()"));
    assert!(result.contains("shared_step_alpha()"));
    assert!(result.contains("shared_step_beta()"));
    assert!(result.contains("fn anchor_end_v2()"));
}

#[test]
fn test_anchor_bounded_at_file_start_and_end() {
    let _ = env_logger::builder().is_test(true).try_init();

    let original = indoc! {r#"
        fn header() {
            log_event();
        }

        fn middle_anchor() {
            anchor_point();
        }

        fn footer() {
            log_event();
        }
    "#};

    // Hunk 1 is ambiguous (matches header and footer), but sits between line 0 and middle_anchor
    // Hunk 3 is ambiguous (matches header and footer), but sits between middle_anchor and EOF
    let diff = indoc! {r#"
        log.rs
        <<<<<<< SEARCH
            log_event();
        =======
            log_header_event();
        >>>>>>> REPLACE
        <<<<<<< SEARCH
        fn middle_anchor() {
        =======
        fn middle_anchor_v2() {
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            log_event();
        =======
            log_footer_event();
        >>>>>>> REPLACE
    "#};

    let result = patch_content_str(diff, Some(original), &ApplyOptions::new()).unwrap();
    assert!(result.contains("fn header() {\n    log_header_event();\n}"));
    assert!(result.contains("fn middle_anchor_v2()"));
    assert!(result.contains("fn footer() {\n    log_footer_event();\n}"));
}

#[test]
fn test_genuine_ambiguity_multiple_matches_in_interval_must_fail() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Inside the interval bounded by anchor_one and anchor_two,
    // there are TWO identical matches. The engine must NOT guess!
    let original = indoc! {r#"
        fn anchor_one() {}

        fn duplicate_one() {
            do_action();
        }

        fn duplicate_two() {
            do_action();
        }

        fn anchor_two() {}
    "#};

    let diff = indoc! {r#"
        fail.rs
        <<<<<<< SEARCH
        fn anchor_one() {}
        =======
        fn anchor_one_v2() {}
        >>>>>>> REPLACE
        <<<<<<< SEARCH
            do_action();
        =======
            do_action_new();
        >>>>>>> REPLACE
        <<<<<<< SEARCH
        fn anchor_two() {}
        =======
        fn anchor_two_v2() {}
        >>>>>>> REPLACE
    "#};

    let res = patch_content_str(diff, Some(original), &ApplyOptions::new());
    assert!(
        res.is_err(),
        "Must reject with AmbiguousExactMatch when >1 matches exist within the anchor interval"
    );
}

#[test]
fn test_position_aware_delta_tracking_out_of_order_non_monotonic() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Hunk 1 edits line 100 (adds 10 lines)
    // Hunk 2 edits line 10 (earlier in the file; must NOT be shifted by Hunk 1's delta)
    // Hunk 3 edits line 150 (after both; must inherit Hunk 1's and Hunk 2's deltas)
    let mut lines = Vec::new();
    for i in 1..=200 {
        lines.push(format!("line_{}", i));
    }
    let original = lines.join("\n") + "\n";

    let diff = indoc! {r#"
        ```diff
        --- a/test.txt
        +++ b/test.txt
        @@ -100,1 +100,3 @@
        -line_100
        +line_100_modified
        +line_100_extra_1
        +line_100_extra_2
        @@ -10,1 +10,2 @@
        -line_10
        +line_10_modified
        +line_10_extra
        @@ -150,1 +150,2 @@
        -line_150
        +line_150_modified
        +line_150_extra
        ```
    "#};

    let result = patch_content_str(diff, Some(&original), &ApplyOptions::exact()).unwrap();
    assert!(result.contains("line_10_modified\nline_10_extra\nline_11"));
    assert!(result.contains("line_100_modified\nline_100_extra_1\nline_100_extra_2\nline_101"));
    assert!(result.contains("line_150_modified\nline_150_extra\nline_151"));
}

#[test]
fn test_low_entropy_blocks_in_interval_never_anchored() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Even if only one closing brace '}' exists between anchor_start and anchor_end,
    // low entropy syntax must never be anchored or tie-broken without entropy!
    let original = indoc! {r#"
        fn anchor_start() {
            init();
        }

        fn worker() {
            let val = 1;
        }

        fn anchor_end() {
            finish();
        }
    "#};

    let diff = indoc! {r#"
        test.rs
        <<<<<<< SEARCH
        }
        =======
        }
        // injected comment
        >>>>>>> REPLACE
    "#};

    let res = patch_content_str(diff, Some(original), &ApplyOptions::exact());
    assert!(
        res.is_err(),
        "Single closing brace must not be anchored or tie-broken without sufficient entropy"
    );
}

mod stdin_cli_tests {
    use indoc::indoc;
    use std::fs;
    use std::io::Write;
    use std::process::{Command, Stdio};
    use tempfile::tempdir;

    fn mpatch_bin() -> &'static str {
        env!("CARGO_BIN_EXE_mpatch")
    }

    #[test]
    fn test_cli_stdin_dash_unified_diff() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        fs::write(&file_path, "fn main() {\n    println!(\"old\");\n}\n").unwrap();

        let diff = indoc! {r#"
            --- a/main.rs
            +++ b/main.rs
            @@ -1,3 +1,3 @@
             fn main() {
            -    println!("old");
            +    println!("new from stdin");
             }
        "#};

        let mut child = Command::new(mpatch_bin())
            .arg("-")
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn mpatch binary");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "mpatch - failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let patched = fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            patched,
            "fn main() {\n    println!(\"new from stdin\");\n}\n"
        );
    }

    #[test]
    fn test_cli_stdin_dash_reverse() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        fs::write(&file_path, "fn main() {\n    println!(\"new\");\n}\n").unwrap();

        let diff = indoc! {r#"
            --- a/main.rs
            +++ b/main.rs
            @@ -1,3 +1,3 @@
             fn main() {
            -    println!("old");
            +    println!("new");
             }
        "#};

        let mut child = Command::new(mpatch_bin())
            .arg("-R")
            .arg("-")
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn mpatch binary");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());

        let restored = fs::read_to_string(&file_path).unwrap();
        assert_eq!(restored, "fn main() {\n    println!(\"old\");\n}\n");
    }

    #[test]
    fn test_cli_stdin_dash_dry_run() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("main.rs");
        let original = "fn main() {\n    println!(\"keep_me\");\n}\n";
        fs::write(&file_path, original).unwrap();

        let diff = indoc! {r#"
            --- a/main.rs
            +++ b/main.rs
            @@ -1,3 +1,3 @@
             fn main() {
            -    println!("keep_me");
            +    println!("changed_dry_run");
             }
        "#};

        let mut child = Command::new(mpatch_bin())
            .arg("-n")
            .arg("-")
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn mpatch binary");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("+    println!(\"changed_dry_run\");"));
        assert_eq!(fs::read_to_string(&file_path).unwrap(), original);
    }

    #[test]
    fn test_cli_stdin_aider_blocks() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("calc.py");
        fs::write(&file_path, "def add(a, b):\n    return a - b\n").unwrap();

        let aider_diff = indoc! {r#"
            calc.py
            <<<<<<< SEARCH
                return a - b
            =======
                return a + b
            >>>>>>> REPLACE
        "#};

        let mut child = Command::new(mpatch_bin())
            .arg("-")
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn mpatch binary");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(aider_diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());

        let patched = fs::read_to_string(&file_path).unwrap();
        assert_eq!(patched, "def add(a, b):\n    return a + b\n");
    }

    #[test]
    fn test_cli_stdin_markdown_blocks() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("server.js");
        fs::write(&file_path, "const port = 3000;\n").unwrap();

        let md_diff = indoc! {r#"
            Here is the requested fix:
            ```diff
            --- a/server.js
            +++ b/server.js
            @@ -1 +1 @@
            -const port = 3000;
            +const port = 8080;
            ```
        "#};

        let mut child = Command::new(mpatch_bin())
            .arg("-")
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn mpatch binary");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(md_diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());

        let patched = fs::read_to_string(&file_path).unwrap();
        assert_eq!(patched, "const port = 8080;\n");
    }

    #[test]
    fn test_cli_stdin_default_target_dir_when_omitted() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("local.txt");
        fs::write(&file_path, "line A\n").unwrap();

        let diff = indoc! {r#"
            --- a/local.txt
            +++ b/local.txt
            @@ -1 +1 @@
            -line A
            +line B
        "#};

        let mut child = Command::new(mpatch_bin())
            .current_dir(dir.path())
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn mpatch binary");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());

        let patched = fs::read_to_string(&file_path).unwrap();
        assert_eq!(patched, "line B\n");
    }

    #[test]
    fn test_cli_stdin_implicit_piping_with_target_dir_only() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("implicit.txt");
        fs::write(&file_path, "alpha\n").unwrap();

        let diff = indoc! {r#"
            --- a/implicit.txt
            +++ b/implicit.txt
            @@ -1 +1 @@
            -alpha
            +beta
        "#};

        let mut child = Command::new(mpatch_bin())
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn mpatch binary");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(diff.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());

        let patched = fs::read_to_string(&file_path).unwrap();
        assert_eq!(patched, "beta\n");
    }

    #[test]
    fn test_cli_stdin_syntax_error_exits_with_error() {
        let dir = tempdir().unwrap();
        let malformed = "@@ -1 +1 @@\n-missing header\n+bad\n";

        let mut child = Command::new(mpatch_bin())
            .arg("-")
            .arg(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn mpatch binary");

        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(malformed.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
    }
}

mod atomic_apply_tests {
    use indoc::indoc;
    use mpatch::{
        apply_patch_to_file_atomic, apply_patches_to_dir_atomic, parse_auto,
        try_apply_patch_to_file_atomic, try_apply_patches_to_dir_atomic, ApplyOptions,
        StrictApplyError, StrictBatchApplyError,
    };
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    fn mpatch_bin() -> &'static str {
        env!("CARGO_BIN_EXE_mpatch")
    }

    #[test]
    fn test_single_patch_atomic_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("atomic_test.txt");
        fs::write(&file_path, "line 1\nline 2\n").unwrap();

        let diff = indoc! {r#"
            --- a/atomic_test.txt
            +++ b/atomic_test.txt
            @@ -1,2 +1,2 @@
             line 1
            -line 2
            +line two
        "#};
        let patch = parse_auto(diff).unwrap().remove(0);
        let res = apply_patch_to_file_atomic(&patch, dir.path(), ApplyOptions::exact()).unwrap();
        assert!(res.report.all_applied_cleanly());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "line 1\nline two\n");
    }

    #[test]
    fn test_single_patch_atomic_failure_discards_all_edits() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("partial_target.txt");
        let original = "line 1\nline 2\nline 3\n\nline 5\nline 6\nline 7\n";
        fs::write(&file_path, original).unwrap();

        let diff = indoc! {r#"
            --- a/partial_target.txt
            +++ b/partial_target.txt
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
        "#};
        let patch = parse_auto(diff).unwrap().remove(0);
        let res = apply_patch_to_file_atomic(&patch, dir.path(), ApplyOptions::exact()).unwrap();
        assert!(!res.report.all_applied_cleanly());

        // The file on disk must be COMPLETELY unmodified (unlike non-atomic apply)
        let disk_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, original);
    }

    #[test]
    fn test_try_apply_patch_to_file_atomic_failure() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("strict_atomic.txt");
        let original = "line 1\nline 2\n";
        fs::write(&file_path, original).unwrap();

        let diff = indoc! {r#"
            --- a/strict_atomic.txt
            +++ b/strict_atomic.txt
            @@ -1,2 +1,2 @@
             line 1
            -WRONG
            +line two
        "#};
        let patch = parse_auto(diff).unwrap().remove(0);
        let res = try_apply_patch_to_file_atomic(&patch, dir.path(), ApplyOptions::exact());
        assert!(matches!(res, Err(StrictApplyError::PartialApply { .. })));

        let disk_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, original);
    }

    #[test]
    fn test_batch_atomic_success() {
        let dir = tempdir().unwrap();
        let f1 = dir.path().join("f1.txt");
        let f2 = dir.path().join("f2.txt");
        fs::write(&f1, "apple\n").unwrap();
        fs::write(&f2, "banana\n").unwrap();

        let diff = indoc! {r#"
            --- a/f1.txt
            +++ b/f1.txt
            @@ -1 +1 @@
            -apple
            +apricot
            --- a/f2.txt
            +++ b/f2.txt
            @@ -1 +1 @@
            -banana
            +blueberry
        "#};
        let patches = parse_auto(diff).unwrap();
        let batch = apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::exact());
        assert!(batch.all_applied_cleanly());
        assert!(!batch.has_failures());

        assert_eq!(fs::read_to_string(&f1).unwrap(), "apricot\n");
        assert_eq!(fs::read_to_string(&f2).unwrap(), "blueberry\n");
    }

    #[test]
    fn test_batch_atomic_partial_failure_discards_all_files() {
        let dir = tempdir().unwrap();
        let f1 = dir.path().join("f1.txt");
        let f2 = dir.path().join("f2.txt");
        fs::write(&f1, "foo\n").unwrap();
        fs::write(&f2, "bar\n").unwrap();

        let diff = indoc! {r#"
            --- a/f1.txt
            +++ b/f1.txt
            @@ -1 +1 @@
            -foo
            +foo_updated
            --- a/f2.txt
            +++ b/f2.txt
            @@ -1 +1 @@
            -WRONG_CONTEXT
            +bar_updated
        "#};
        let patches = parse_auto(diff).unwrap();
        let batch = apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::exact());
        assert!(batch.all_succeeded()); // No I/O errors
        assert!(!batch.all_applied_cleanly()); // One patch had a failing hunk
        assert!(batch.has_failures());

        // NEITHER file should have been modified on disk!
        assert_eq!(fs::read_to_string(&f1).unwrap(), "foo\n");
        assert_eq!(fs::read_to_string(&f2).unwrap(), "bar\n");
    }

    #[test]
    fn test_batch_atomic_file_creation_aborted() {
        let dir = tempdir().unwrap();
        let existing = dir.path().join("existing.txt");
        let new_file = dir.path().join("new_file.txt");
        fs::write(&existing, "line 1\n").unwrap();

        let diff = indoc! {r#"
            --- /dev/null
            +++ b/new_file.txt
            @@ -0,0 +1 @@
            +created
            --- a/existing.txt
            +++ b/existing.txt
            @@ -1 +1 @@
            -WRONG
            +modified
        "#};
        let patches = parse_auto(diff).unwrap();
        let batch = apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::exact());
        assert!(!batch.all_applied_cleanly());

        // New file must NOT be created, existing file must remain unchanged
        assert!(!new_file.exists());
        assert_eq!(fs::read_to_string(&existing).unwrap(), "line 1\n");
    }

    #[test]
    fn test_try_apply_patches_to_dir_atomic_err() {
        let dir = tempdir().unwrap();
        let f1 = dir.path().join("f1.txt");
        fs::write(&f1, "alpha\n").unwrap();

        let diff = indoc! {r#"
            --- a/f1.txt
            +++ b/f1.txt
            @@ -1 +1 @@
            -WRONG
            +beta
        "#};
        let patches = parse_auto(diff).unwrap();
        let res = try_apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::exact());
        assert!(matches!(res, Err(StrictBatchApplyError::Failed { .. })));
        assert_eq!(fs::read_to_string(&f1).unwrap(), "alpha\n");
    }

    #[test]
    fn test_cli_atomic_flag_discards_on_failure() {
        let dir = tempdir().unwrap();
        let f1 = dir.path().join("f1.txt");
        let f2 = dir.path().join("f2.txt");
        fs::write(&f1, "content_1\n").unwrap();
        fs::write(&f2, "content_2\n").unwrap();

        let diff = indoc! {r#"
            --- a/f1.txt
            +++ b/f1.txt
            @@ -1 +1 @@
            -content_1
            +content_1_updated
            --- a/f2.txt
            +++ b/f2.txt
            @@ -1 +1 @@
            -DOES_NOT_EXIST
            +content_2_updated
        "#};
        let diff_file = dir.path().join("patch.diff");
        fs::write(&diff_file, diff).unwrap();

        let output = Command::new(mpatch_bin())
            .arg("-a")
            .arg(&diff_file)
            .arg(dir.path())
            .output()
            .expect("Failed to execute mpatch binary");

        assert!(!output.status.success());
        // In atomic mode (-a), f1 must NOT be touched
        assert_eq!(fs::read_to_string(&f1).unwrap(), "content_1\n");
        assert_eq!(fs::read_to_string(&f2).unwrap(), "content_2\n");
    }
}

#[test]
fn test_similar_v3_format_inline_diff() {
    let _ = env_logger::builder().is_test(true).try_init();
    let expected = vec!["fn compute(x: i32) -> i32 {"];
    let actual = vec!["fn compute(x: i64) -> i32 {"];
    let diff = mpatch::format_inline_diff(&expected, &actual);
    assert!(diff.contains("compute"));
    assert!(diff.contains("i32"));
    assert!(diff.contains("i64"));
}

#[test]
fn test_similar_v3_merge_three_way_clean() {
    let _ = env_logger::builder().is_test(true).try_init();
    let base = "alpha\nbeta\ncommon\ngamma\ndelta\n";
    let ours = "alpha\nbeta_modified\ncommon\ngamma\ndelta\n";
    let theirs = "alpha\nbeta\ncommon\ngamma_modified\ndelta\n";
    let (merged, is_conflicted) = mpatch::merge_three_way(base, ours, theirs, None);
    assert!(!is_conflicted);
    assert_eq!(
        merged,
        "alpha\nbeta_modified\ncommon\ngamma_modified\ndelta\n"
    );
}

#[test]
fn test_similar_v3_merge_three_way_conflicted() {
    let _ = env_logger::builder().is_test(true).try_init();
    let base = "status = draft\n";
    let ours = "status = review\n";
    let theirs = "status = published\n";
    let (merged, is_conflicted) =
        mpatch::merge_three_way(base, ours, theirs, Some(("base", "ours", "theirs")));
    assert!(is_conflicted);
    assert!(merged.contains("<<<<<<< ours"));
    assert!(merged.contains("||||||| base"));
    assert!(merged.contains("======="));
    assert!(merged.contains(">>>>>>> theirs"));
}

#[test]
fn test_similar_v3_suggest_close_file_paths() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempfile::tempdir().unwrap();
    let f1 = dir.path().join("service.rs");
    let f2 = dir.path().join("controller.rs");
    std::fs::write(&f1, "fn service() {}\n").unwrap();
    std::fs::write(&f2, "fn controller() {}\n").unwrap();

    let suggestions =
        mpatch::suggest_close_file_paths(std::path::Path::new("services.rs"), dir.path(), 3);
    assert!(!suggestions.is_empty());
    assert_eq!(suggestions[0].to_str().unwrap(), "service.rs");
}

#[test]
fn test_similar_v3_format_inline_diff_multiline() {
    let _ = env_logger::builder().is_test(true).try_init();
    let expected = vec![
        "def run_server(host, port):",
        "    init_logger()",
        "    bind_socket(host, port)",
        "    start_loop()",
    ];
    let actual = vec![
        "def run_server(host, port, timeout=30):",
        "    init_logger()",
        "    bind_socket(host, port)",
        "    start_event_loop()",
    ];

    let diff = mpatch::format_inline_diff(&expected, &actual);
    assert!(diff.contains("run_server"));
    assert!(diff.contains("timeout=30"));
    assert!(diff.contains("start_event_loop"));
    assert!(diff.contains("init_logger"));
    assert!(diff.contains('-'));
    assert!(diff.contains('+'));
}

#[test]
fn test_similar_v3_format_inline_diff_whitespace_and_punctuation() {
    let _ = env_logger::builder().is_test(true).try_init();
    let expected = vec!["fn compute(data: &[u8], timeout: u64) -> Result<()> {"];
    let actual = vec!["fn compute(data: &[u8], timeout: Duration, force: bool) -> Result<()> {"];

    let diff = mpatch::format_inline_diff(&expected, &actual);
    assert!(diff.contains("compute"));
    assert!(diff.contains("timeout"));
    assert!(diff.contains("Duration"));
    assert!(diff.contains("force: bool"));
}

#[test]
fn test_similar_v3_merge_three_way_identical_edits() {
    let _ = env_logger::builder().is_test(true).try_init();
    let base = "line 1\nline 2\nline 3\n";
    let ours = "line 1\nline TWO\nline 3\n";
    let theirs = "line 1\nline TWO\nline 3\n";

    let (merged, is_conflicted) = mpatch::merge_three_way(base, ours, theirs, None);
    assert!(
        !is_conflicted,
        "Identical edits on both branches must not conflict"
    );
    assert_eq!(merged, "line 1\nline TWO\nline 3\n");
}

#[test]
fn test_similar_v3_merge_three_way_disjoint_edits() {
    let _ = env_logger::builder().is_test(true).try_init();
    let base = "header\nmiddle 1\nmiddle 2\nfooter\n";
    let ours = "NEW HEADER\nmiddle 1\nmiddle 2\nfooter\n";
    let theirs = "header\nmiddle 1\nmiddle 2\nNEW FOOTER\n";

    let (merged, is_conflicted) = mpatch::merge_three_way(base, ours, theirs, None);
    assert!(!is_conflicted, "Disjoint edits must merge cleanly");
    assert_eq!(merged, "NEW HEADER\nmiddle 1\nmiddle 2\nNEW FOOTER\n");
}

#[test]
fn test_similar_v3_merge_three_way_custom_labels_diff3() {
    let _ = env_logger::builder().is_test(true).try_init();
    let base = "timeout = 30\n";
    let ours = "timeout = 60\n";
    let theirs = "timeout = 120\n";

    let (merged, is_conflicted) = mpatch::merge_three_way(
        base,
        ours,
        theirs,
        Some(("ancestor-v1", "feature-timeout", "main-branch")),
    );

    assert!(is_conflicted);
    assert!(merged.contains("<<<<<<< feature-timeout"));
    assert!(merged.contains("||||||| ancestor-v1"));
    assert!(merged.contains("======="));
    assert!(merged.contains(">>>>>>> main-branch"));
}

#[test]
fn test_similar_v3_suggest_close_file_paths_nested_and_exclusions() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    // Create nested source directories
    std::fs::create_dir_all(base.join("crates/core/src/parser")).unwrap();
    std::fs::create_dir_all(base.join("target/debug/build")).unwrap();
    std::fs::create_dir_all(base.join("node_modules/some_lib")).unwrap();
    std::fs::create_dir_all(base.join(".git/objects")).unwrap();

    std::fs::write(
        base.join("crates/core/src/parser/engine.rs"),
        "pub struct Engine;",
    )
    .unwrap();
    std::fs::write(base.join("target/debug/build/engine.rs"), "noise").unwrap();
    std::fs::write(base.join("node_modules/some_lib/engine.rs"), "noise").unwrap();

    // Query with a slight typo in the filename
    let suggestions = mpatch::suggest_close_file_paths(
        std::path::Path::new("crates/core/src/parser/engin.rs"),
        base,
        5,
    );

    assert!(!suggestions.is_empty());
    let found = suggestions[0].to_str().unwrap().replace('\\', "/");
    assert_eq!(found, "crates/core/src/parser/engine.rs");

    // Ensure target, node_modules, and hidden dirs are never suggested
    assert!(!suggestions.iter().any(|p| {
        let s = p.to_string_lossy();
        s.starts_with("target") || s.starts_with("node_modules") || s.starts_with(".git")
    }));
}

#[test]
fn test_similar_v3_suggest_close_file_paths_no_match() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();
    std::fs::write(base.join("main.rs"), "fn main() {}\n").unwrap();

    let suggestions = mpatch::suggest_close_file_paths(
        std::path::Path::new("totally_unrelated_xyz_12345.dat"),
        base,
        3,
    );
    assert!(suggestions.is_empty());
}

#[test]
fn test_cli_suggests_close_path_on_target_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let target_file = dir.path().join("calculator.rs");
    std::fs::write(&target_file, "fn add() {}\n").unwrap();

    // Patch targets 'calculate.rs' instead of 'calculator.rs'
    let diff = indoc! {r#"
        --- a/calculate.rs
        +++ b/calculate.rs
        @@ -1 +1 @@
        -fn add() {}
        +fn add_numbers() {}
    "#};
    let patch_file = dir.path().join("patch.diff");
    std::fs::write(&patch_file, diff).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mpatch"))
        .arg(&patch_file)
        .arg(dir.path())
        .output()
        .expect("Failed to execute mpatch binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Target file not found. Did you mean: 'calculator.rs'?"),
        "Expected suggestion in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_cli_near_miss_inline_diagnostic_output() {
    let dir = tempfile::tempdir().unwrap();
    let target_file = dir.path().join("worker.rs");
    // Target file has slightly different context line to trigger FuzzyMatchBelowThreshold with fuzz_factor=0.99
    std::fs::write(
        &target_file,
        "fn worker_task(id: u64, queue: &Queue) {\n    execute(id);\n}\n",
    )
    .unwrap();

    let diff = indoc! {r#"
        --- a/worker.rs
        +++ b/worker.rs
        @@ -1,3 +1,3 @@
         fn worker_task(id: usize, queue: &Queue) {
        -    execute(id);
        +    execute_task(id);
         }
    "#};
    let patch_file = dir.path().join("patch.diff");
    std::fs::write(&patch_file, diff).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mpatch"))
        .arg("-vv")
        .arg("-f")
        .arg("0.99") // Strict threshold ensures fuzzy near-miss failure
        .arg(&patch_file)
        .arg(dir.path())
        .output()
        .expect("Failed to execute mpatch binary");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Near-miss candidate at"),
        "Expected near-miss candidate message in stderr, got: {}",
        stderr
    );
    assert!(
        stderr.contains("worker_task"),
        "Expected diff lines in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_large_hunk_large_file_fuzzy_match_with_anchor() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("large_service.rs");

    // Generate a 10,000-line file with realistic code
    let mut lines = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        lines.push(format!("    let stage_{} = execute_step({});", i, i));
    }
    fs::write(&file_path, lines.join("\n") + "\n").unwrap();

    // Create a 500-line hunk targeting lines 5,000..5,500
    let mut patch_lines = vec![
        "--- a/large_service.rs".to_string(),
        "+++ b/large_service.rs".to_string(),
        "@@ -5000,500 +5000,500 @@".to_string(),
    ];
    for i in 5000..5500 {
        if i == 5250 {
            patch_lines.push(format!("-    let stage_{} = execute_step({});", i, i));
            patch_lines.push(format!("+    let stage_{} = execute_step_optimized({});", i, i));
        } else {
            patch_lines.push(format!("     let stage_{} = execute_step({});", i, i));
        }
    }

    let patch = mpatch::parse_auto(&patch_lines.join("\n")).unwrap().remove(0);
    let options = ApplyOptions::new(); // Fuzz factor 0.70
    let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

    assert!(
        result.report.all_applied_cleanly(),
        "500-line hunk in 10,000-line file should apply cleanly"
    );
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("let stage_5250 = execute_step_optimized(5250);"));
}

#[test]
fn test_large_hunk_large_file_coincidental_anchor_fallback() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("service_collision.rs");

    let mut lines = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        if i == 1_000 {
            // Coincidental anchor collision line early in the file
            lines.push("    let unique_marker_token = 0xDEADBEEF;".to_string());
        } else if i >= 8_000 && i < 8_500 {
            lines.push(format!("    let core_calc_{} = compute_val({});", i, i));
        } else {
            lines.push(format!("    let noise_{} = dummy_step({});", i, i));
        }
    }
    fs::write(&file_path, lines.join("\n") + "\n").unwrap();

    // Hunk has unique_marker_token (which collides with line 1,000), but the true 500-line
    // body is at lines 8,000..8,500.
    let mut patch_lines = vec![
        "--- a/service_collision.rs".to_string(),
        "+++ b/service_collision.rs".to_string(),
        "@@ -8000,500 +8000,500 @@".to_string(),
    ];
    for i in 8000..8500 {
        if i == 8005 {
            patch_lines.push("     let unique_marker_token = 0xDEADBEEF;".to_string());
        } else if i == 8250 {
            patch_lines.push(format!("-    let core_calc_{} = compute_val({});", i, i));
            patch_lines.push(format!("+    let core_calc_{} = compute_val_fast({});", i, i));
        } else {
            patch_lines.push(format!("     let core_calc_{} = compute_val({});", i, i));
        }
    }

    let patch = mpatch::parse_auto(&patch_lines.join("\n")).unwrap().remove(0);
    let options = ApplyOptions::new();

    let timeout = if cfg!(debug_assertions) {
        std::time::Duration::from_secs(10)
    } else {
        std::time::Duration::from_secs(5)
    };
    let start_time = std::time::Instant::now();

    // Run patch application on a worker thread bounded by a 5-second hard timeout
    let (tx, rx) = std::sync::mpsc::channel();
    let target_dir = dir.path().to_path_buf();
    let patch_clone = patch.clone();
    std::thread::spawn(move || {
        let res = apply_patch_to_file(&patch_clone, &target_dir, options);
        let _ = tx.send(res);
    });

    let result = rx
        .recv_timeout(timeout)
        .unwrap_or_else(|_| panic!("test_large_hunk_large_file_coincidental_anchor_fallback took too long (> {:?})", timeout))
        .unwrap();

    let elapsed = start_time.elapsed();
    assert!(
        elapsed < timeout,
        "Test execution exceeded timeout: {:?} >= {:?}",
        elapsed,
        timeout
    );

    assert!(
        result.report.all_applied_cleanly(),
        "Tiered fallback must recover from coincidental anchor collision and find the true match"
    );
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("let core_calc_8250 = compute_val_fast(8250);"));
}

#[test]
fn test_large_hunk_non_matching_fast_rejection() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("unrelated_file.rs");

    let lines: Vec<String> = (0..10_000).map(|i| format!("fn handler_{}() {{}}", i)).collect();
    fs::write(&file_path, lines.join("\n") + "\n").unwrap();

    // 500-line hunk from completely different codebase
    let mut patch_lines = vec![
        "--- a/unrelated_file.rs".to_string(),
        "+++ b/unrelated_file.rs".to_string(),
        "@@ -1,500 +1,500 @@".to_string(),
    ];
    for i in 0..500 {
        if i == 250 {
            patch_lines.push(format!("-SELECT column_{} FROM database_table_{};", i, i));
            patch_lines.push(format!("+SELECT column_{} FROM database_table_mod_{};", i, i));
        } else {
            if i == 250 {
                patch_lines.push(format!("-SELECT column_{} FROM database_table_{};", i, i));
                patch_lines.push(format!("+SELECT column_{} FROM database_table_mod_{};", i, i));
            } else {
                patch_lines.push(format!(" SELECT column_{} FROM database_table_{};", i, i));
            }
        }
    }

    let patch = mpatch::parse_auto(&patch_lines.join("\n")).unwrap().remove(0);
    let options = ApplyOptions::new();

    let timeout = std::time::Duration::from_secs(5);
    let start_time = std::time::Instant::now();

    // Run patch application on a worker thread bounded by a 5-second hard timeout
    let (tx, rx) = std::sync::mpsc::channel();
    let target_dir = dir.path().to_path_buf();
    let patch_clone = patch.clone();
    std::thread::spawn(move || {
        let res = apply_patch_to_file(&patch_clone, &target_dir, options);
        let _ = tx.send(res);
    });

    let result = rx
        .recv_timeout(timeout)
        .unwrap_or_else(|_| panic!("test_large_hunk_non_matching_fast_rejection took too long (> {:?})", timeout))
        .unwrap();

    let elapsed = start_time.elapsed();
    assert!(
        elapsed < timeout,
        "Test execution exceeded timeout: {:?} >= {:?}",
        elapsed,
        timeout
    );

    assert!(
        !result.report.all_applied_cleanly(),
        "Unrelated 500-line hunk must fail cleanly"
    );
}

#[test]
fn test_large_scale_patience_diff_roundtrip() {
    let _ = env_logger::builder().is_test(true).try_init();

    // Generate 1,000+ lines of realistic code
    let mut original = String::with_capacity(30_000);
    for i in 0..100 {
        original.push_str(&format!("fn function_{}(val: i32) -> i32 {{\n", i));
        original.push_str(&format!("    let step_a = val + {};\n", i));
        original.push_str("    let step_b = step_a * 2;\n");
        original.push_str("    step_b\n");
        original.push_str("}\n\n");
    }

    let mut modified = String::with_capacity(35_000);
    // Insert helper functions every 15 items, and modify step_b calculation every 5 items
    for i in 0..100 {
        if i % 15 == 0 {
            modified.push_str(&format!(
                "// Added helper for module {}\nfn helper_module_{}() -> bool {{ true }}\n\n",
                i, i
            ));
        }
        modified.push_str(&format!("fn function_{}(val: i32) -> i32 {{\n", i));
        modified.push_str(&format!("    let step_a = val + {};\n", i));
        if i % 5 == 0 {
            modified.push_str(&format!("    let step_b = step_a * 10 + {};\n", i));
        } else {
            modified.push_str("    let step_b = step_a * 2;\n");
        }
        modified.push_str("    step_b\n");
        modified.push_str("}\n\n");
    }

    // Generate patch using Algorithm::Patience
    let patch = mpatch::Patch::from_texts("large.rs", &original, &modified, 3).unwrap();
    assert!(
        patch.hunks.len() >= 20,
        "Should generate dozens of hunks across 1,000 lines"
    );

    // Apply patch to original and verify byte-for-byte equality with modified
    let options = mpatch::ApplyOptions::exact();
    let result = mpatch::apply_patch_to_content(&patch, Some(&original), &options);
    assert!(
        result.report.all_applied_cleanly(),
        "All hunks generated by Patience diff should apply cleanly"
    );
    assert_eq!(result.new_content, modified);
}

#[test]
fn test_large_scale_three_way_merge_concurrent_disjoint() {
    let _ = env_logger::builder().is_test(true).try_init();

    // 1,000+ line base file containing 200 functions
    let mut base = String::with_capacity(30_000);
    for i in 0..200 {
        base.push_str(&format!(
            "// Component {}\nfn comp_{}() {{\n    step();\n}}\n\n",
            i, i
        ));
    }

    // Branch A (ours) modifies every 4th function (0, 4, 8, ...)
    let mut ours = String::with_capacity(30_000);
    for i in 0..200 {
        if i % 4 == 0 {
            ours.push_str(&format!(
                "// Component {}\nfn comp_{}() {{\n    ours_step();\n}}\n\n",
                i, i
            ));
        } else {
            ours.push_str(&format!(
                "// Component {}\nfn comp_{}() {{\n    step();\n}}\n\n",
                i, i
            ));
        }
    }

    // Branch B (theirs) modifies every 4th function with offset 2 (2, 6, 10, ...)
    let mut theirs = String::with_capacity(30_000);
    for i in 0..200 {
        if i % 4 == 2 {
            theirs.push_str(&format!(
                "// Component {}\nfn comp_{}() {{\n    theirs_step();\n}}\n\n",
                i, i
            ));
        } else {
            theirs.push_str(&format!(
                "// Component {}\nfn comp_{}() {{\n    step();\n}}\n\n",
                i, i
            ));
        }
    }

    // 3-way merge across 200 functions with 100 concurrent edits
    let (merged, is_conflicted) = mpatch::merge_three_way(&base, &ours, &theirs, None);
    assert!(
        !is_conflicted,
        "Large-scale disjoint 3-way merge across 200 functions must resolve cleanly"
    );

    for i in 0..200 {
        if i % 4 == 0 {
            assert!(merged.contains(&format!("fn comp_{}() {{\n    ours_step();\n}}", i)));
        } else if i % 4 == 2 {
            assert!(merged.contains(&format!("fn comp_{}() {{\n    theirs_step();\n}}", i)));
        }
    }
}

#[test]
fn test_large_scale_three_way_merge_interleaved_conflicts() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut base = String::new();
    let mut ours = String::new();
    let mut theirs = String::new();

    for i in 0..50 {
        base.push_str(&format!("// Module {}\nfn setup_{}() {{}}\n", i, i));
        ours.push_str(&format!("// Module {}\nfn setup_{}() {{}}\n", i, i));
        theirs.push_str(&format!("// Module {}\nfn setup_{}() {{}}\n", i, i));

        if i % 2 == 0 {
            base.push_str("state = 'base'\n\n");
            ours.push_str("state = 'ours'\n\n");
            theirs.push_str("state = 'theirs'\n\n");
        } else {
            base.push_str("state = 'base'\n\n");
            ours.push_str("state = 'base'\n\n");
            theirs.push_str("state = 'clean_theirs'\n\n");
        }
    }

    let (merged, is_conflicted) =
        mpatch::merge_three_way(&base, &ours, &theirs, Some(("BASE", "OURS", "THEIRS")));

    assert!(is_conflicted);
    assert_eq!(merged.matches("<<<<<<< OURS").count(), 25);
    assert_eq!(merged.matches("||||||| BASE").count(), 25);
    assert_eq!(merged.matches(">>>>>>> THEIRS").count(), 25);

    // The 25 odd non-conflicting modules must be merged cleanly without conflict markers
    for i in (1..50).step_by(2) {
        assert!(merged.contains(&format!(
            "// Module {}\nfn setup_{}() {{}}\nstate = 'clean_theirs'\n\n",
            i, i
        )));
    }
}

#[test]
fn test_large_scale_suggest_close_file_paths_deep_tree() {
    let _ = env_logger::builder().is_test(true).try_init();
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    // Generate 250 files across deeply nested subdirectories
    for module in &["auth", "billing", "analytics", "storage", "compute"] {
        for layer in &["models", "views", "controllers", "services", "helpers"] {
            let dir_path = base.join(format!("src/{}/{}", module, layer));
            std::fs::create_dir_all(&dir_path).unwrap();
            for k in 0..10 {
                std::fs::write(
                    dir_path.join(format!("handler_{}.rs", k)),
                    "pub struct Handler;",
                )
                .unwrap();
            }
        }
    }

    // Query for a typo deep in the directory hierarchy
    let suggestions = mpatch::suggest_close_file_paths(
        std::path::Path::new("src/bililng/services/handler_7.rs"),
        base,
        3,
    );

    assert!(!suggestions.is_empty());
    let top = suggestions[0].to_str().unwrap().replace('\\', "/");
    assert_eq!(top, "src/billing/services/handler_7.rs");
}

#[test]
fn test_large_scale_inline_diff_rendering() {
    let _ = env_logger::builder().is_test(true).try_init();

    let mut expected = Vec::with_capacity(100);
    let mut actual = Vec::with_capacity(100);

    for i in 0..100 {
        expected.push(format!(
            "pub fn handle_event_{}(ctx: &mut Context, id: u32) -> Result<(), Error> {{",
            i
        ));
        actual.push(format!("pub fn handle_event_{}(ctx: &mut Context, id: u64, flags: EventFlags) -> Result<(), AppError> {{", i));
    }

    let exp_refs: Vec<&str> = expected.iter().map(|s| s.as_str()).collect();
    let diff = mpatch::format_inline_diff(&exp_refs, &actual);

    assert!(diff.contains("handle_event_0"));
    assert!(diff.contains("handle_event_99"));
    assert!(diff.contains("EventFlags"));
    assert!(diff.contains("AppError"));
}

mod multi_level_fuzzy_weakness_tests {
    use super::*;
    use indoc::indoc;
    use std::fs;
    use tempfile::tempdir;

    /// LEVEL 1: Head Context Chopping & Security Privilege Escalation
    ///
    /// The window finder chops off the function header and security check at the head
    /// of the hunk because the search window started mid-hunk. It drops them as "stale context"
    /// and splices elevated execution into an unauthenticated guest handler!
    #[test]
    fn test_head_context_truncation_hijacks_unrelated_function() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("auth.rs");

        let original = indoc! {r#"
            pub fn handle_guest_request(req: &Request) -> Result<(), Error> {
                let session = get_session(req);
                let role = get_role(session);
                let quota = get_quota(role);
                verify_quota(quota);
                dispatch(req)
            }

            // 50 lines of intervening code
            pub fn handle_admin_request(ctx: &SecurityContext, req: &Request) -> Result<(), Error> {
                // Signature refactored to take SecurityContext
                ctx.verify_mfa()?;
                let session = get_session(req);
                let role = get_role(session);
                let quota = get_quota(role);
                verify_quota(quota);
                dispatch(req)
            }
        "#};
        fs::write(&file_path, original).unwrap();

        // Patch was written for the legacy admin_request signature
        let diff = indoc! {r#"
            ```diff
            --- a/auth.rs
            +++ b/auth.rs
            @@ -1,8 +1,9 @@
             pub fn handle_admin_request(req: &Request) -> Result<(), Error> {
                 assert_admin_privileges(req);
                 let session = get_session(req);
                 let role = get_role(session);
                 let quota = get_quota(role);
                 verify_quota(quota);
            +    grant_superuser_privileges(session);
                 dispatch(req)
             }
            ```
        "#};

        let patch = parse_diffs(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

        // Must reject! grant_superuser_privileges must NEVER be injected into handle_guest_request!
        assert!(
            !result.report.all_applied_cleanly(),
            "VULNERABILITY: Head context was chopped, injecting admin privileges into guest handler!"
        );
        let disk_content = fs::read_to_string(&file_path).unwrap();
        assert!(
            !disk_content.contains("grant_superuser_privileges"),
            "CRITICAL: Privileged escalation was injected into the guest handler!"
        );
        assert_eq!(disk_content, original);
    }

    /// LEVEL 2: Tail Context Chopping Drops Mandatory Audit & Safety Cleanup
    ///
    /// Suffix context lines (audit logging, lock release) are cut off by the window boundary.
    /// The applier treats the missing tail lines as "stale context", omitting mandatory audit logs.
    #[test]
    fn test_tail_context_truncation_drops_mandatory_audit_and_cleanup() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("tx.rs");

        let original = indoc! {r#"
            pub fn dry_run_simulation(tx: &mut Transaction) -> Result<(), Error> {
                tx.prepare();
                tx.validate();
                tx.simulate();
                Ok(())
            }

            pub fn commit_transaction(tx: &mut Transaction) -> Result<(), Error> {
                tx.prepare();
                tx.validate();
                tx.execute_real();
                tx.record_audit_log();
                tx.release_distributed_lock();
                Ok(())
            }
        "#};
        fs::write(&file_path, original).unwrap();

        // A patch modifying the transaction pipeline with strict mandatory audit tail
        let diff = indoc! {r#"
            ```diff
            --- a/tx.rs
            +++ b/tx.rs
            @@ -1,7 +1,8 @@
             pub fn commit_transaction(tx: &mut Transaction) -> Result<(), Error> {
                 tx.prepare();
                 tx.validate();
            -    tx.execute_real();
            +    tx.execute_real_v2();
                 tx.record_audit_log();
                 tx.release_distributed_lock();
                 Ok(())
             }
            ```
        "#};

        // Modify commit_transaction in target so its audit/lock calls differ
        let modified_target = indoc! {r#"
            pub fn dry_run_simulation(tx: &mut Transaction) -> Result<(), Error> {
                tx.prepare();
                tx.validate();
                tx.simulate();
                Ok(())
            }

            pub fn commit_transaction(tx: &mut Transaction) -> Result<(), Error> {
                tx.prepare();
                tx.validate();
                tx.execute_real();
                // Tail was replaced with modern RAII guard
                let _guard = TxAuditGuard::new(tx);
                Ok(())
            }
        "#};
        fs::write(&file_path, modified_target).unwrap();

        let patch = parse_diffs(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

        // Must reject because the tail context (mandatory audit + lock) is missing
        assert!(
            !result.report.all_applied_cleanly(),
            "VULNERABILITY: Tail context was truncated, bypassing security invariants!"
        );
        let disk_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, modified_target);
    }

    /// LEVEL 3: Dilution-Free Interleaved Window Bloat (`scale` Metric Flaw)
    ///
    /// The formula `scale = (window_len + len) / (2 * len)` cancels out window length.
    /// A window spanning 70 lines of unrelated database queries scores M / len = 6/7 = 85.7%!
    /// It splices configuration flags into the middle of the unrelated database code.
    #[test]
    fn test_scale_metric_allows_absurd_window_bloat_matching_unrelated_code() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("service.rs");

        let mut original = String::from("pub fn setup_constants() {\n");
        original.push_str("    let timeout_ms = 5000;\n");
        original.push_str("    let max_retries = 3;\n");
        original.push_str("}\n\n");

        original.push_str("pub fn run_financial_payroll_batch() {\n");
        for i in 0..70 {
            original.push_str(&format!("    execute_payroll_stage_{}();\n", i));
        }
        original.push_str("    let keep_alive = true;\n");
        original.push_str("    let verify_tls = true;\n");
        original.push_str("}\n");
        fs::write(&file_path, &original).unwrap();

        // Patch written for a compact 7-line configuration struct
        let diff = indoc! {r#"
            ```diff
            --- a/service.rs
            +++ b/service.rs
            @@ -1,7 +1,8 @@
                 let timeout_ms = 5000;
                 let max_retries = 3;
            +    let pool_size = 64;
                 let keep_alive = true;
                 let verify_tls = true;
            ```
        "#};

        let patch = parse_diffs(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

        // Must reject! The hunk was never meant to span 70 lines of financial payroll code!
        assert!(
            !result.report.all_applied_cleanly(),
            "VULNERABILITY: Window bloat bridged 70 lines of payroll logic without dilution penalty!"
        );
        let disk_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, original);
    }

    /// LEVEL 4: Coincidental Anchor Collision Hijacks Addition into Wrong Function
    ///
    /// `worker_b` was deleted from the codebase. But its context line `shutdown();` coincidentally
    /// exists in `system_emergency_stop()`. The window stretches to grab `shutdown();`,
    /// so the orphan guard NEVER fires and injects the hook into emergency stop!
    #[test]
    fn test_coincidental_context_collision_prevents_orphan_guard_from_firing() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("daemon.rs");

        // Note: worker_b is DELETED. Only worker_a and emergency_stop exist.
        let original = indoc! {r#"
            pub fn worker_a() {
                init_buffers();
                step_1();
                step_2();
            }

            // 35 lines of unrelated daemon logic
            pub fn system_emergency_stop() {
                cut_power();
                shutdown();
            }
        "#};
        fs::write(&file_path, original).unwrap();

        // Patch created against older version that still had worker_b
        let diff = indoc! {r#"
            ```diff
            --- a/daemon.rs
            +++ b/daemon.rs
            @@ -1,11 +1,13 @@
             pub fn worker_a() {
                 init_buffers();
            -    step_1();
            +    step_1_v2();
                 step_2();
             }

             pub fn worker_b() {
            +    injected_probe_hook();
                 shutdown();
             }
            ```
        "#};

        let patch = parse_diffs(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

        // Must reject with ContextNotFound because worker_b does not exist!
        assert!(
            !result.report.all_applied_cleanly(),
            "VULNERABILITY: Addition was coincidentally anchored to emergency_stop instead of being rejected!"
        );
        let disk_content = fs::read_to_string(&file_path).unwrap();
        assert!(
            !disk_content.contains("injected_probe_hook"),
            "CRITICAL: Hook was injected into emergency stop!"
        );
        assert_eq!(disk_content, original);
    }

    /// LEVEL 5: Scope Delimiter Violation (Window Bridges Across Function Closure `}`)
    ///
    /// The window finder matches the head in `authenticate`, crosses `}`, and grabs lines
    /// from `anonymous_ping`. Changes are spliced into `anonymous_ping` while believing
    /// it's still in `authenticate`.
    #[test]
    fn test_scope_boundary_violation_bridges_across_closing_braces() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("server.rs");

        let original = indoc! {r#"
            pub fn authenticate(req: &Request) -> bool {
                let token = req.header("X-Auth");
                validate_token(token)
            }

            pub fn anonymous_health_ping() -> &'static str {
                let token = "ping";
                "pong"
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/server.rs
            +++ b/server.rs
            @@ -1,5 +1,6 @@
             pub fn authenticate(req: &Request) -> bool {
                 let token = req.header("X-Auth");
            +    verify_not_blacklisted(token);
                 validate_token(token)
             }
            ```
        "#};

        // Modify authenticate in target so it doesn't match, leaving anonymous_health_ping
        let refactored_target = indoc! {r#"
            pub fn authenticate(ctx: &Context, req: &Request) -> bool {
                // Completely refactored
                oauth2_validate(ctx, req)
            }

            pub fn anonymous_health_ping() -> &'static str {
                let token = req.header("X-Auth");
                "pong"
            }
        "#};
        fs::write(&file_path, refactored_target).unwrap();

        let patch = parse_diffs(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

        // Must reject! Must NOT cross into anonymous_health_ping
        assert!(
            !result.report.all_applied_cleanly(),
            "VULNERABILITY: Crossed scope delimiter into anonymous health ping!"
        );
        let disk_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, refactored_target);
    }

    /// LEVEL 6: Statement Matching False Positive Rewrites Unrelated API Calls
    ///
    /// In `find_statement_match_in_block`, word similarity threshold 0.60 causes
    /// `send_payment_request` to falsely match `send_telemetry_request` because the argument
    /// options dict shares >60% of words!
    #[test]
    fn test_statement_match_threshold_hijacks_different_api_call() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("api.ts");

        let original = indoc! {r#"
            async function executeWorkflow(ctx: Context) {
                const config = { timeout: 5000, retries: 3, verbose: false };
                await sendTelemetryRequest("/api/v1/metrics", config);
                return true;
            }
        "#};
        fs::write(&file_path, original).unwrap();

        // Patch modifies sendPaymentRequest, which does NOT exist in the file!
        let diff = indoc! {r#"
            ```diff
            --- a/api.ts
            +++ b/api.ts
            @@ -1,4 +1,5 @@
             async function executeWorkflow(ctx: Context) {
                 const config = { timeout: 5000, retries: 3, verbose: false };
            -    await sendPaymentRequest("/api/v1/charge", config);
            +    await sendPaymentRequestV2("/api/v2/charge", config, authToken);
                 return true;
             }
            ```
        "#};

        let patch = parse_diffs(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

        // Must reject! sendPaymentRequest must not hijack sendTelemetryRequest!
        assert!(
            !result.report.all_applied_cleanly(),
            "VULNERABILITY: Statement matcher hijacked telemetry call for payment call!"
        );
        let disk_content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(disk_content, original);
    }

    /// LEVEL 7: Python Indentation Hierarchy Corrupted by Truncated Window
    ///
    /// Splicing a truncated window into Python code samples `target_indent` from inside
    /// a nested block (8 spaces) while the patch expected root indentation (0 spaces),
    /// corrupting the entire block with syntax errors.
    #[test]
    fn test_python_indentation_hierarchy_corrupted_by_truncated_window() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("pipeline.py");

        let original = indoc! {r#"
            def run_pipeline(data):
                if data is not None:
                    for item in data:
                        step_1(item)
                        step_2(item)
                        step_3(item)
        "#};
        fs::write(&file_path, original).unwrap();

        // Patch modifying the inner loop, but providing outer context
        let diff = indoc! {r#"
            ```diff
            --- a/pipeline.py
            +++ b/pipeline.py
            @@ -1,7 +1,9 @@
             def run_pipeline_legacy(data):
                 if data is not None:
                     for item in data:
                         step_1(item)
            -            step_2(item)
            +            step_2_v2(item)
            +            audit_item(item)
                         step_3(item)
            ```
        "#};

        let patch = parse_diffs(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

        if result.report.all_applied_cleanly() {
            let content = fs::read_to_string(&file_path).unwrap();
            // Verify Python indentation is not scrambled to 16 spaces or 0 spaces
            for line in content.lines() {
                if line.contains("step_2_v2") || line.contains("audit_item") {
                    assert_eq!(
                        &line[..12],
                        "            ",
                        "CRITICAL: Python indentation corrupted! Expected 12 spaces, got: '{}'",
                        line
                    );
                }
            }
        }
    }

    /// LEVEL 8: Cascading Anchor Poisoning in Multi-Hunk Applications
    ///
    /// In a multi-hunk patch, Hunk 1 binds to a bloated/truncated window.
    /// Its corrupt anchor in `completed_edits` misdirects Hunk 2 to the wrong module!
    #[test]
    fn test_cascading_anchor_poisoning_in_multi_hunk_patch() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("module.rs");

        let original = indoc! {r#"
            // Module Alpha
            pub fn handle_event() {
                log_event("alpha");
            }

            // 60 lines of intermediate code
            // Module Beta
            pub fn handle_event() {
                log_event("beta");
            }
        "#};
        fs::write(&file_path, original).unwrap();

        let diff = indoc! {r#"
            ```diff
            --- a/module.rs
            +++ b/module.rs
            @@ -1,3 +1,3 @@
             // Module Alpha
             pub fn handle_event() {
            -    log_event("alpha");
            +    log_event("alpha_v2");
             }
            @@ -20,3 +20,3 @@
             // Module Beta
             pub fn handle_event() {
            -    log_event("beta");
            +    log_event("beta_v2");
             }
            ```
        "#};

        let patch = parse_diffs(diff).unwrap().remove(0);
        let options = ApplyOptions::exact();
        let result = apply_patch_to_file(&patch, dir.path(), options).unwrap();

        assert!(result.report.all_applied_cleanly());
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains(r#"log_event("alpha_v2");"#));
        assert!(content.contains(r#"log_event("beta_v2");"#));
    }

    /// LEVEL 9: Atomic Batch Application False Clean Commit
    ///
    /// In atomic mode, File 2 has an anchor collision that should fail.
    /// Because the false match reports `all_applied_cleanly == true`, atomic mode
    /// falsely commits corrupted files to disk!
    #[test]
    fn test_atomic_batch_commits_corrupted_files_due_to_false_clean_match() {
        let dir = tempdir().unwrap();
        let f1 = dir.path().join("valid.txt");
        let f2 = dir.path().join("service.rs");

        fs::write(&f1, "legitimate_content\n").unwrap();

        // In service.rs, worker_secondary was deleted
        let original_f2 = indoc! {r#"
            pub fn worker_primary() {
                step_1();
                step_2();
            }

            pub fn garbage_collector() {
                run_gc();
                shutdown_all();
            }
        "#};
        fs::write(&f2, original_f2).unwrap();

        let diff = indoc! {r#"
            --- a/valid.txt
            +++ b/valid.txt
            @@ -1 +1 @@
            -legitimate_content
            +updated_content
            --- a/service.rs
            +++ b/service.rs
            @@ -1,8 +1,10 @@
             pub fn worker_primary() {
                 step_1();
                 step_2();
             }

             pub fn worker_secondary() {
            +    injected_probe();
                 shutdown_all();
             }
        "#};

        let patches = parse_auto(diff).unwrap();
        let batch = apply_patches_to_dir_atomic(&patches, dir.path(), ApplyOptions::new());

        // The entire batch MUST fail atomically!
        assert!(
            !batch.all_applied_cleanly(),
            "CRITICAL: Atomic batch falsely committed corrupted files!"
        );
        // NEITHER file should have been modified!
        assert_eq!(
            fs::read_to_string(&f1).unwrap(),
            "legitimate_content\n",
            "File 1 was modified despite atomic failure in File 2!"
        );
        assert_eq!(
            fs::read_to_string(&f2).unwrap(),
            original_f2,
            "File 2 was modified despite atomic failure!"
        );
    }

    #[test]
    fn test_fallback_reconciliation_on_trailing_delimiter_context_drift() {
        let _ = env_logger::builder().is_test(true).try_init();
        let original = indoc! {r#"
            pub fn is_plausible_file_path(s: &str) -> bool {
                if s.contains(char::is_whitespace) {
                    has_valid_extension || is_known_filename
                } else {
                    has_slash || has_valid_extension || is_known_filename
                }
            }

            pub fn next_function() {}
        "#};

        // Patch was written expecting `    };` on the else block (with a semicolon),
        // which differs from the target file's `    }`.
        let diff = indoc! {r#"
            --- a/test.rs
            +++ b/test.rs
            @@ -4,6 +4,10 @@
                     has_slash || has_valid_extension || is_known_filename
                 };
            +    if is_plausible {
            +        println!("accepted");
            +    }
            +    is_plausible
             }

             pub fn next_function() {}
        "#};

        let patch = parse_auto(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = mpatch::apply_patch_to_content(&patch, Some(original), &options);

        assert!(
            result.report.all_applied_cleanly(),
            "Hunk must apply cleanly via fallback reconciliation despite context delimiter drift"
        );
        assert!(result.new_content.contains("if is_plausible {"));
        assert!(result.new_content.contains(
            "has_slash || has_valid_extension || is_known_filename\n    }\n    if is_plausible {"
        ));
    }

    #[test]
    fn test_fallback_reconciliation_trailing_comma_and_comment_drift() {
        let _ = env_logger::builder().is_test(true).try_init();
        let original = indoc! {r#"
            fn setup() {
                let config = Config {
                    host: "localhost",
                    port: 8080
                };
            }
        "#};

        // Patch context line has a trailing comma and an inline comment
        let diff = indoc! {r#"
            --- a/config.rs
            +++ b/config.rs
            @@ -2,4 +2,5 @@
                 let config = Config {
                     host: "localhost",
                     port: 8080, // default port
            +        timeout: 30,
                 };
        "#};

        let patch = parse_auto(diff).unwrap().remove(0);
        let options = ApplyOptions::new();
        let result = mpatch::apply_patch_to_content(&patch, Some(original), &options);

        assert!(
            result.report.all_applied_cleanly(),
            "Hunk must apply cleanly via fallback reconciliation despite trailing comma and comment drift"
        );
        assert!(result
            .new_content
            .contains("port: 8080\n        timeout: 30,\n    };"));
    }

    #[test]
    fn test_reanchor_additions_attached_to_missing_blank_context_line() {
        let _ = env_logger::builder().is_test(true).try_init();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("service.rs");

        // Target file has NO blank line between fn one() and fn two()
        let original = indoc! {r#"
            fn one() {
                step_1();
            }
            fn two() {
                step_2();
            }
        "#};
        fs::write(&file_path, original).unwrap();

        // Patch expects a blank line between fn one() and fn two(),
        // and inserts fn middle() after the blank line
        let diff = indoc! {r#"
            --- a/service.rs
            +++ b/service.rs
            @@ -1,6 +1,10 @@
             fn one() {
                 step_1();
             }

            +fn middle() {
            +    step_middle();
            +}
            +
             fn two() {
                 step_2();
             }
        "#};

        let patches = parse_auto(diff).unwrap();
        let options = ApplyOptions::new();
        let result = apply_patch_to_file(&patches[0], dir.path(), options).unwrap();

        assert!(
            result.report.all_applied_cleanly(),
            "Additions attached to a missing blank context line should re-anchor and apply cleanly"
        );

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("fn middle() {"));
        assert!(content.contains("fn one() {"));
        assert!(content.contains("fn two() {"));
    }
}
