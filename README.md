# Mpatch

[![CI Status](https://img.shields.io/github/actions/workflow/status/romelium/mpatch/ci.yml?branch=main&style=flat-square&logo=githubactions&logoColor=white)](https://github.com/romelium/mpatch/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/romelium/mpatch?style=flat-square&logo=github&logoColor=white)](https://github.com/romelium/mpatch/releases/latest)
[![Crates.io](https://img.shields.io/crates/v/mpatch?style=flat-square&logo=rust&logoColor=white)](https://crates.io/crates/mpatch)
[![License: MIT](https://img.shields.io/crates/l/mpatch)](https://opensource.org/licenses/MIT)
[![Downloads](https://img.shields.io/crates/d/mpatch?style=flat-square)](https://crates.io/crates/mpatch)

`mpatch` is a contextual patching program/library tailored towards the "messy" world of modern software development. Different from standard `patch` and `git apply`, which require line number/context to be exact matches, `mpatch` utilizes **fuzzy matching**, looking for patches based on the context around the change in the code.

This tool was created to easily apply diffs produced by **LLMs (ChatGPT, Gemini, Claude, Copilot)**, since they tend to hallucinate the exact line numbers or the context around the patch.

> **🐍 Python Developer?** We have official Python bindings! Check out the [`mpatch` PyPI package and Python documentation](https://github.com/romelium/mpatch/tree/main/bindings/python).

---

## The Problem and the Solution

You ask an AI to modify some code. You get the diff. Except, the comment inside this code block is missing.

**Standard `patch`:** Won't work. The context isn't exact byte-for-byte match.
**`mpatch`:** Works! It notices that the code structure hasn't changed and applies the patch.

| Original File (Modified Locally) | AI-Generated Patch (Stale Context) | `patch` Result | `mpatch` Result |
| :--- | :--- | :--- | :--- |
| <pre>fn main() {<br>    // Forgettable comment<br>    println!("Hello");<br>}</pre> | <pre> fn main() {<br>-    println!("Hello");<br>+    println!("World");<br> }</pre> | <pre>❌ Failed<br>Hunk #1 FAILED at 1.<br>1 out of 1 hunk FAILED</pre> | <pre>fn main() {<br>    // Forgettable comment<br>    println!("World");<br>}</pre> |

---

### Supported Input Formats

`mpatch` automatically detects the input format. Flags are not needed.

1.  **Code Blocks in Markdown:** The default output format for language models. Uses either ` ```diff `, ` ```rust `, or other code blocks if they have diff headers.
2.  **Unified Diff:** Default `git diff` or `diff -u` format.
3.  **Aider Search/Replace Blocks:** Search and replace blocks with `<<<<<<< SEARCH` (or `ORIGINAL`), `=======`, and `>>>>>>> REPLACE` (or `UPDATED`). File paths are extracted automatically from the block header, preceding markdown text, or code comments. Supports wildcard ellipsis matching (`...`) to preserve intermediate code.
4.  **Conflict Markers:** Git-like conflict markers with `<<<<`, `====`, and `>>>>`. Defaults to `patch_target` on the filesystem, or patches strings directly in memory.

---

## Features

*   **🧠 Fuzzy Matching:** Runs a similarity algorithm to find the most suitable place to apply a patch if no exact matching is found. It supports stale context, whitespace changes, and minor code differences.
*   **🔒 Atomic (All-or-Nothing) Mode:** Pass `-a` / `--atomic` (or `--all-or-nothing`) to guarantee that changes are only written to disk if every single patch and hunk succeeds cleanly. If any hunk fails, the filesystem remains completely untouched.
*   **🤖 Format Independent:** Automatically recognizes and processes:
    *   **Markdown** diff code blocks (standard chat output format).
    *   **Unified Diff** (output from `git diff` or `diff -u` command).
    *   **Aider Search/Replace** blocks (popularized by Aider and LLM coding assistants).
    *   **Conflict Markers** (`<<<<`, `====`, `>>>>`).
*   **🔍 Wildcard / Ellipsis Matching:** Supports `...`, `…`, and comment-wrapped ellipsis lines (`// ... existing code ...`, `<!-- ... -->`, etc.) in search/replace blocks, preserving untouched code gaps while strictly preventing runaway gaps across functions.
*   **🔀 Three-Way Line Merge:** Built-in line-level 3-way merge engine (`merge_three_way`) with standard Diff3 conflict markers.
*   **💡 Sub-Line & Near-Miss Diagnostics:** Inline word-level diff visualization (`format_inline_diff`) for failed hunks and fuzzy file path suggestions (`suggest_close_file_paths`) when a target file is missing.
*   **📥 Standard Input (Stdin):** Pipe diffs directly from `git diff`, `cat`, or `curl` using `-` (e.g., `git diff | mpatch - ./src` or `cat patch.diff | mpatch -`).
*   **📋 Clipboard Support:** Input directly from your clipboard with `-c` or `--clipboard`.
*   **✨ Smarter Indentation:** Automatically indents added lines to be consistent with the target file. It translates tabs/spaces dynamically and preserves the target file's indentation style.
*   **🗑️ File Deletion:** Automatically removes the target file if the output becomes empty after patching.
*   **🛡️ Secure:** Path traversal is automatically prevented, ensuring no patch files can access or overwrite files outside the target directory.
*   **⚡ Fast & Concurrent:** Parallelizes fuzzy matching across all CPU cores via `rayon`, using histogram-based word diffing, zero-allocation window length iteration, and mathematical upper-bound pruning for rapid search.
*   **🔍 Dry Run:** Preview what the tool would do without modifying any files using `--dry-run`.

---

## Installation

### Option 1: Pre-compiled Binaries (Recommended)

We provide pre-compiled binaries for Windows, macOS, and Linux.

**Using [`cargo-binstall`](https://github.com/cargo-bins/cargo-binstall#installation) (Fastest):**
```bash
cargo binstall mpatch
```

**Manual Download:**
1.  Go to the [**Releases Page**](https://github.com/romelium/mpatch/releases).
2.  Download the archive for your architecture (see table below).
3.  Extract and add to your `PATH`.

| Platform | Architecture | Target | Notes |
| :--- | :--- | :--- | :--- |
| **macOS** | Universal | `universal-apple-darwin` | **Best for Mac.** Runs natively on M1/M2/M3 & Intel. |
| | x64 | `x86_64-apple-darwin` | Older Intel Macs. |
| | ARM64 | `aarch64-apple-darwin` | Apple Silicon (M1/M2/M3). |
| **Windows** | x64 | `x86_64-pc-windows-msvc` | Standard 64-bit Windows. |
| | ARM64 | `aarch64-pc-windows-msvc` | Surface Pro X, Parallels. |
| **Linux** | x64 | `x86_64-unknown-linux-gnu` | Ubuntu, Debian, Fedora, etc. |
| | x64 (Static) | `x86_64-unknown-linux-musl` | **Alpine Linux**, Docker containers. |
| | ARM64 | `aarch64-unknown-linux-gnu` | Raspberry Pi 4/5, AWS Graviton. |
| | ARM64 (Static) | `aarch64-unknown-linux-musl` | Alpine on ARM64. |
| | ARMv7 | `armv7-unknown-linux-gnueabihf` | Older Raspberry Pi (2/3), IoT. |
| | ARMv7 (Static) | `armv7-unknown-linux-musleabihf` | Static binaries for ARMv7. |

### Verifying Signatures

The integrity of all released binaries is verified with GPG. The `.sig` signature file along with our public key (`public.key`) will help you verify the authenticity of the downloaded files.

1.  **Import the public key:**
    ```bash
    gpg --import public.key
    ```

2.  **Verify the archive:**
    ```bash
    # Example for Linux x64
    gpg --verify mpatch-x86_64-unknown-linux-gnu-v1.6.4.tar.gz.sig mpatch-x86_64-unknown-linux-gnu-v1.6.4.tar.gz
    ```

### Option 2: Build from Source

```bash
cargo install mpatch
```

---

## CLI Usage

### Basic Application
Apply a patch file (Markdown, Diff, or Conflict markers) to a target directory (defaults to current directory `.` if omitted).

```bash
mpatch changes.md ./src

# Target directory defaults to '.' when omitted:
mpatch changes.md
```

### Atomic (All-or-Nothing) Mode
Only modify files on disk if **all** patches and hunks apply cleanly. If any hunk fails, zero files on disk are touched.

```bash
mpatch -a changes.md ./src
```

### From Standard Input (Stdin)
Pipe diffs directly from `git diff`, `cat`, or `curl` using `-`:

```bash
# Pipe from git diff
git diff | mpatch - ./src

# Reverse a piped patch
git diff | mpatch -R - ./src

# Pipe from a file or network stream
cat patch.diff | mpatch - ./src
curl -s https://example.com/fix.patch | mpatch - ./src

# Target directory defaults to current directory ('.') if omitted
cat patch.diff | mpatch -
```

### From Clipboard
Apply a patch copied to your clipboard directly to a target directory.

```bash
mpatch -c ./src
```

### Preview Changes (Dry Run)
See exactly what will happen without modifying files.

```bash
mpatch --dry-run changes.md ./src
```

### Adjusting Sensitivity
If `mpatch` is matching the wrong place, increase the strictness (default is 0.7). If it's failing to find a match, lower it.

```bash
# Stricter (needs 90% similarity)
mpatch --fuzz-factor 0.9 changes.md ./src

# Disable fuzzy matching (exact match only)
mpatch --fuzz-factor 0.0 changes.md ./src
```

### Reversing a Patch
Undo a previously applied patch (swaps additions and deletions).

```bash
mpatch -R changes.md ./src
```

### Debugging
If a patch fails, generate a comprehensive debug report (includes file states, logs, and diffs) to analyze why.

```bash
mpatch -vvvv changes.md ./src
# Generates: mpatch-debug-report-[timestamp].md
```

---

## Library Usage

`mpatch` is designed to be the patching engine for AI coding agents and tools. It exposes a robust Rust API.

Add to `Cargo.toml`:
```toml
[dependencies]
mpatch = "1.6.4"
```

### 1. Simple One-Shot (String to String)
Ideal for processing text in memory.

```rust
use mpatch::{patch_content_str, ApplyOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let original_code = "fn main() { println!(\"Old\"); }";

    // Input can be Markdown, Raw Diff, or Conflict Markers
    let patch_text = r#"
    ```diff
    --- a/main.rs
    +++ b/main.rs
    @@ -1 +1 @@
    -fn main() { println!("Old"); }
    +fn main() { println!("New"); }
    ```
    "#;

    let options = ApplyOptions::new(); // Default fuzz_factor: 0.7
    let new_code = patch_content_str(patch_text, Some(original_code), &options)?;

    assert_eq!(new_code, "fn main() { println!(\"New\"); }");
    Ok(())
}
```

### 2. Batch Application (File System)
Ideal for CLI tools applying multi-file patches.

```rust
use mpatch::{parse_auto, apply_patches_to_dir, ApplyOptions};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diff_content = include_str!("../tests/fixtures/changes.md");

    // 1. Parse (automatically detects format)
    let patches = parse_auto(diff_content)?;

    // 2. Apply
    let target_dir = Path::new("./src");
    let options = ApplyOptions::new();

    let results = apply_patches_to_dir(&patches, target_dir, options);

    if results.all_succeeded() {
        println!("All patches applied successfully!");
    } else {
        // Inspect specific failures
        for (path, result) in results.hard_failures() {
            eprintln!("Failed to process {}: {}", path.display(), result);
        }
    }
    Ok(())
}
```

### 3. Reversing Patches
Programmatically invert patches (additions become deletions and vice versa).

```rust
use mpatch::{parse_auto, invert_patches};

let diff_content = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new";
let patches = parse_auto(diff_content)?;
let reversed = invert_patches(&patches);

// Now apply `reversed` to undo changes
```

### 4. Deduplicating and Merging Patches
Consolidate multiple patch sections or blocks targeting the same file into unified `Patch` objects with combined hunks:

```rust
use mpatch::merge_patches;

// Automatically merges patches targeting the same file in O(N) time
let consolidated = merge_patches(patches);
```

### 4. Strict Apply-or-Fail Workflow
If you want to treat partial applications (where some hunks fail) as an error, use the `try_` variants.

```rust
use mpatch::{parse_single_patch, try_apply_patch_to_content, ApplyOptions, StrictApplyError};

let original_content = "fn main() { println!(\"Old\"); }";
let diff_content = "--- a/main.rs\n+++ b/main.rs\n@@ -1 +1 @@\n-fn main() { println!(\"Old\"); }\n+fn main() { println!(\"New\"); }";

let patch = parse_single_patch(diff_content)?;
let options = ApplyOptions::exact();

// Returns an Err if any hunk fails to apply
match try_apply_patch_to_content(&patch, Some(original_content), &options) {
    Ok(result) => println!("Success: {}", result.new_content),
    Err(StrictApplyError::PartialApply { report }) => {
        eprintln!("Patch partially applied. {} hunks failed.", report.failure_count());
    }
    Err(e) => eprintln!("Hard error: {}", e),
}
```

### 5. Creating Patches
You can also use `mpatch` to generate patches by comparing two strings.

```rust
use mpatch::Patch;

let old_text = "fn main() { println!(\"Old\"); }";
let new_text = "fn main() { println!(\"New\"); }";

// Create a patch with 3 lines of context
let patch = Patch::from_texts("src/main.rs", old_text, new_text, 3).unwrap();

println!("{}", patch);
```

### 6. Atomic (All-or-Nothing) Batch Application
Stage all changes in memory and commit to disk if and only if all hunks across all patches apply cleanly:

```rust
use mpatch::{parse_auto, apply_patches_to_dir_atomic, ApplyOptions};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diff_content = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
    let patches = parse_auto(diff_content)?;
    let target_dir = Path::new("./src");
    let options = ApplyOptions::exact();

    // Changes are only written to disk if ALL patches and hunks succeed
    let batch = apply_patches_to_dir_atomic(&patches, target_dir, options);
    if batch.all_applied_cleanly() {
        println!("All files updated cleanly on disk!");
    } else {
        eprintln!("Atomic batch failed: zero files on disk were modified.");
    }
    Ok(())
}
```

### 7. Aider Search/Replace with Wildcard Ellipsis Matching
Apply Aider search/replace blocks containing wildcard ellipsis lines (`...`) that match and preserve multi-line code gaps:

```rust
use mpatch::{patch_content_str, ApplyOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let original = "def compute(x):\n    setup()\n    res = x * 2\n    teardown()\n    return res\n";
    let aider_diff = r#"
math.py
<<<<<<< SEARCH
def compute(x):
    ...
    res = x * 2
=======
def compute(x):
    ...
    res = x * 4
>>>>>>> REPLACE
"#;

    let options = ApplyOptions::new();
    let new_code = patch_content_str(aider_diff, Some(original), &options)?;

    assert!(new_code.contains("res = x * 4"));
    assert!(new_code.contains("setup()"));
    assert!(new_code.contains("teardown()"));
    Ok(())
}
```

### 8. Three-Way Line Merging
Perform line-level 3-way merges backed by `similar::TextMerge`, automatically generating Diff3 conflict markers on conflicts:

```rust
use mpatch::merge_three_way;

let base   = "Apples\nBananas\nCherries\nDates\n";
let ours   = "Apples\nBlueberries\nCherries\nDates\n";
let theirs = "Apples\nBananas\nCherries\nDragonfruit\n";

let (merged, is_conflicted) = merge_three_way(base, ours, theirs, None);
assert!(!is_conflicted);
assert_eq!(merged, "Apples\nBlueberries\nCherries\nDragonfruit\n");
```

### 9. Inline Word Diffs & Path Suggestions
Visualize sub-line changes with word-level highlights and suggest close matching file paths when a target is not found:

```rust
use mpatch::{format_inline_diff, suggest_close_file_paths};
use std::path::Path;

// Highlight sub-line word additions and deletions
let expected = vec!["fn compute(x: i32) -> i32 {"];
let actual = vec!["fn compute(x: i64) -> i32 {"];
let diff_view = format_inline_diff(&expected, &actual);
println!("{}", diff_view);

// Find close file path candidates when a patch targets a misspelled path
let suggestions = suggest_close_file_paths(Path::new("calculate.rs"), Path::new("./src"), 3);
for candidate in suggestions {
    println!("Did you mean: {}", candidate.display());
}
```

---

## About the Conflict Markers Format

Though supported by `mpatch`, this file format (`<<<<`, `====`, `>>>>`) **does not include any file path information**.

*   **Using `mpatch` from CLI:** If the provided file contains only conflict markers, `mpatch` will try to patch a file called `patch_target`.
*   **Using `mpatch` as a library:** The format can be used by the `patch_content_str` function if your target file content is stored in memory.

In case of a multiple file patching or using `mpatch` from CLI, it is recommended to use the **Unified Diffs** format (with `---` and `+++`).

---

## Performance

Fuzzy match is $O(N \times M)$ operation. To ensure speed on large files:
1.  **Heuristics:** Before doing a fuzzy match, `mpatch` tries to find both exact and "whitespace-insensitive" exact matches.
2.  **Anchoring:** `mpatch` tries to look for unique lines in the patch file to shrink the match range.
3.  **Parallelism:** If a full scan is required, it uses [Rayon](https://github.com/rayon-rs/rayon) to parallelize the workload.

*Benchmark code can be found in `benches/mpatch_bench.rs`. To run, use `cargo bench`.*

---

## Contributing

Contributions are welcome!

*   **Bug Reports:** Please run `mpatch -vvvv ...` to generate a debug report and attach it to your issue.
*   **Development:**
    ```bash
    git clone https://github.com/romelium/mpatch.git
    cd mpatch
    cargo test
    ```

## License

MIT License. See [LICENSE](LICENSE) for details.