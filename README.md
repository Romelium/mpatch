# Mpatch

[![CI Status](https://img.shields.io/github/actions/workflow/status/romelium/mpatch/ci.yml?branch=main&style=flat-square&logo=githubactions&logoColor=white)](https://github.com/romelium/mpatch/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/romelium/mpatch?style=flat-square&logo=github&logoColor=white)](https://github.com/romelium/mpatch/releases/latest)
[![Crates.io](https://img.shields.io/crates/v/mpatch?style=flat-square&logo=rust&logoColor=white)](https://crates.io/crates/mpatch)
[![License: MIT](https://img.shields.io/crates/l/mpatch)](https://opensource.org/licenses/MIT)
[![Rust Version](https://img.shields.io/badge/rust-1.83.0%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Downloads](https://img.shields.io/crates/d/mpatch?style=flat-square)](https://crates.io/crates/mpatch)

**A smart, context-aware patch tool for the modern developer.**

`mpatch` applies diffs using **context-aware fuzzy matching** instead of strict line numbers. It locates changes based on the surrounding code, making it resilient to code drift. It works seamlessly with AI-generated suggestions, raw diffs, conflict markers, and markdown snippets.

---

## Why `mpatch`?

The primary motivation for `mpatch` comes from working with Large Language Models (LLMs).

When you ask an AI like ChatGPT, Claude, or Copilot to refactor code, it often provides the changes in a convenient markdown format inside code blocks. **However, you can't trust that the line numbers are correct.** Sometimes, even the surrounding context lines aren't a perfect, character-for-character match to your current code. A standard `patch` command will often fail in these situations.

**This is the core problem `mpatch` was built to solve.**

It intelligently ignores line numbers and uses a fuzzy, context-based search to find where the patch *should* apply. This makes it highly resilient to the small inaccuracies common in AI-generated diffs, allowing you to apply them with confidence.

This same logic makes it perfect for other common developer scenarios where patches are less formal:
*   **Code Snippets:** Using a diff copied from a GitHub comment, a blog post, or a team chat.
*   **Iterative Development:** Applying a patch to a branch that has slightly diverged from where the patch was created.

---

## Core Features

*   **Format Agnostic:** Automatically detects and parses **Markdown** code blocks, raw **Unified Diffs**, and **Conflict Markers** (`<<<<`, `====`, `>>>>`). Just pass the file, and `mpatch` figures it out.
*   **Context-Driven:** Primarily finds patch locations by matching context lines, making it resilient to preceding file changes. It intelligently uses the `@@ ... @@` line numbers as a hint to resolve ambiguity when the same context appears in multiple places.
*   **Fuzzy Matching:** If an exact context match isn't found, `mpatch` uses a sophisticated similarity algorithm to find the *best* fuzzy match. This logic can handle cases where lines have been added or removed near the patch location, allowing patches to apply even when the surrounding context has moderately diverged.
*   **Highly Performant:** The most computationally intensive task—fuzzy searching—is parallelized to take full advantage of multi-core processors, ensuring fast performance even on large files.
*   **Safe & Secure:** Includes a `--dry-run` mode to preview changes and built-in protection against path traversal attacks.
*   **Flexible:** Handles multiple files and multiple hunks in a single pass. It correctly processes file creations, modifications, and deletions (by removing all content from a file).
*   **Informative Logging:** Adjustable verbosity levels (`-v`, `-vv`) to see exactly what `mpatch` is doing.

---

## Supported Input Formats

`mpatch` automatically detects the format based on the **content** of your input file, regardless of the file extension.

### 1. Markdown
Commonly output by AI (ChatGPT, Claude, etc.). `mpatch` scans for code blocks containing diffs. It supports variable-length fences (e.g., ` ```` `) and correctly ignores nested code blocks (like diffs inside documentation examples).

````markdown
Here is the fix:
```diff
--- a/src/main.rs
@@ -1 +1 @@
-println!("Old");
+println!("New");
```
````

### 2. Raw Unified Diff
Standard output from tools like `git diff`.

```diff
--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1 @@
-println!("Old");
+println!("New");
```

### 3. Conflict Markers
Often used by AI (Gemini 3, etc.) to show "before" and "after" states without full headers.

```text
<<<<
println!("Old");
====
println!("New");
>>>>
```

---

## Library Usage

While `mpatch` is a powerful CLI tool, it is also designed to be the patching engine for your own AI tools and workflows.

If you are building an AI coding agent, a CLI utility, or a CI bot, you don't need to write your own fuzzy matching logic or Markdown parsers. `mpatch` exposes its core logic as a robust Rust crate.

**Why use `mpatch` in your tool?**
*   **Robust Parsing:** Feed it raw LLM output (Markdown, conflict markers, or diffs), and it extracts the patches automatically.
*   **Safety:** Built-in path traversal checks prevent AI hallucinations from writing outside the target directory.
*   **Detailed Diagnostics:** Get structured error reports (`ApplyResult`) to tell your agent exactly *why* a patch failed (e.g., "Context not found at line 50"), allowing for self-correction loops.

Add `mpatch` to your `Cargo.toml`:
```bash
cargo add mpatch
```

### Simple One-Shot Patching

Here's a basic example of how to parse a diff and apply it to a string in memory:

````rust
use mpatch::{patch_content_str, ApplyOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Define the original content and the diff.
    let original_content = "fn main() {\n    println!(\"Hello, world!\");\n}\n";
    let diff_content = r#"
        A markdown file with a diff block.
        ```diff
        --- a/src/main.rs
        +++ b/src/main.rs
        @@ -1,3 +1,3 @@
         fn main() {
        -    println!("Hello, world!");
        +    println!("Hello, mpatch!");
         }
        ```
    "#;

    // 2. Call the one-shot function to parse and apply the patch.
    let options = ApplyOptions::new();
    let new_content = patch_content_str(diff_content, Some(original_content), &options)?;

    // 3. Verify the new content.
    let expected_content = "fn main() {\n    println!(\"Hello, mpatch!\");\n}\n";
    assert_eq!(new_content, expected_content);

    Ok(())
}
````

### Advanced Usage: Applying to Files

For more control, or to apply patches to a directory structure, use `parse_auto` and `apply_patches_to_dir`.

```rust
use mpatch::{parse_auto, apply_patches_to_dir, ApplyOptions};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diff_content = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new";

    // 1. Parse the content (automatically detects format)
    let patches = parse_auto(diff_content)?;

    // 2. Apply all patches to the target directory
    let target_dir = Path::new("./project");
    let options = ApplyOptions::new();

    let results = apply_patches_to_dir(&patches, target_dir, options);

    if results.all_succeeded() {
        println!("Patches applied successfully!");
    }

    Ok(())
}
```

### Strict Application (Apply-or-Fail)

For workflows where any failed hunk should be treated as an error, use the `try_` variants (e.g., `try_apply_patch_to_content`). These return a `Result` that fails if any hunk cannot be applied, simplifying error handling.

```rust
use mpatch::{parse_single_patch, try_apply_patch_to_content, ApplyOptions, StrictApplyError};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new";
    let patch = parse_single_patch(diff)?;
    let content = "old\n";

    match try_apply_patch_to_content(&patch, Some(content), &ApplyOptions::new()) {
        Ok(result) => println!("Success: {}", result.new_content),
        Err(StrictApplyError::PartialApply { report }) => {
            println!("Partial failure: {} hunks failed", report.failure_count());
        }
        Err(e) => println!("Error: {}", e),
    }
    Ok(())
}
```

For even more advanced use cases, such as iterating through hunks one by one, check out the [**library documentation on docs.rs**](https://docs.rs/mpatch).

---

## Feature Flags

*   `parallel` (**Enabled by default**): Enables parallel processing for the fuzzy matching algorithm using `rayon`. This significantly speeds up searching in large files. Disable this feature to reduce binary size or for environments that do not support threading (e.g., WASM).

    ```toml
    [dependencies]
    mpatch = { version = "1.3.0", default-features = false }
    ```

---

## CLI Installation

### Method 1: Using `cargo-binstall` (Recommended)

For users with the [Rust toolchain](https://rustup.rs/), `cargo-binstall` is the fastest way to install `mpatch`. It downloads pre-compiled binaries, avoiding a local build.

First, install `cargo-binstall` if you don't have it. Here are a few quick methods:

**On Linux and macOS:**
```bash
curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
```
*Or, if you use Homebrew:*
```bash
brew install cargo-binstall
```

**On Windows (PowerShell):**
```powershell
Set-ExecutionPolicy Unrestricted -Scope Process; iex (iwr "https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.ps1").Content
```

Once `cargo-binstall` is installed, you can install `mpatch`:

```bash
cargo binstall mpatch
```

### Method 2: From GitHub Releases (Manual)

Pre-compiled binaries for various platforms are available for direct download.

1.  Navigate to the [**GitHub Releases page**](https://github.com/romelium/mpatch/releases).
2.  Find the latest release and download the archive that matches your system (e.g., `.tar.gz` for Linux/macOS, `.zip` for Windows).
3.  Extract the `mpatch` executable (`mpatch.exe` on Windows).
4.  Move the executable to a directory in your system's `PATH` (e.g., `/usr/local/bin` on Linux/macOS).

Binaries are available for the following targets:

| OS      | Architecture | Target Triple                  |
|---------|--------------|--------------------------------|
| Linux   | x86-64       | `x86_64-unknown-linux-gnu`     |
| Linux   | x86-64       | `x86_64-unknown-linux-musl` (static) |
| Linux   | ARM64        | `aarch64-unknown-linux-gnu`    |
| macOS   | Intel        | `x86_64-apple-darwin`          |
| macOS   | Apple Silicon| `aarch64-apple-darwin`         |
| Windows | x86-64       | `x86_64-pc-windows-msvc`       |
| Windows | x86 (32-bit) | `i686-pc-windows-msvc`         |
| Windows | ARM64        | `aarch64-pc-windows-msvc`      |

### Method 3: From Crates.io (Build from Source)

If you have the [Rust toolchain](https://rustup.rs/) installed, you can compile and install `mpatch` from the official package registry:

```bash
cargo install mpatch
```

### Method 4: From Source (for Developers)

To build the very latest development version or to contribute to the project:

```bash
# Install directly from the main branch of the repository
cargo install --git https://github.com/romelium/mpatch.git

# Or, to work on the code locally:
git clone https://github.com/romelium/mpatch.git
cd mpatch
cargo install --path .
```

---

## CLI Usage

### Basic Command

```bash
mpatch [OPTIONS] <INPUT_FILE> <TARGET_DIR>
```

### Verifying Changes with `--dry-run`

Before modifying any files, you can preview the exact changes using the `-n` or `--dry-run` flag. You can provide a markdown file, a raw diff, or a file with conflict markers.

```bash
mpatch --dry-run changes.md my-project/
```

This will produce a diff of the proposed changes for each file, printed directly to your terminal:

```
----- Proposed Changes for src/main.rs -----
--- a
+++ b
@@ -1,5 +1,5 @@
 fn main() {
-    // This is the original program
-    println!("Hello, world!");
+    // This is the updated program
+    println!("Hello, mpatch!");
 }

------------------------------------
DRY RUN completed. No files were modified.
```

### Applying Changes

Once you are confident in the proposed changes, run the command without `--dry-run`. Use `-v` for informational output.

```bash
mpatch -v changes.md my-project/
```

You will see a confirmation log:
```
Found 1 patch operation(s) to perform.
Fuzzy matching enabled with threshold: 0.70

>>> Operation 1/1
Applying patch to: src/main.rs
  Applying Hunk 1/1...
  Successfully wrote changes to 'my-project/src/main.rs'

--- Summary ---
Successful operations: 1
Failed operations:     0
```

### Key Options

*   `-n`, `--dry-run`: Show what changes would be made without modifying any files.
*   `-f`, `--fuzz-factor <FACTOR>`: Set the similarity threshold for fuzzy matching, from `0.0` (disabled) to `1.0` (exact match). Default is `0.7`.
*   `-v`, `--verbose`: Increase logging output. Use `-v` for info, `-vv` for debug, `-vvv` for trace, and `-vvvv` to generate a comprehensive debug report file.

---

## Troubleshooting

If a patch doesn't apply as expected, the best first step is to increase the logging verbosity to understand what `mpatch` is doing.

*   **Run with `-v`:** This shows which files and hunks are being processed.
*   **Run with `-vv`:** This provides detailed debug information, including why a hunk might have failed to apply (e.g., "ambiguous match", "context not found").
*   **Run with `-vvv`:** This enables trace-level logging, showing the fuzzy matching scores and every step of the decision-making process.

### Generating a Debug Report

For complex issues, the easiest way to gather all necessary information for a bug report is to use the `-vvvv` flag.

```bash
mpatch -vvvv changes.md my-project/
```

This command will:
1.  Print full trace logs to your terminal.
2.  Create a file named `mpatch-debug-report-[timestamp].md` in your current directory.

This single markdown file contains everything needed to reproduce the issue: the command you ran, system information, the full input patch file, the original content of all target files, the final content of all target files after patching, and the complete trace log.

---

## License

This project is licensed under [MIT LICENSE](LICENSE)

## Contributing

Contributions are welcome! Whether it's a bug report, a feature request, or a pull request, your input is valued.

### Reporting Issues

When opening an issue, the best way to help us is to provide a debug report.

1.  Run your command again with the `-vvvv` flag.
    ```bash
    mpatch -vvvv [YOUR_ARGS]
    ```
2.  This will create a `mpatch-debug-report-[timestamp].md` file.
3.  Create a new issue on GitHub.
4.  Drag and drop the generated `.md` file into the issue description to attach it.
5.  Add any additional comments about what you expected to happen versus what actually happened.

The report includes the original and final state of all affected files, which is crucial for debugging. This self-contained report gives us all the context we need to investigate the problem efficiently.

### Pull Requests

1.  Fork the repository.
2.  Create a new branch for your feature or bug fix.
3.  Make your changes.
4.  Add tests for your changes in the `tests/` directory.
5.  Ensure all tests pass by running `cargo test`.
6.  Format your code with `cargo fmt`.
7.  Submit a pull request with a clear description of your changes.
