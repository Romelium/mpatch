# mpatch for Python

[![PyPI version](https://img.shields.io/pypi/v/mpatch.svg?style=flat-square&logo=pypi&logoColor=white)](https://pypi.org/project/mpatch/)
[![Python versions](https://img.shields.io/pypi/pyversions/mpatch.svg?style=flat-square&logo=python&logoColor=white)](https://pypi.org/project/mpatch/)
[![Type Hints](https://img.shields.io/badge/Typing-Strict-success.svg?style=flat-square)](https://pypi.org/project/mpatch/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](https://opensource.org/licenses/MIT)

`mpatch` applies diffs by finding where they belong in the code, not by trusting line numbers. When the context in a diff doesn't match the file exactly, it uses **fuzzy matching** to find the most similar location and applies the change there. This package is the Python interface to the Rust library. The command-line tool and the Rust docs are in the [main repository](https://github.com/romelium/mpatch).

It was built for diffs written by **LLMs (ChatGPT, Gemini, Claude)**, which often get line numbers wrong, drop lines from the context, or describe code that has changed since they last saw it.

**`mpatch` is built with `mpatch`.** Changes to its own [`src/lib.rs`](https://github.com/romelium/mpatch/blob/main/src/lib.rs) (over 15,000 lines) and [test file](https://github.com/romelium/mpatch/blob/main/tests/patch_apply_tests.rs) (over 16,000 lines) are applied with it.

## Installation

```bash
pip install mpatch
```

Wheels are published for Windows, macOS, and Linux (including Alpine) on x64 and ARM, for CPython 3.8 and later and free-threaded CPython 3.14. On other platforms, pip builds from source, which needs a [Rust toolchain](https://rustup.rs).

This installs the library only. For the `mpatch` command, see [Installation](https://github.com/romelium/mpatch#installation) in the main repository.

## Quick Start

Your file has a comment that the AI never saw, and its diff leaves it out:

````python
import mpatch

original = """\
def greet(name):
    greeting = "Hello"
    # Forgettable comment
    print(f"{greeting}, {name}!")
    print(f"Goodbye, {name}!")
"""

reply = """\
Here's the change:

```diff
--- a/greet.py
+++ b/greet.py
@@ -1,4 +1,4 @@
 def greet(name):
-    greeting = "Hello"
+    greeting = "Hi"
     print(f"{greeting}, {name}!")
-    print(f"Goodbye, {name}!")
+    print(f"See you, {name}!")
```
"""

print(mpatch.patch_content(reply, original=original))
````

```text
def greet(name):
    greeting = "Hi"
    # Forgettable comment
    print(f"{greeting}, {name}!")
    print(f"See you, {name}!")
```

`git apply` and `patch` both reject this diff, because the comment breaks its context. `mpatch` applies both changes and keeps the comment. You can pass the whole chat reply: the prose around the diff is ignored.

`patch_content` raises `mpatch.ApplyError` if a hunk (one `@@` block of a diff) can't be placed, and `mpatch.ParseError` if the text contains no patch.

## Will it change the wrong code?

`mpatch` only applies a hunk where it is confident the hunk belongs:

- An exact match is tried first. A fuzzy match is used only when there is none, and the location must be at least 70% similar (change this with `fuzz_factor`).
- If the same code appears in more than one place, the diff's line numbers pick the nearest. If they can't decide, the hunk fails instead of guessing.
- A hunk that fails changes nothing, and the result tells you the closest location found and how similar it was.
- `dry_run=True` returns the changes without writing them, `atomic=True` writes nothing unless every hunk applies, and a patch that points outside the target directory raises `mpatch.PathTraversalError`.

Review the result as you would any AI edit.

## Supported Formats

The format is detected automatically, and one input can hold patches for several files.

- **Markdown code blocks**, such as a chat reply with ` ```diff ` blocks. Prose around them is ignored.
- **Unified diffs** from `git diff` or `diff -u`. Each file's hunks need the `--- a/<path>` and `+++ b/<path>` lines above them.
- **Aider search/replace blocks** (`<<<<<<< SEARCH`, `=======`, `>>>>>>> REPLACE`), with the file path just above the block. Placeholders like `# ... existing code ...` stand for the code they skip.
- **Conflict markers** (`<<<<`, `====`, `>>>>`). These don't name a file, so use them with `patch_content`.

## Applying to a Directory

```python
from pathlib import Path
import mpatch

# A small project to patch
project = Path("my_project")
project.mkdir(exist_ok=True)
(project / "config.py").write_text("DEBUG = True\nPORT = 8000\n")

diff = """\
--- a/config.py
+++ b/config.py
@@ -1,2 +1,2 @@
-DEBUG = True
+DEBUG = False
 PORT = 8000
"""

if mpatch.apply_directory(diff, project, atomic=True):
    print("Every hunk applied")
```

With `atomic=True`, `apply_directory` writes nothing unless every hunk of every patch applies, and returns `True` only if they all did. Without it, the hunks that apply are written even if others fail, and it returns `True` unless a file couldn't be read or written, so check for failed hunks as shown below. Add `dry_run=True` to check a diff without changing any files.

## Handling Failures

To find out what failed, apply the patches one at a time. Each failure says which hunk failed and why, which a coding agent can send back to the model to ask for a corrected diff.

```python
from pathlib import Path
import mpatch

project = Path("my_project")
project.mkdir(exist_ok=True)
(project / "app.py").write_text("x = 1\ny = 2\n")

reply = """\
--- a/app.py
+++ b/app.py
@@ -1,2 +1,2 @@
 totally = different
-content = here
+content = there
"""

for patch in mpatch.parse_auto(reply):
    result = patch.apply_to_file(project)
    for failure in result.report.failures:
        print(f"{patch.file_path}, hunk {failure.hunk_index}: {failure.reason}")
```

```text
app.py, hunk 1: Best fuzzy match at line 1 (score: 0.495) was below threshold (0.700)
```

`failure.error_type` names the reason, such as `"ContextNotFound"`, `"FuzzyMatchBelowThreshold"`, or `"AmbiguousExactMatch"`. Fuzzy failures also carry `best_score`, `threshold`, and the near-miss location in `location_start` and `location_length`.

## More

- `parse_auto` and `parse_single_patch` return `Patch` objects, which you can index and slice by hunk and reverse with `~patch`.
- `merge_patches` combines patches that target the same file.
- `create_unified_diff` and `Patch.from_texts` make a diff from two strings.
- `merge_three_way` does a line-level three-way merge with conflict markers.

Every function has type hints and a docstring, so your editor and `help(mpatch)` show the details.

## Exceptions

All exceptions derive from `mpatch.MpatchError`:

- `mpatch.ParseError`: the input contains no patch, or a patch is malformed.
- `mpatch.ApplyError`: `patch_content` couldn't apply a hunk, or a file couldn't be read or written.
- `mpatch.PathTraversalError`: a patch points outside the target directory. It's a subclass of `ApplyError`.

## Threads

`mpatch` releases the GIL while it works, so you can apply patches from several threads, or from `asyncio` with `asyncio.to_thread`.

## License

MIT License. See [LICENSE](https://github.com/romelium/mpatch/blob/main/LICENSE).
