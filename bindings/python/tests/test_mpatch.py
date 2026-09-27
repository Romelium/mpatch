import textwrap
from pathlib import Path

import pytest

import mpatch

# --- Test Data ---

MD_DIFF = textwrap.dedent("""\
    Here is a markdown diff:
    ```diff
    --- a/file.txt
    +++ b/file.txt
    @@ -1,3 +1,3 @@
     line 1
    -line 2
    +line two
     line 3
    ```
""")

RAW_DIFF = textwrap.dedent("""\
    --- a/src/main.rs
    +++ b/src/main.rs
    @@ -1 +1 @@
    -old
    +new
""")

CONFLICT_DIFF = textwrap.dedent("""\
    <<<<
    old logic
    ====
    new logic
    >>>>
""")

AIDER_DIFF = textwrap.dedent("""\
    src/app.py
    <<<<<<< ORIGINAL
    def run():
        old_runner()
    =======
    def run():
        new_runner()
    >>>>>>> UPDATED
""")

# --- Format Detection Tests ---


def test_detect_patch_format():
    assert mpatch.detect_patch(MD_DIFF) == "Markdown"
    assert mpatch.detect_patch(RAW_DIFF) == "Unified"
    assert mpatch.detect_patch(CONFLICT_DIFF) == "Conflict"
    assert mpatch.detect_patch(AIDER_DIFF) == "Aider"
    assert mpatch.detect_patch("Just some normal text") == "Unknown"


# --- Parsing Tests ---


def test_parse_auto():
    # Markdown
    patches = mpatch.parse_auto(MD_DIFF)
    assert len(patches) == 1
    patch = patches[0]
    assert Path(patch.file_path).as_posix() == "file.txt"
    assert len(patch.hunks) == 1
    assert patch.hunks[0].removed_lines == ["line 2"]
    assert patch.hunks[0].added_lines == ["line two"]
    assert patch.hunks[0].has_changes is True

    # Raw
    patches = mpatch.parse_auto(RAW_DIFF)
    assert len(patches) == 1
    assert Path(patches[0].file_path).as_posix() == "src/main.rs"

    # Conflict
    patches = mpatch.parse_auto(CONFLICT_DIFF)
    assert len(patches) == 1
    # Conflict markers default to 'patch_target' because they lack headers
    assert Path(patches[0].file_path).as_posix() == "patch_target"

    # Aider
    patches = mpatch.parse_auto(AIDER_DIFF)
    assert len(patches) == 1
    assert Path(patches[0].file_path).as_posix() == "src/app.py"
    assert patches[0].hunks[0].removed_lines == ["    old_runner()"]
    assert patches[0].hunks[0].added_lines == ["    new_runner()"]


def test_parse_aider():
    patches = mpatch.parse_aider(AIDER_DIFF)
    assert len(patches) == 1
    patch = patches[0]
    assert Path(patch.file_path).as_posix() == "src/app.py"
    assert len(patch.hunks) == 1
    assert patch.hunks[0].removed_lines == ["    old_runner()"]
    assert patch.hunks[0].added_lines == ["    new_runner()"]


def test_parse_errors():
    malformed_diff = "@@ -1 +1 @@\n-a\n+b\n"
    # mpatch.parse_patches is strict and requires headers
    with pytest.raises(mpatch.ParseError, match="without a file path header"):
        mpatch.parse_patches(malformed_diff)


# --- Pythonic API and Dunder Method Tests ---


def test_pythonic_patch_methods():
    patch = mpatch.parse_auto(RAW_DIFF)[0]

    # __len__
    assert len(patch) == 1

    # __getitem__
    hunk = patch[0]
    assert len(hunk) == 2  # 2 lines in RAW_DIFF hunk (-old, +new)

    # Slicing
    hunks_slice = patch[0:1]
    assert isinstance(hunks_slice, list)
    assert len(hunks_slice) == 1
    assert hunks_slice[0].added_lines == ["new"]

    # iteration (Python automatically uses __getitem__ and __len__ to iterate)
    hunks = list(patch)
    assert len(hunks) == 1

    # __bool__
    assert bool(patch) is True

    # __invert__ (Unary ~ operator)
    inverted = ~patch
    assert inverted.hunks[0].added_lines == ["old"]

    # Out of bounds
    with pytest.raises(IndexError):
        _ = patch[1]


def test_pythonic_result_methods():
    patch = mpatch.parse_auto(RAW_DIFF)[0]

    # Successful memory result
    mem_result = patch.apply_to_content("old\n")
    assert bool(mem_result) is True
    assert bool(mem_result.report) is True

    # Failing memory result
    fail_result = patch.apply_to_content("wrong\n")
    assert bool(fail_result) is False
    assert bool(fail_result.report) is False


def test_patch_oop_apply_to_content():
    original = "line 1\nline 2\nline 3\n"
    expected = "line 1\nline two\nline 3\n"
    patch = mpatch.parse_auto(MD_DIFF)[0]

    result = patch.apply_to_content(original)
    assert result.new_content == expected
    assert bool(result.report) is True


def test_patch_oop_apply_to_file(tmp_path: Path):
    target_file = tmp_path / "file.txt"
    target_file.write_text("line 1\nline 2\nline 3\n")
    patch = mpatch.parse_auto(MD_DIFF)[0]

    result = patch.apply_to_file(tmp_path)
    assert result.report.all_applied_cleanly is True
    assert target_file.read_text() == "line 1\nline two\nline 3\n"


def test_hunk_sequence_behavior():
    patch = mpatch.parse_auto(RAW_DIFF)[0]
    hunk = patch[0]

    # __len__
    assert len(hunk) == 2

    # __getitem__
    assert hunk[0] == "-old"
    assert hunk[-1] == "+new"

    # Slicing
    assert hunk[:] == ["-old", "+new"]

    # iteration
    assert list(hunk) == ["-old", "+new"]


def test_apply_result_sequence_behavior():
    patch = mpatch.parse_auto(RAW_DIFF)[0]
    result = patch.apply_to_content("old\n")
    report = result.report

    assert len(report) == 1
    assert report[0].status == "Applied"
    assert [status.status for status in report] == ["Applied"]


# --- In-Memory Patching Tests ---


def test_patch_content_exact():
    original = "line 1\nline 2\nline 3\n"
    expected = "line 1\nline two\nline 3\n"

    result = mpatch.patch_content(MD_DIFF, original=original)
    assert result == expected


def test_patch_content_fuzzy():
    # Original has an extra blank line and extra trailing whitespace,
    # making exact match fail. Fuzzy matching (default fuzz_factor=0.7)
    # should succeed and preserve the local changes.
    original = "line 1  \n\nline 2\nline 3\n"
    expected = "line 1  \n\nline two\nline 3\n"

    result = mpatch.patch_content(MD_DIFF, original=original)
    assert result == expected


def test_patch_content_fuzzy_fails_on_completely_different_file():
    original = "completely\ndifferent\ncontent\n"

    # Should raise ApplyError because it strictly expects success
    with pytest.raises(mpatch.ApplyError, match="Patch applied partially"):
        mpatch.patch_content(MD_DIFF, original=original)


def test_apply_patch_to_content_detailed_report():
    # apply_patch_to_content doesn't throw on partial apply,
    # it returns an InMemoryResult
    original = "wrong\ncontext\n"
    patch = mpatch.parse_auto(MD_DIFF)[0]

    result = mpatch.apply_patch_to_content(patch, original)

    assert result.new_content == "wrong\ncontext\n"  # unchanged

    report = result.report
    assert report.all_applied_cleanly is False
    assert report.has_failures is True
    assert report.failure_count == 1
    assert report.success_count == 0

    failures = report.failures
    assert len(failures) == 1
    assert failures[0].hunk_index == 1
    assert "below threshold" in failures[0].reason

    # Assert newly added error diagnostic properties
    assert failures[0].error_type == "FuzzyMatchBelowThreshold"
    assert failures[0].best_score is not None
    assert failures[0].threshold == 0.7
    assert failures[0].ambiguous_matches is None

    # Assert new hunk statuses properties
    statuses = report.hunk_results
    assert len(statuses) == 1
    assert statuses[0].status == "Failed"
    assert statuses[0].error_reason is not None


def test_hunk_match_replace_blocks():
    patch = mpatch.parse_auto(MD_DIFF)[0]
    hunk = patch[0]

    # Should accurately isolate what will be matched vs replacing content
    assert hunk.match_block == ["line 1", "line 2", "line 3"]
    assert hunk.replace_block == ["line 1", "line two", "line 3"]


# --- Filesystem Patching Tests ---


def test_apply_patch_to_file(tmp_path: Path):
    target_dir = tmp_path / "src"
    target_dir.mkdir()
    target_file = target_dir / "file.txt"
    target_file.write_text("line 1\nline 2\nline 3\n")

    patch = mpatch.parse_auto(MD_DIFF)[0]

    result = mpatch.apply_patch_to_file(patch, target_dir)
    assert result.report.all_applied_cleanly is True

    # Verify file was written
    assert target_file.read_text() == "line 1\nline two\nline 3\n"


def test_apply_patch_to_file_dry_run(tmp_path: Path):
    target_dir = tmp_path / "src"
    target_dir.mkdir()
    target_file = target_dir / "file.txt"
    target_file.write_text("line 1\nline 2\nline 3\n")

    patch = mpatch.parse_auto(MD_DIFF)[0]

    result = mpatch.apply_patch_to_file(patch, target_dir, dry_run=True)
    assert result.report.all_applied_cleanly is True

    # Check that diff is populated
    assert result.diff is not None
    assert "+line two" in result.diff

    # Verify file was NOT changed
    assert target_file.read_text() == "line 1\nline 2\nline 3\n"


def test_apply_directory_batch(tmp_path: Path):
    multi_file_diff = textwrap.dedent("""\
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
    """)

    (tmp_path / "file1.txt").write_text("foo\n")
    (tmp_path / "file2.txt").write_text("baz\n")

    # Apply directly using the high-level apply_directory helper
    success = mpatch.apply_directory(multi_file_diff, tmp_path)
    assert success is True

    assert (tmp_path / "file1.txt").read_text() == "bar\n"
    assert (tmp_path / "file2.txt").read_text() == "qux\n"


def test_apply_patches_to_dir_detailed(tmp_path: Path):
    # Tests the detailed BatchResult return type
    (tmp_path / "file.txt").write_text("line 1\nline 2\nline 3\n")
    patches = mpatch.parse_auto(MD_DIFF)

    batch_result = mpatch.apply_patches_to_dir(patches, tmp_path)
    assert batch_result.all_succeeded is True
    assert bool(batch_result) is True
    assert len(batch_result.hard_failures) == 0

    results_dict = batch_result.results
    # File paths in dict might be strings depending on OS, normalize to string
    file_key = str(Path("file.txt"))
    assert file_key in results_dict

    patch_result = results_dict[file_key]
    assert isinstance(patch_result, mpatch.PatchResult)
    assert patch_result.report.all_applied_cleanly is True

    # Validate Pythonic Mapping functionality
    assert len(batch_result) == 1
    assert file_key in batch_result
    assert isinstance(batch_result[file_key], mpatch.PatchResult)


def test_file_creation(tmp_path: Path):
    creation_diff = textwrap.dedent("""\
        --- /dev/null
        +++ b/new_file.txt
        @@ -0,0 +1,2 @@
        +hello
        +world
    """)

    patches = mpatch.parse_auto(creation_diff)
    assert patches[0].is_creation is True

    result = mpatch.apply_patch_to_file(patches[0], tmp_path)
    assert result.report.all_applied_cleanly is True

    assert (tmp_path / "new_file.txt").read_text() == "hello\nworld\n"


# --- Security Tests ---


def test_path_traversal_prevention(tmp_path: Path):
    evil_diff = textwrap.dedent("""\
        --- a/../evil.txt
        +++ b/../evil.txt
        @@ -0,0 +1 @@
        +hacked
    """)
    patch = mpatch.parse_auto(evil_diff)[0]

    with pytest.raises(
        mpatch.PathTraversalError, match="resolves outside the target directory"
    ):
        mpatch.apply_patch_to_file(patch, tmp_path)


# --- Patch Creation and Manipulation Tests ---


def test_create_and_invert_patch():
    old_text = 'fn main() {\n    println!("old");\n}\n'
    new_text = 'fn main() {\n    println!("new");\n}\n'

    # Create diff via static method
    patch = mpatch.Patch.from_texts(Path("src/main.rs"), old_text, new_text)

    assert Path(patch.file_path).as_posix() == "src/main.rs"
    assert len(patch.hunks) == 1
    assert patch.hunks[0].removed_lines == ['    println!("old");']
    assert patch.hunks[0].added_lines == ['    println!("new");']

    # Invert the patch
    inverted = patch.invert()
    assert inverted.hunks[0].removed_lines == ['    println!("new");']
    assert inverted.hunks[0].added_lines == ['    println!("old");']

    # Test batch inversion helper
    inverted_list = mpatch.invert_patches([patch])
    assert inverted_list[0].hunks[0].removed_lines == ['    println!("new");']


def test_create_unified_diff_str():
    old_text = "A\n"
    new_text = "B\n"
    diff_str = mpatch.create_unified_diff("target.txt", old_text, new_text)

    assert "--- a/target.txt" in diff_str
    assert "+++ b/target.txt" in diff_str
    assert "-A" in diff_str
    assert "+B" in diff_str


# --- Data Structure Representation Tests ---


def test_hunk_and_patch_repr():
    patch = mpatch.parse_auto(RAW_DIFF)[0]

    # Test __repr__ implementations
    assert repr(patch).startswith("<Patch file_path=")
    assert "hunks=1" in repr(patch)

    hunk = patch.hunks[0]
    assert repr(hunk).startswith("<Hunk old_start=1 new_start=1")
    assert "lines=2" in repr(hunk)

    # Test __str__ (should yield the valid unified diff)
    assert str(hunk) == "@@ -1,1 +1,1 @@\n-old\n+new\n"
    assert (
        str(patch)
        == "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,1 +1,1 @@\n-old\n+new\n"
    )


# --- Wildcard and Ellipsis Tests ---


def test_python_wildcard_aider_basic():
    original = textwrap.dedent("""\
        def compute():
            setup()
            prepare_data()
            run_calculation()
            cleanup()
            return 42
    """)
    diff = textwrap.dedent("""\
        calc.py
        <<<<<<< SEARCH
        def compute():
            ...
            run_calculation()
            ...
            return 42
        =======
        def compute():
            ...
            run_fast_calculation()
            ...
            return 100
        >>>>>>> REPLACE
    """)
    patched = mpatch.patch_content(diff, original=original)
    assert "setup()" in patched
    assert "prepare_data()" in patched
    assert "cleanup()" in patched
    assert "run_fast_calculation()" in patched
    assert "return 100" in patched
    assert "run_calculation()" not in patched


def test_python_wildcard_multi_segment_preserves_gaps():
    original = textwrap.dedent("""\
        class Pipeline:
            def step_a(self):
                step_a_internal()
            def step_b(self):
                step_b_internal()
            def step_c(self):
                step_c_internal()
    """)
    diff = textwrap.dedent("""\
        pipe.py
        <<<<<<< SEARCH
        class Pipeline:
            ...
            def step_b(self):
                step_b_internal()
            ...
        =======
        class Pipeline:
            ...
            def step_b(self):
                step_b_v2()
            ...
        >>>>>>> REPLACE
    """)
    patched = mpatch.patch_content(diff, original=original)
    assert "def step_a(self):" in patched
    assert "step_a_internal()" in patched
    assert "step_b_v2()" in patched
    assert "def step_c(self):" in patched
    assert "step_c_internal()" in patched


def test_python_wildcard_ellipsis_removal_deletes_gap():
    original = textwrap.dedent("""\
        def cleanup():
            acquire()
            legacy_item_1()
            legacy_item_2()
            release()
    """)
    diff = textwrap.dedent("""\
        clean.py
        <<<<<<< SEARCH
        def cleanup():
            acquire()
            ...
            release()
        =======
        def cleanup():
            acquire()
            release()
        >>>>>>> REPLACE
    """)
    patched = mpatch.patch_content(diff, original=original)
    assert "acquire()\n    release()" in patched
    assert "legacy_item_1" not in patched
    assert "legacy_item_2" not in patched


def test_python_wildcard_file_apply(tmp_path: Path):
    target = tmp_path / "app.py"
    target.write_text("def main():\n    init()\n    old_logic()\n    exit()\n")

    diff = textwrap.dedent("""\
        app.py
        <<<<<<< SEARCH
        def main():
            ...
            old_logic()
        =======
        def main():
            ...
            new_logic()
        >>>>>>> REPLACE
    """)
    patches = mpatch.parse_auto(diff)
    res = mpatch.apply_patch_to_file(patches[0], tmp_path)
    assert res.report.all_applied_cleanly is True
    assert target.read_text() == "def main():\n    init()\n    new_logic()\n    exit()\n"


def test_python_wildcard_runaway_gap_rejected():
    lines = ["def alpha():\n", "    return 1\n\n"]
    for i in range(350):
        lines.append(f"def intermediate_{i}(): pass\n")
    lines.append("\ndef beta():\n    return 1\n")
    original = "".join(lines)

    diff = textwrap.dedent("""\
        test.py
        <<<<<<< SEARCH
        def alpha():
            ...
            return 1
        =======
        def alpha():
            ...
            return 2
        >>>>>>> REPLACE
    """)
    with pytest.raises(mpatch.ApplyError):
        mpatch.patch_content(diff, original=original)


# --- False Positive and False Negative Detection Tests ---


def test_python_detect_patch_false_positive_rejection():
    # Markdown table
    table = textwrap.dedent("""\
        | Col A | Col B |
        | --- | --- |
        | 1 | 2 |
    """)
    assert mpatch.detect_patch(table) == "Unknown"

    # Markdown list
    bullets = textwrap.dedent("""\
        Summary:
        + Added feature X
        - Removed legacy code
    """)
    assert mpatch.detect_patch(bullets) == "Unknown"

    # Shift operators
    code = "let x = 1 << 4;\nlet y = x >> 2;\n"
    assert mpatch.detect_patch(code) == "Unknown"

    # Plain text
    assert mpatch.detect_patch("Just some normal conversation.") == "Unknown"


def test_python_detect_patch_false_negative_prevention():
    unified = textwrap.dedent("""\
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -old
        +new
    """)
    assert mpatch.detect_patch(unified) == "Unified"

    md = textwrap.dedent("""\
        ```diff
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -old
        +new
        ```
    """)
    assert mpatch.detect_patch(md) == "Markdown"

    conflict = textwrap.dedent("""\
        <<<<
        old
        ====
        new
        >>>>
    """)
    assert mpatch.detect_patch(conflict) == "Conflict"

    aider = textwrap.dedent("""\
        app.py
        <<<<<<< SEARCH
        old()
        =======
        new()
        >>>>>>> REPLACE
    """)
    assert mpatch.detect_patch(aider) == "Aider"


def test_python_wildcard_similar_functions_no_false_positive():
    original = textwrap.dedent("""\
        def handle_user(req):
            log("request")
            verify_token(req)
            return format_response(req)

        def handle_admin(req):
            log("request")
            verify_token(req)
            return format_response(req)
    """)
    diff = textwrap.dedent("""\
        api.py
        <<<<<<< SEARCH
        def handle_admin(req):
            ...
            return format_response(req)
        =======
        def handle_admin(req):
            ...
            audit_admin_access(req)
            return format_response(req)
        >>>>>>> REPLACE
    """)
    patched = mpatch.patch_content(diff, original=original)
    assert "audit_admin_access(req)" in patched
    assert (
        "def handle_user(req):\n    log(\"request\")\n    verify_token(req)\n    return format_response(req)"
        in patched
    )


def test_python_wildcard_empty_lines_gap_preserved():
    original = "def init():\n    setup()\n\n    \n    finish()\n"
    diff = textwrap.dedent("""\
        init.py
        <<<<<<< SEARCH
        def init():
            setup()
            ...
            finish()
        =======
        def init():
            setup()
            ...
            log_done()
            finish()
        >>>>>>> REPLACE
    """)
    patched = mpatch.patch_content(diff, original=original)
    assert patched == "def init():\n    setup()\n\n    \n    log_done()\n    finish()\n"


# --- Atomic (All-or-Nothing) Application Tests ---


def test_python_atomic_single_patch_success(tmp_path: Path):
    target = tmp_path / "app.txt"
    target.write_text("line 1\nline 2\n")

    diff = textwrap.dedent("""\
        --- a/app.txt
        +++ b/app.txt
        @@ -1,2 +1,2 @@
         line 1
        -line 2
        +line two
    """)
    patch = mpatch.parse_auto(diff)[0]
    res = patch.apply_to_file(tmp_path, atomic=True)
    assert res.report.all_applied_cleanly is True
    assert target.read_text() == "line 1\nline two\n"


def test_python_atomic_single_patch_partial_failure_leaves_disk_untouched(tmp_path: Path):
    target = tmp_path / "partial.txt"
    original = "line 1\nline 2\nline 3\n"
    target.write_text(original)

    diff = textwrap.dedent("""\
        --- a/partial.txt
        +++ b/partial.txt
        @@ -1,1 +1,1 @@
        -line 1
        +line one
        @@ -3,1 +3,1 @@
        -WRONG LINE
        +line three
    """)
    patch = mpatch.parse_auto(diff)[0]
    res = mpatch.apply_patch_to_file(patch, tmp_path, atomic=True)
    assert res.report.all_applied_cleanly is False
    # File on disk must remain 100% untouched
    assert target.read_text() == original


def test_python_atomic_multi_file_failure_discards_all(tmp_path: Path):
    f1 = tmp_path / "f1.txt"
    f2 = tmp_path / "f2.txt"
    f1.write_text("foo\n")
    f2.write_text("bar\n")

    diff = textwrap.dedent("""\
        --- a/f1.txt
        +++ b/f1.txt
        @@ -1 +1 @@
        -foo
        +foo_updated
        --- a/f2.txt
        +++ b/f2.txt
        @@ -1 +1 @@
        -WRONG
        +bar_updated
    """)

    patches = mpatch.parse_auto(diff)
    batch_res = mpatch.apply_patches_to_dir(patches, tmp_path, atomic=True)
    assert batch_res.all_applied_cleanly is False

    # Neither file should have been modified
    assert f1.read_text() == "foo\n"
    assert f2.read_text() == "bar\n"

    # High-level apply_directory helper with atomic=True
    success = mpatch.apply_directory(diff, tmp_path, atomic=True)
    assert success is False
    assert f1.read_text() == "foo\n"
    assert f2.read_text() == "bar\n"


# --- Similar v3.2.0 Integration Tests ---


def test_format_inline_diff():
    expected = ["fn calculate(x: i32) -> i32 {"]
    actual = ["fn calculate(x: i64) -> i32 {"]
    diff = mpatch.format_inline_diff(expected, actual)
    assert "calculate" in diff
    assert "i32" in diff
    assert "i64" in diff


def test_merge_three_way():
    base = "alpha\nbeta\ncommon\ngamma\ndelta\n"
    ours = "alpha\nbeta_mod\ncommon\ngamma\ndelta\n"
    theirs = "alpha\nbeta\ncommon\ngamma_mod\ndelta\n"
    merged, conflicted = mpatch.merge_three_way(base, ours, theirs)
    assert conflicted is False
    assert merged == "alpha\nbeta_mod\ncommon\ngamma_mod\ndelta\n"

    base_c = "val = 1\n"
    ours_c = "val = 2\n"
    theirs_c = "val = 3\n"
    merged_c, conflicted_c = mpatch.merge_three_way(
        base_c, ours_c, theirs_c, labels=("base", "ours", "theirs")
    )
    assert conflicted_c is True
    assert "<<<<<<< ours" in merged_c
    assert ">>>>>>> theirs" in merged_c


def test_python_format_inline_diff_multiline():
    expected = [
        "def handle_request(req, timeout):",
        "    verify_auth(req)",
        "    return process(req)",
    ]
    actual = [
        "def handle_request(req, timeout, retry=3):",
        "    verify_auth(req)",
        "    return process_request(req)",
    ]
    diff = mpatch.format_inline_diff(expected, actual)
    assert "handle_request" in diff
    assert "timeout" in diff
    assert "retry=3" in diff
    assert "process_request" in diff


def test_python_merge_three_way_identical_and_disjoint():
    # Identical edits resolve cleanly
    base = "a\nb\nc\n"
    ours = "a\nB_MOD\nc\n"
    theirs = "a\nB_MOD\nc\n"
    merged, conflicted = mpatch.merge_three_way(base, ours, theirs)
    assert conflicted is False
    assert merged == "a\nB_MOD\nc\n"

    # Disjoint edits resolve cleanly
    base = "first\nmiddle\nlast\n"
    ours = "FIRST_MOD\nmiddle\nlast\n"
    theirs = "first\nmiddle\nLAST_MOD\n"
    merged, conflicted = mpatch.merge_three_way(base, ours, theirs)
    assert conflicted is False
    assert merged == "FIRST_MOD\nmiddle\nLAST_MOD\n"


def test_python_merge_three_way_custom_labels():
    base = "var = 'base'\n"
    ours = "var = 'local'\n"
    theirs = "var = 'remote'\n"
    merged, conflicted = mpatch.merge_three_way(
        base, ours, theirs, labels=("BASE_VER", "LOCAL_VER", "REMOTE_VER")
    )
    assert conflicted is True
    assert "<<<<<<< LOCAL_VER" in merged
    assert "||||||| BASE_VER" in merged
    assert "=======" in merged
    assert ">>>>>>> REMOTE_VER" in merged


def test_python_large_scale_three_way_merge():
    # 300-line base file
    base_lines = [f"item_{i} = {i}\n" for i in range(300)]
    base = "".join(base_lines)

    # Ours updates items 0..50
    ours_lines = list(base_lines)
    for i in range(50):
        ours_lines[i] = f"item_{i} = 'ours_{i}'\n"
    ours = "".join(ours_lines)

    # Theirs updates items 250..300
    theirs_lines = list(base_lines)
    for i in range(250, 300):
        theirs_lines[i] = f"item_{i} = 'theirs_{i}'\n"
    theirs = "".join(theirs_lines)

    merged, is_conflicted = mpatch.merge_three_way(base, ours, theirs)
    assert is_conflicted is False
    assert "item_0 = 'ours_0'" in merged
    assert "item_49 = 'ours_49'" in merged
    assert "item_150 = 150" in merged
    assert "item_250 = 'theirs_250'" in merged
    assert "item_299 = 'theirs_299'" in merged


def test_python_large_scale_inline_diff():
    expected = [
        f"def compute_step_{i}(val: int, factor: float = 1.0) -> float:"
        for i in range(80)
    ]
    actual = [
        f"def compute_step_{i}(val: int, factor: float = 2.5, verbose: bool = False) -> float:"
        for i in range(80)
    ]
    diff = mpatch.format_inline_diff(expected, actual)
    assert "compute_step_0" in diff
    assert "compute_step_79" in diff
    assert "factor: float = 2.5" in diff
    assert "verbose: bool = False" in diff


def test_python_orphan_addition_rejected_during_fuzzy_match(tmp_path: Path):
    target_file = tmp_path / "service.py"
    original = textwrap.dedent("""\
        class DataService:
            def step_1(self): pass
            def step_2(self): pass
            def step_3(self): pass
            def step_4(self): pass
            def step_5(self): pass
            def step_6(self): pass
            def step_7(self): pass
            def step_8(self): pass
    """)
    target_file.write_text(original)

    diff = textwrap.dedent("""\
        ```diff
        --- a/service.py
        +++ b/service.py
        @@ -1,9 +1,11 @@
         class DataService:
             def step_1(self): pass
             def step_2(self): pass
             def step_3(self): pass
             def step_4(self): pass
             def step_5(self): pass
             def step_6(self): pass
             def step_7(self): pass
             def step_8(self): pass
             def legacy_cleanup(self):
        +        audit_log("cleaning")
                 pass
        ```
    """)
    patch = mpatch.parse_auto(diff)[0]
    result = patch.apply_to_file(tmp_path)
    assert result.report.all_applied_cleanly is False
    assert result.report.has_failures is True
    assert result.report.failures[0].error_type == "ContextNotFound"
    assert target_file.read_text() == original
