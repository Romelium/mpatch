use criterion::{black_box, criterion_group, criterion_main, Criterion};
use indoc::indoc;
use mpatch::{apply_patch_to_content, parse_diffs, Patch};

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
            apply_patch_to_content(black_box(&creation_setup.patch), black_box(None), 0.0);
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
            apply_patch_to_content(
                black_box(&exact_large_setup.patch),
                black_box(Some(&exact_large_setup.initial_content)),
                0.0,
            );
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
            apply_patch_to_content(
                black_box(&fuzzy_anchor_setup.patch),
                black_box(Some(&fuzzy_anchor_setup.initial_content)),
                0.7,
            );
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
            apply_patch_to_content(
                black_box(&worst_case_setup.patch),
                black_box(Some(&worst_case_setup.initial_content)),
                0.7,
            );
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
            apply_patch_to_content(
                black_box(&ambiguous_setup.patch),
                black_box(Some(&ambiguous_setup.initial_content)),
                0.0,
            );
        });
    });

    group.finish();
}

criterion_group!(benches, parsing_benches, applying_benches);
criterion_main!(benches);
        );
    });

    // --- Benchmark 3: Fuzzy Match on a large file (anchor found) ---
    let mut fuzzy_target_content = exact_large_setup.initial_content.clone();
    // Insert a line to break the exact match but keep anchors intact
    fuzzy_target_content.insert_str(100, "An extra line to break exact match\n");
    let fuzzy_anchor_setup = ApplyBenchSetup {
        patch: exact_large_setup.patch.clone(), // Use the same patch
        initial_content: fuzzy_target_content,
        target_filename: "large_file.txt".to_string(),
    };

    group.bench_function("fuzzy_match_large_file_with_anchor", |b| {
        b.iter_with_setup(
            || {
                let dir = tempdir().unwrap();
                let file_path = dir.path().join(&fuzzy_anchor_setup.target_filename);
                fs::write(file_path, &fuzzy_anchor_setup.initial_content).unwrap();
                (dir, fuzzy_anchor_setup.patch.clone())
            },
            |(dir, patch)| {
                apply_patch(black_box(&patch), black_box(dir.path()), false, 0.7).unwrap();
            },
        );
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
        target_filename: "repetitive.txt".to_string(),
    };

    group.bench_function("fuzzy_match_worst_case_no_anchor", |b| {
        b.iter_with_setup(
            || {
                let dir = tempdir().unwrap();
                let file_path = dir.path().join(&worst_case_setup.target_filename);
                fs::write(file_path, &worst_case_setup.initial_content).unwrap();
                (dir, worst_case_setup.patch.clone())
            },
            |(dir, patch)| {
                // We expect this to fail (return Ok(false)), but we're measuring the time it takes to search.
                apply_patch(black_box(&patch), black_box(dir.path()), false, 0.7).unwrap();
            },
        );
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
        target_filename: "ambiguous.txt".to_string(),
    };

    group.bench_function("ambiguous_exact_match_resolved_by_hint", |b| {
        b.iter_with_setup(
            || {
                let dir = tempdir().unwrap();
                let file_path = dir.path().join(&ambiguous_setup.target_filename);
                fs::write(file_path, &ambiguous_setup.initial_content).unwrap();
                (dir, ambiguous_setup.patch.clone())
            },
            |(dir, patch)| {
                apply_patch(black_box(&patch), black_box(dir.path()), false, 0.0).unwrap();
            },
        );
    });

    group.finish();
}

criterion_group!(benches, parsing_benches, applying_benches);
criterion_main!(benches);
