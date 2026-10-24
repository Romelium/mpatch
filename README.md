# Mpatch

[![CI Status](https://img.shields.io/github/actions/workflow/status/romelium/mpatch/ci.yml?branch=main&style=flat-square&logo=githubactions&logoColor=white)](https://github.com/romelium/mpatch/actions/workflows/ci.yml)
[![Latest Release](https://img.shields.io/github/v/release/romelium/mpatch?style=flat-square&logo=github&logoColor=white)](https://github.com/romelium/mpatch/releases/latest)
[![Crates.io](https://img.shields.io/crates/v/mpatch?style=flat-square&logo=rust&logoColor=white&cacheSeconds=300)](https://crates.io/crates/mpatch)
[![docs.rs](https://img.shields.io/docsrs/mpatch?style=flat-square&logo=docsdotrs)](https://docs.rs/mpatch)
[![PyPI](https://img.shields.io/pypi/v/mpatch?style=flat-square&logo=pypi&logoColor=white)](https://pypi.org/project/mpatch/)
[![Downloads](https://img.shields.io/crates/d/mpatch?style=flat-square)](https://crates.io/crates/mpatch)
[![License: MIT](https://img.shields.io/crates/l/mpatch?style=flat-square)](https://opensource.org/licenses/MIT)

`mpatch` applies diffs by finding where they belong in the code, not by trusting line numbers. When the context in a diff doesn't match the file exactly, it uses **fuzzy matching** to find the most similar location and applies the change there. It ships as a CLI, a Rust library, and [Python bindings](https://github.com/romelium/mpatch/tree/main/bindings/python).

It was built for diffs written by **LLMs (ChatGPT, Gemini, Claude)**, which often get line numbers wrong, drop lines from the context, or describe code that has changed since they last saw it.

**`mpatch` is built with `mpatch`.** Changes to its own [`src/lib.rs`](https://github.com/romelium/mpatch/blob/main/src/lib.rs) (over 15,000 lines) and [test file](https://github.com/romelium/mpatch/blob/main/tests/patch_apply_tests.rs) (over 16,000 lines) are applied with it.

```bash
cargo binstall mpatch          # or download a binary, see Installation
mpatch reply.md ./my-project   # apply every diff in a saved AI reply
mpatch -c                      # apply the diff on your clipboard to the current directory
```

## Why mpatch?

Your file has a comment that the AI never saw:

```rust
fn main() {
    let name = "World";
    // Forgettable comment
    println!("Hello, {name}!");
    println!("Goodbye, {name}!");
}
```

The AI's diff leaves it out:

```diff
--- a/main.rs
+++ b/main.rs
@@ -1,5 +1,5 @@
 fn main() {
-    let name = "World";
+    let name = "Ferris";
     println!("Hello, {name}!");
-    println!("Goodbye, {name}!");
+    println!("See you, {name}!");
 }
```

| Tool | Result |
| :--- | :--- |
| `git apply` | ❌ `error: patch failed: main.rs:1` |
| `patch -p1` | ❌ `Hunk #1 FAILED at 1.` |
| `mpatch` | ✅ Applies both changes and keeps the comment |

```rust
fn main() {
    let name = "Ferris";
    // Forgettable comment
    println!("Hello, {name}!");
    println!("See you, {name}!");
}
```

Each `@@` block of a diff is a *hunk*. `patch` tolerates mismatched context at the edges of a hunk, but not inside it. `mpatch` scores candidate locations by similarity and applies the hunk at the best one above a threshold you control.

## Will it change the wrong code?

`mpatch` only applies a hunk where it is confident the hunk belongs:

- An exact match is tried first. A fuzzy match is used only when there is none, and the location must be at least 70% similar (change this with `-f`).
- If the same code appears in more than one place, the diff's line numbers pick the nearest. If they can't decide, the hunk fails instead of guessing.
- A hunk that fails changes nothing. `mpatch` prints the closest location it found and how similar it was, and exits with status `1`.
- `-n` shows the exact changes before anything is written, `-a` writes nothing unless every hunk applies, and a patch can't touch files outside the target directory.

Review the result as you would any AI edit, for example with `git diff`. If a diff was made from exactly the files you're applying it to, such as your own `git diff` output, `git apply` is stricter and also handles renames and binary files.

## Supported Input Formats

The format is detected automatically, so no flags are needed. One input can hold several patches for several files.

| Format | What it looks like |
| :--- | :--- |
| **Markdown code blocks** | A chat reply with ` ```diff ` blocks, or any code block (` ```rust `, …) that contains diff headers. Prose around the blocks is ignored. |
| **Unified diff** | Output of `git diff`, `diff -u`, or `diff -Naur`. Each file's hunks need the `--- a/<path>` and `+++ b/<path>` lines above them, so copy those too. |
| **Aider search/replace blocks** | `<<<<<<< SEARCH`, `=======`, `>>>>>>> REPLACE`, with the file path just above the block. |
| **Conflict markers** | `<<<<`, `====`, `>>>>`. These don't name a file, so they're mainly for patching a string with the library. The CLI applies them to a file named `patch_target`. |

## Features

- **Works with stale diffs:** applies changes even when the code has moved, the line numbers are wrong, or the AI left out or changed lines around the edit.
- **Matches your indentation:** added code is re-indented to fit the file, whether it uses tabs or spaces.
- **Understands placeholders:** `// ... existing code ...` and `...` in search/replace blocks stand for the code they skip, and that code is kept.
- **Doesn't duplicate changes:** re-running a patch usually skips changes that are already in the file instead of adding them again.
- **Leaves the rest alone:** line endings and the final newline stay as they were.
- **No `-p` option:** `a/` and `b/` path prefixes are handled for you.
- **Fast on large files:** the search starts at the likeliest spots and uses every CPU core.

## Installation

Download a binary for Windows, macOS, or Linux (x64 and ARM) from the [Releases page](https://github.com/romelium/mpatch/releases), extract it, and put `mpatch` on your `PATH`. You don't need Rust. Archives are named by Rust target, such as `x86_64-pc-windows-msvc`. On macOS, `universal-apple-darwin` runs on both Apple Silicon and Intel. On Linux, the `-musl` builds are static, for Alpine and minimal containers. You can ignore the `-full` archives unless you need debug symbols.

The binaries aren't notarized by Apple, so macOS may refuse to run one you downloaded with a browser. Run `xattr -d com.apple.quarantine mpatch` once to allow it.

With Rust installed:

```bash
cargo binstall mpatch   # downloads the pre-built binary
cargo install mpatch    # builds from source
```

`pip install mpatch` installs the [Python library](https://github.com/romelium/mpatch/tree/main/bindings/python), not the `mpatch` command.

Each archive has a GPG signature (`.sig`), and each release includes the signing key as `public.key`:

```bash
gpg --import public.key
gpg --verify mpatch-x86_64-unknown-linux-gnu-v1.8.0.tar.gz.sig mpatch-x86_64-unknown-linux-gnu-v1.8.0.tar.gz
```

## CLI Usage

```text
mpatch [OPTIONS] [INPUT_FILE] [TARGET_DIR]
```

| Option | Description |
| :--- | :--- |
| `INPUT_FILE` | The patch to apply. Use `-` for standard input. Can be omitted when piping or using `-c`. |
| `TARGET_DIR` | The directory that the patch's paths are relative to. Defaults to `.`. |
| `-n`, `--dry-run` | Show the changes that would be made without modifying any files. |
| `-a`, `--atomic` | Write files only if every patch and hunk applies (alias: `--all-or-nothing`). |
| `-f`, `--fuzz-factor <F>` | Minimum similarity for a fuzzy match, from `0.0` to `1.0` (default `0.7`). Higher is stricter; `0` disables fuzzy matching. |
| `-R`, `--reverse` | Undo the patch by swapping additions and deletions. |
| `-c`, `--clipboard` | Read the patch from the clipboard. |
| `-v`, `--verbose` | Repeat for more detail: `-v` info, `-vv` debug, `-vvv` trace, `-vvvv` also writes a debug report. |

```bash
mpatch changes.md ./src        # apply a patch file to ./src
mpatch changes.md              # apply to the current directory
git diff | mpatch - ./src      # read from standard input ('-' is optional when piping)
mpatch -n changes.md           # preview the changes
mpatch -a changes.md           # change nothing unless every hunk applies
mpatch -R changes.md           # undo a patch
mpatch -f 0.9 changes.md       # require closer matches
mpatch -vvvv changes.md        # write mpatch-debug-report-<timestamp>.md
```

On success `mpatch` prints nothing. Use `-n` to see the changes first, or `-v` for a summary. The changes shown by `-n` go to standard output, and warnings and errors go to standard error.

`mpatch` exits with status `1` if any hunk fails to apply or an error occurs, and `0` otherwise. If the input contains no patch, it prints a warning and exits with `0`. Without `--atomic`, hunks that apply are written even if others fail, so files can be left partly patched. The log lists every failed hunk.

`-f 0` turns off fuzzy matching, but exact matches and matches that ignore trailing whitespace are still used. It's a good choice when a diff should match exactly, such as `git diff` output applied to the same code. It still doesn't behave exactly like GNU `patch`, so preview the result with `--dry-run` first.

The debug report contains the file states, logs, and diffs. Paths such as your home directory are redacted, but the contents of the files being patched are included.

### Limitations

`mpatch` applies content changes. It stops without modifying any files if the input contains:

- Git renames or copies (`rename from` / `copy from`)
- Git binary patches (`GIT binary patch`)
- Symlink or submodule modes (anything other than `100644` and `100755`)
- Context diffs (`diff -c`). Regenerate them with `diff -u`.

Executable-bit changes are applied only on Unix. On Windows, `mpatch` applies the content and prints a warning.

## Library Usage

Use the library to build `mpatch` into an editor, a coding agent, or another tool.

```toml
[dependencies]
mpatch = "1.8.0"
```

The default features are `parallel` (multi-threaded fuzzy search with Rayon) and `clipboard` (the CLI's `-c` flag). Library users can turn off both, or keep only `parallel`:

```toml
[dependencies]
mpatch = { version = "1.8.0", default-features = false, features = ["parallel"] }
```

Patch a string in memory:

````rust
use mpatch::{patch_content_str, ApplyOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let original_code = "fn main() { println!(\"Old\"); }\n";

    // Any supported format works: Markdown, unified diff, Aider blocks, or conflict markers.
    let patch_text = r#"```diff
--- a/main.rs
+++ b/main.rs
@@ -1 +1 @@
-fn main() { println!("Old"); }
+fn main() { println!("New"); }
```
"#;

    let new_code = patch_content_str(patch_text, Some(original_code), &ApplyOptions::new())?;

    assert_eq!(new_code, "fn main() { println!(\"New\"); }\n");
    Ok(())
}
````

Apply a multi-file patch to a directory:

````rust,no_run
use mpatch::{parse_auto, apply_patches_to_dir, ApplyOptions};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let diff_content = r#"```diff
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
"#;

    let patches = parse_auto(diff_content)?;
    let results = apply_patches_to_dir(&patches, Path::new("./src"), ApplyOptions::new());

    for (path, error) in results.hard_failures() {
        eprintln!("{}: {}", path.display(), error);
    }
    Ok(())
}
````

Each failed hunk comes with its reason: no matching location, a best match below the threshold (with its location and score), or a match in more than one place. A coding agent can send that back to the model and ask for a corrected diff.

The [API documentation](https://docs.rs/mpatch) covers the rest: atomic application, strict `try_` variants that treat a partial apply as an error, reversing and merging patches, creating patches from two texts, Aider blocks with wildcards, and three-way merges.

From Python, `pip install mpatch` and call `mpatch.patch_content(diff, original=code)`. See the [Python documentation](https://github.com/romelium/mpatch/tree/main/bindings/python).

## Contributing

To report a bug, run the failing command with `-vvvv` and attach the generated debug report to your issue. To work on `mpatch`:

```bash
git clone https://github.com/romelium/mpatch.git
cd mpatch
cargo test
cargo bench   # benchmarks

# Python bindings
cd bindings/python
pip install -e ".[test]"
pytest
```

## License

MIT License. See [LICENSE](LICENSE) for details.
