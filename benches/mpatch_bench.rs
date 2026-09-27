use criterion::{black_box, criterion_group, criterion_main, Criterion};
use indoc::indoc;
use mpatch::{
    apply_patch_to_content, detect_patch, find_hunk_location_in_lines, format_inline_diff,
    merge_patches, merge_three_way, parse_aider, parse_conflict_markers, parse_diffs,
    parse_patches, ApplyOptions, Hunk, Patch, WindowLengthIter,
};

// --- Detecting Benchmarks ---

fn detecting_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("Detecting");

    let md_diff = "```diff\n--- a/file\n+++ b/file\n@@ -1 +1 @@\n-a\n+b\n```";
    let raw_diff = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-a\n+b";
    let conflict_diff = "<<<<\na\n====\nb\n>>>>";
    let aider_diff = "file.txt\n<<<<<<< SEARCH\na\n=======\nb\n>>>>>>> REPLACE";

    group.bench_function("detect_markdown", |b| {
        b.iter(|| detect_patch(black_box(md_diff)))
    });

    group.bench_function("detect_unified", |b| {
        b.iter(|| detect_patch(black_box(raw_diff)))
    });

    group.bench_function("detect_conflict", |b| {
        b.iter(|| detect_patch(black_box(conflict_diff)))
    });

    group.bench_function("detect_aider", |b| {
        b.iter(|| detect_patch(black_box(aider_diff)))
    });

    group.finish();
}

// --- Parsing Benchmarks ---

fn parsing_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("Parsing");

    // Simple, single-hunk diff
    let simple_diff = indoc! {r#"
        A markdown file with some text.
        ```diff
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1,3 +1,3 @@
         fn main() {
        -    println!("Hello, world!");
        +    println!("Hello, mpatch!");
         }
        ```
    "#};
    group.bench_function("simple_diff", |b| {
        b.iter(|| parse_diffs(black_box(simple_diff)).unwrap())
    });

    // Diff with multiple files in one block
    let multi_file_diff = indoc! {r#"
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
    group.bench_function("multi_file_diff", |b| {
        b.iter(|| parse_diffs(black_box(multi_file_diff)).unwrap())
    });

    // Diff with many hunks for a single file
    let mut large_diff_content =
        "```diff\n--- a/large_file.txt\n+++ b/large_file.txt\n".to_string();
    for i in 0..100 {
        large_diff_content.push_str(&format!(
            "@@ -{},3 +{},3 @@\n context line {}\n-old line {}\n+new line {}\n",
            i * 5 + 1,
            i * 5 + 1,
            i,
            i,
            i
        ));
    }
    large_diff_content.push_str("```");
    group.bench_function("large_diff_100_hunks", |b| {
        b.iter(|| parse_diffs(black_box(&large_diff_content)).unwrap())
    });

    // Large markdown file with one diff block at the end to test scanning speed
    let mut large_markdown = "Lorem ipsum dolor sit amet...\n".repeat(1000);
    large_markdown.push_str(simple_diff);
    group.bench_function("large_markdown_scan", |b| {
        b.iter(|| parse_diffs(black_box(&large_markdown)).unwrap())
    });

    let raw_diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-foo\n+bar\n";
    group.bench_function("raw_diff", |b| {
        b.iter(|| parse_patches(black_box(raw_diff)).unwrap())
    });

    let conflict_diff = "<<<<\nfoo\n====\nbar\n>>>>\n";
    group.bench_function("conflict_markers", |b| {
        b.iter(|| parse_conflict_markers(black_box(conflict_diff)))
    });

    let aider_diff = "file.txt\n<<<<<<< SEARCH\nfoo\n=======\nbar\n>>>>>>> REPLACE\n";
    group.bench_function("aider_blocks", |b| {
        b.iter(|| parse_aider(black_box(aider_diff)))
    });

    // Patch Merging Benchmark (100 interleaved patches across 10 files)
    let mut patches_to_merge = Vec::with_capacity(100);
    for i in 0..100 {
        let file_num = i % 10;
        patches_to_merge.push(Patch {
            file_path: std::path::PathBuf::from(format!("src/module_{}.rs", file_num)),
            hunks: vec![Hunk {
                lines: vec![format!("-old_{}", i), format!("+new_{}", i)],
                old_start_line: Some(i * 10 + 1),
                new_start_line: Some(i * 10 + 1),
            }],
            ends_with_newline: true,
        });
    }
    group.bench_function("merge_patches_100_interleaved", |b| {
        b.iter(|| merge_patches(black_box(patches_to_merge.clone())))
    });
    group.finish();
}

// --- Finding Benchmarks ---

fn finding_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("Finding");
    group.sample_size(10);

    let options_exact = ApplyOptions::exact();
    let options_fuzzy = ApplyOptions::new();

    // Setup large file content as a vector of lines
    let mut large_file_lines = Vec::new();
    for i in 0..10000 {
        large_file_lines.push(format!("This is line number {}", i));
    }

    let exact_patch = parse_diffs(indoc! {r#"
        ```diff
        --- a/large_file.txt
        +++ b/large_file.txt
        @@ -5000,5 +5000,5 @@
         This is line number 4999
         This is line number 5000
        -This is line number 5001
        +THIS LINE WAS CHANGED
         This is line number 5002
         This is line number 5003
        ```
    "#})
    .unwrap()
    .remove(0);
    let exact_hunk = exact_patch.hunks[0].clone();

    group.bench_function("exact_match_large_file", |b| {
        b.iter(|| {
            criterion::black_box(find_hunk_location_in_lines(
                black_box(&exact_hunk),
                black_box(&large_file_lines),
                &options_exact,
            ))
        });
    });

    // Setup for fuzzy matching with anchor
    let mut fuzzy_anchor_lines = large_file_lines.clone();
    // Insert a line to break the exact match but keep anchors intact
    fuzzy_anchor_lines.insert(100, "An extra line to break exact match".to_string());

    group.bench_function("fuzzy_match_large_file_with_anchor", |b| {
        b.iter(|| {
            criterion::black_box(find_hunk_location_in_lines(
                black_box(&exact_hunk),
                black_box(&fuzzy_anchor_lines),
                &options_fuzzy,
            ))
        });
    });

    // Setup worst-case fuzzy match (no anchor, full scan)
    let repetitive_lines: Vec<String> = (0..10000)
        .map(|_| "println!(\"hello world\");".to_string())
        .collect();
    let worst_case_patch = parse_diffs(indoc! {r#"
        ```diff
        --- a/repetitive.txt
        +++ b/repetitive.txt
        @@ -5000,5 +5000,5 @@
         This is a unique context line 1
        -This is a unique line to be removed
        +This is a unique line to be added
         This is a unique context line 2
        ```
    "#})
    .unwrap()
    .remove(0);
    let worst_case_hunk = worst_case_patch.hunks[0].clone();

    group.bench_function("fuzzy_match_worst_case_no_anchor", |b| {
        b.iter(|| {
            criterion::black_box(find_hunk_location_in_lines(
                black_box(&worst_case_hunk),
                black_box(&repetitive_lines),
                &options_fuzzy,
            ))
        });
    });

    group.finish();
}

// --- Applying Benchmarks ---

/// Helper struct to manage state for apply benchmarks, keeping setup code clean.
struct ApplyBenchSetup {
    patch: Patch,
    initial_content: String,
}

fn applying_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("Applying");
    group.sample_size(10);

    let options_exact = ApplyOptions::exact();
    let options_fuzzy = ApplyOptions::new();

    // --- Benchmark 1: File Creation ---
    let creation_setup = ApplyBenchSetup {
        patch: parse_diffs(indoc! {r#"
            ```diff
            --- a/new_file.txt
            +++ b/new_file.txt
            @@ -0,0 +1,2 @@
            +Hello
            +New World
            ```
        "#})
        .unwrap()
        .remove(0),
        initial_content: String::new(), // Not used, but struct requires it
    };

    group.bench_function("file_creation", |b| {
        b.iter(|| {
            criterion::black_box(apply_patch_to_content(
                black_box(&creation_setup.patch),
                black_box(None),
                &options_exact,
            ));
        });
    });

    // --- Benchmark 2: Exact Match on a large file ---
    let mut large_file_content = String::new();
    for i in 0..10000 {
        large_file_content.push_str(&format!("This is line number {}\n", i));
    }
    let exact_large_setup = ApplyBenchSetup {
        patch: parse_diffs(indoc! {r#"
            ```diff
            --- a/large_file.txt
            +++ b/large_file.txt
            @@ -5000,5 +5000,5 @@
             This is line number 4999
             This is line number 5000
            -This is line number 5001
            +THIS LINE WAS CHANGED
             This is line number 5002
             This is line number 5003
            ```
        "#})
        .unwrap()
        .remove(0),
        initial_content: large_file_content,
    };

    group.bench_function("exact_match_large_file", |b| {
        b.iter(|| {
            criterion::black_box(apply_patch_to_content(
                black_box(&exact_large_setup.patch),
                black_box(Some(&exact_large_setup.initial_content)),
                &options_exact,
            ));
        });
    });

    // --- Benchmark 3: Fuzzy Match on a large file (anchor found) ---
    let mut fuzzy_target_content = exact_large_setup.initial_content.clone();
    // Insert a line to break the exact match but keep anchors intact
    fuzzy_target_content.insert_str(100, "An extra line to break exact match\n");
    let fuzzy_anchor_setup = ApplyBenchSetup {
        patch: exact_large_setup.patch.clone(), // Use the same patch
        initial_content: fuzzy_target_content,
    };

    group.bench_function("fuzzy_match_large_file_with_anchor", |b| {
        b.iter(|| {
            criterion::black_box(apply_patch_to_content(
                black_box(&fuzzy_anchor_setup.patch),
                black_box(Some(&fuzzy_anchor_setup.initial_content)),
                &options_fuzzy,
            ));
        });
    });

    // --- Benchmark 4: Fuzzy Match worst-case (no anchor, full scan) ---
    let repetitive_content = "println!(\"hello world\");\n".repeat(10000);
    let worst_case_setup = ApplyBenchSetup {
        patch: parse_diffs(indoc! {r#"
            ```diff
            --- a/repetitive.txt
            +++ b/repetitive.txt
            @@ -5000,5 +5000,5 @@
             This is a unique context line 1
            -This is a unique line to be removed
            +This is a unique line to be added
             This is a unique context line 2
            ```
        "#})
        .unwrap()
        .remove(0),
        initial_content: repetitive_content,
    };

    group.bench_function("fuzzy_match_worst_case_no_anchor", |b| {
        b.iter(|| {
            // We expect this to fail (return (..., false)), but we're measuring the time it takes to search.
            criterion::black_box(apply_patch_to_content(
                black_box(&worst_case_setup.patch),
                black_box(Some(&worst_case_setup.initial_content)),
                &options_fuzzy,
            ));
        });
    });

    // --- Benchmark 5: Ambiguous exact match resolved by line hint ---
    let ambiguous_content = indoc! {"
        // Block 1
        fn duplicate() {
            println!(\"hello\");
        }
        // ...
        // Block 2
        fn duplicate() {
            println!(\"hello\");
        }
    "}
    .repeat(100); // Make the file larger to make the search non-trivial

    let ambiguous_setup = ApplyBenchSetup {
        patch: parse_diffs(indoc! {r#"
            ```diff
            --- a/ambiguous.txt
            +++ b/ambiguous.txt
            @@ -7,3 +7,3 @@
             fn duplicate() {
            -    println!("hello");
            +    println!("world");
             }
            ```
        "#})
        .unwrap()
        .remove(0),
        initial_content: ambiguous_content,
    };

    group.bench_function("ambiguous_exact_match_resolved_by_hint", |b| {
        b.iter(|| {
            criterion::black_box(apply_patch_to_content(
                black_box(&ambiguous_setup.patch),
                black_box(Some(&ambiguous_setup.initial_content)),
                &options_exact,
            ));
        });
    });

    group.finish();
}

// --- Similar v3.2.0 Capabilities Benchmarks ---

fn similar_v3_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("SimilarV3");
    group.sample_size(10);

    // 1. Large 3-Way Merge Benchmark (1,000 lines, 200 functions)
    let mut base = String::with_capacity(25_000);
    let mut ours = String::with_capacity(25_000);
    let mut theirs = String::with_capacity(25_000);

    for i in 0..200 {
        base.push_str(&format!("fn comp_{}() {{\n    step();\n}}\n\n", i));
        if i % 4 == 0 {
            ours.push_str(&format!("fn comp_{}() {{\n    ours_step();\n}}\n\n", i));
        } else {
            ours.push_str(&format!("fn comp_{}() {{\n    step();\n}}\n\n", i));
        }
        if i % 4 == 2 {
            theirs.push_str(&format!("fn comp_{}() {{\n    theirs_step();\n}}\n\n", i));
        } else {
            theirs.push_str(&format!("fn comp_{}() {{\n    step();\n}}\n\n", i));
        }
    }

    group.bench_function("merge_three_way_200_functions", |b| {
        b.iter(|| {
            black_box(merge_three_way(
                black_box(&base),
                black_box(&ours),
                black_box(&theirs),
                None,
            ))
        });
    });

    // 2. Large Inline Diff Formatting Benchmark (100 functions)
    let mut expected_lines = Vec::with_capacity(100);
    let mut actual_lines = Vec::with_capacity(100);
    for i in 0..100 {
        expected_lines.push(format!(
            "pub fn handle_event_{}(ctx: &mut Context, id: u32) -> Result<(), Error> {{",
            i
        ));
        actual_lines.push(format!("pub fn handle_event_{}(ctx: &mut Context, id: u64, flags: EventFlags) -> Result<(), AppError> {{", i));
    }
    let exp_refs: Vec<&str> = expected_lines.iter().map(|s| s.as_str()).collect();

    group.bench_function("format_inline_diff_100_lines", |b| {
        b.iter(|| {
            black_box(format_inline_diff(
                black_box(&exp_refs),
                black_box(&actual_lines),
            ))
        });
    });

    // 3. Patience Diff Generation Benchmark (1,000 lines)
    let mut orig_text = String::with_capacity(25_000);
    let mut mod_text = String::with_capacity(25_000);
    for i in 0..100 {
        orig_text.push_str(&format!(
            "fn task_{}() {{\n    compute_a();\n    compute_b();\n}}\n\n",
            i
        ));
        if i % 10 == 0 {
            mod_text.push_str(&format!("fn injected_helper_{}() {{}}\n\n", i));
        }
        if i % 4 == 0 {
            mod_text.push_str(&format!(
                "fn task_{}() {{\n    compute_a_fast();\n    compute_b();\n}}\n\n",
                i
            ));
        } else {
            mod_text.push_str(&format!(
                "fn task_{}() {{\n    compute_a();\n    compute_b();\n}}\n\n",
                i
            ));
        }
    }

    group.bench_function("patience_diff_generation_1000_lines", |b| {
        b.iter(|| {
            black_box(
                Patch::from_texts("bench.rs", black_box(&orig_text), black_box(&mod_text), 3)
                    .unwrap(),
            )
        });
    });

    group.finish();
}

// --- Window Length Search Benchmark ---

fn window_length_search_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group("WindowLengthSearch");

    group.bench_function("radiating_window_lengths_50", |b| {
        b.iter(|| {
            let iter = WindowLengthIter::new(black_box(25), black_box(1), black_box(50));
            iter.fold(0usize, |acc, len| acc.wrapping_add(len))
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    detecting_benches,
    parsing_benches,
    finding_benches,
    applying_benches,
    similar_v3_benches,
    window_length_search_benches
);
criterion_main!(benches);
