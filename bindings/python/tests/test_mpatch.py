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


def test_parse_single_patch():
    patch = mpatch.parse_single_patch(MD_DIFF)
    assert Path(patch.file_path).as_posix() == "file.txt"
    assert len(patch.hunks) == 1

    # Zero patches
    with pytest.raises(mpatch.ParseError, match="No patches were found"):
        mpatch.parse_single_patch("plain text with no patches")

    # Multiple patches in one diff
    multi = textwrap.dedent("""\
        --- a/f1.txt
        +++ b/f1.txt
        @@ -1 +1 @@
        -a
        +b
        --- a/f2.txt
        +++ b/f2.txt
        @@ -1 +1 @@
        -c
        +d
    """)
    with pytest.raises(mpatch.ParseError, match="multiple files"):
        mpatch.parse_single_patch(multi)


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
    assert failures[0].location_start is not None
    assert failures[0].location_length is not None
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


def test_hunk_required_match_span():
    patch = mpatch.parse_auto(MD_DIFF)[0]
    hunk = patch[0]
    assert hunk.required_match_span >= 1

    empty_hunk = mpatch.Hunk([" line 1", " line 2"])
    assert empty_hunk.has_changes is False
    assert empty_hunk.required_match_span == 0


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
    assert batch_result.has_failures is False
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
    # Normalization should handle forward slashes
    assert "file.txt" in batch_result
    assert isinstance(batch_result["file.txt"], mpatch.PatchResult)
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

    # Test setters for hunks and ends_with_newline
    patch.ends_with_newline = False
    assert patch.ends_with_newline is False
    patch.ends_with_newline = True
    assert patch.ends_with_newline is True

    new_hunk = mpatch.Hunk(["-old", "+new"])
    patch.hunks = [new_hunk]
    assert len(patch.hunks) == 1
    assert patch.hunks[0].added_lines == ["new"]

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
    assert (
        target.read_text() == "def main():\n    init()\n    new_logic()\n    exit()\n"
    )


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
        'def handle_user(req):\n    log("request")\n    verify_token(req)\n    return format_response(req)'
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


def test_python_atomic_single_patch_partial_failure_leaves_disk_untouched(
    tmp_path: Path,
):
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
    assert batch_res.has_failures is True

    # Neither file should have been modified
    assert f1.read_text() == "foo\n"
    assert f2.read_text() == "bar\n"

    # High-level apply_directory helper with atomic=True
    success = mpatch.apply_directory(diff, tmp_path, atomic=True)
    assert success is False
    assert f1.read_text() == "foo\n"
    assert f2.read_text() == "bar\n"

    # Dedicated atomic batch function
    batch_res2 = mpatch.apply_patches_to_dir_atomic(patches, tmp_path)
    assert batch_res2.all_applied_cleanly is False
    assert batch_res2.has_failures is True

    # Dedicated single patch atomic function on valid patch
    valid_diff = "--- a/f1.txt\n+++ b/f1.txt\n@@ -1 +1 @@\n-foo\n+foo_ok\n"
    valid_patch = mpatch.parse_auto(valid_diff)[0]
    res = mpatch.apply_patch_to_file_atomic(valid_patch, tmp_path)
    assert res.report.all_applied_cleanly is True
    assert f1.read_text() == "foo_ok\n"


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


def test_python_suggest_close_file_paths(tmp_path: Path):
    target_dir = tmp_path / "src"
    target_dir.mkdir()
    (target_dir / "calculator.py").write_text("def add(): pass\n")
    (target_dir / "controller.py").write_text("def run(): pass\n")

    suggestions = mpatch.suggest_close_file_paths("calculate.py", target_dir, limit=3)
    assert len(suggestions) >= 1
    assert "calculator.py" in Path(suggestions[0]).as_posix()


def test_python_path_and_ellipsis_inspection():
    # is_ellipsis_line
    assert mpatch.is_ellipsis_line("...") is True
    assert mpatch.is_ellipsis_line("    // ... existing code ...") is True
    assert mpatch.is_ellipsis_line("# ... rest of function ...") is True
    assert mpatch.is_ellipsis_line("const copy = [...items];") is False

    # is_plausible_file_path
    assert mpatch.is_plausible_file_path("src/core/main.rs") is True
    assert mpatch.is_plausible_file_path("components/Button.tsx") is True
    assert mpatch.is_plausible_file_path(".gitignore") is True
    assert mpatch.is_plausible_file_path("Please check the following:") is False

    # extract_file_path_from_line
    extracted = mpatch.extract_file_path_from_line(
        "In file `src/routes/auth.py`, update handler:"
    )
    assert extracted is not None
    assert Path(extracted).as_posix() == "src/routes/auth.py"
    assert mpatch.extract_file_path_from_line("No path here.") is None


def test_python_ensure_path_is_safe(tmp_path: Path):
    safe_child = mpatch.ensure_path_is_safe(tmp_path, "sub/file.txt")
    assert Path(safe_child).is_absolute()

    with pytest.raises(mpatch.PathTraversalError):
        mpatch.ensure_path_is_safe(tmp_path, "../outside.txt")


def test_python_find_hunk_location():
    patch = mpatch.parse_auto(RAW_DIFF)[0]
    hunk = patch[0]
    start, length, match_type = mpatch.find_hunk_location(hunk, "old\n")
    assert start == 0
    assert length == 1
    assert match_type == "Exact"

    # Test via Hunk method
    assert hunk.find_location("old\n") == (0, 1, "Exact")


def test_python_merge_patches():
    diff1 = textwrap.dedent("""\
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -line 1
        +line one
    """)
    diff2 = textwrap.dedent("""\
        --- a/file.txt
        +++ b/file.txt
        @@ -10 +10 @@
        -line 10
        +line ten
    """)
    diff3 = textwrap.dedent("""\
        --- a/other.txt
        +++ b/other.txt
        @@ -1 +1 @@
        -foo
        +bar
    """)
    p1 = mpatch.parse_single_patch(diff1)
    p2 = mpatch.parse_single_patch(diff2)
    p3 = mpatch.parse_single_patch(diff3)

    merged = mpatch.merge_patches([p1, p3, p2])
    assert len(merged) == 2
    assert Path(merged[0].file_path).as_posix() == "file.txt"
    assert len(merged[0].hunks) == 2
    assert merged[0].hunks[0].added_lines == ["line one"]
    assert merged[0].hunks[1].added_lines == ["line ten"]
    assert Path(merged[1].file_path).as_posix() == "other.txt"
    assert len(merged[1].hunks) == 1

    p1_copy = mpatch.parse_single_patch(diff1)
    p2_copy = mpatch.parse_single_patch(diff2)
    p1_copy.merge(p2_copy)
    assert len(p1_copy.hunks) == 2
    assert p1_copy.hunks[1].added_lines == ["line ten"]


# --- README Examples Tests ---


def test_python_readme_problem_and_solution_table():
    original = textwrap.dedent("""\
        def main():
            # Updated comment
            print("Hello")
    """)
    diff = textwrap.dedent("""\
        --- a/main.py
        +++ b/main.py
        @@ -1,3 +1,3 @@
         def main():
        -    print("Hello")
        +    print("World")
    """)
    patched = mpatch.patch_content(diff, original=original)
    expected = textwrap.dedent("""\
        def main():
            # Updated comment
            print("World")
    """)
    assert patched == expected


def test_python_readme_quick_start():
    original_code = """\
def greet():
    print("Hello, old friend")
"""
    diff = """\
Here is the fix:
```diff
--- a/greet.py
+++ b/greet.py
@@ -1,2 +1,2 @@
 def greet():
-    print("Hello, old friend")
+    print("Hello, new world!")
```
"""
    new_code = mpatch.patch_content(diff, original=original_code)
    expected = """\
def greet():
    print("Hello, new world!")
"""
    assert new_code == expected


def test_python_readme_tour_parsing_and_inspecting():
    diff_string = textwrap.dedent("""\
        --- a/example.py
        +++ b/example.py
        @@ -1 +1 @@
        -old
        +new
    """)
    patches = mpatch.parse_auto(diff_string)
    assert len(patches) == 1
    for patch in patches:
        assert Path(patch.file_path).as_posix() == "example.py"
        assert len(patch) == 1
        assert not patch.is_creation

    patch = patches[0]
    for i, hunk in enumerate(patch):
        assert hunk.removed_lines == ["old"]
        assert hunk.added_lines == ["new"]
        assert hunk.required_match_span == 1

    single_diff_str = textwrap.dedent("""\
        --- a/single.py
        +++ b/single.py
        @@ -1 +1 @@
        -a
        +b
    """)
    single_patch = mpatch.parse_single_patch(single_diff_str)
    assert Path(single_patch.file_path).as_posix() == "single.py"


def test_python_readme_tour_filesystem_and_atomicity(tmp_path: Path):
    target_dir = tmp_path / "my_project"
    target_dir.mkdir()
    target_file = target_dir / "file.txt"
    target_file.write_text("hello\n")

    diff = textwrap.dedent("""\
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -hello
        +world
    """)

    # 1. apply_directory
    success = mpatch.apply_directory(diff, target_dir)
    assert success is True
    assert target_file.read_text() == "world\n"

    # 2. apply_directory with atomic=True
    diff_atomic = textwrap.dedent("""\
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -world
        +universe
    """)
    success = mpatch.apply_directory(diff_atomic, target_dir, atomic=True)
    assert success is True
    assert target_file.read_text() == "universe\n"

    # 3. apply_patches_to_dir
    diff_batch = textwrap.dedent("""\
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -universe
        +multiverse
    """)
    patches = mpatch.parse_auto(diff_batch)
    batch_result = mpatch.apply_patches_to_dir(patches, target_dir)
    assert batch_result.all_succeeded is True
    assert not batch_result.has_failures
    assert target_file.read_text() == "multiverse\n"


def test_python_readme_tour_reporting_and_dry_run(tmp_path: Path):
    target_dir = tmp_path / "my_project"
    target_dir.mkdir()
    target_file = target_dir / "calc.py"
    target_file.write_text("def calc(): return 1\n")

    diff = textwrap.dedent("""\
        --- a/calc.py
        +++ b/calc.py
        @@ -1 +1 @@
        -def calc(): return 1
        +def calc(): return 2
    """)
    patch = mpatch.parse_auto(diff)[0]

    # Dry run with fuzz factor
    result = patch.apply_to_file(target_dir, fuzz_factor=0.5, dry_run=True)
    assert result.report.all_applied_cleanly is True
    assert result.diff is not None
    assert "+def calc(): return 2" in result.diff
    assert target_file.read_text() == "def calc(): return 1\n"

    # Invert patch with ~
    reversed_patch = ~patch
    assert reversed_patch[0].removed_lines == ["def calc(): return 2"]
    assert reversed_patch[0].added_lines == ["def calc(): return 1"]


def test_python_readme_tour_deduplication_and_diff_creation():
    diff1 = textwrap.dedent("""\
        --- a/file.txt
        +++ b/file.txt
        @@ -1 +1 @@
        -line 1
        +line one
    """)
    diff2 = textwrap.dedent("""\
        --- a/file.txt
        +++ b/file.txt
        @@ -10 +10 @@
        -line 10
        +line ten
    """)
    patches = mpatch.parse_auto(diff1) + mpatch.parse_auto(diff2)
    consolidated = mpatch.merge_patches(patches)
    assert len(consolidated) == 1
    assert len(consolidated[0].hunks) == 2

    # Patch.merge
    p1 = mpatch.parse_single_patch(diff1)
    p2 = mpatch.parse_single_patch(diff2)
    p1.merge(p2)
    assert len(p1.hunks) == 2

    # create_unified_diff & Patch.from_texts
    old_text = "apple\nbanana\npineapple\n"
    new_text = "apple\norange\npineapple\n"
    diff_str = mpatch.create_unified_diff("fruits.txt", old_text, new_text)
    assert "--- a/fruits.txt" in diff_str
    assert "+++ b/fruits.txt" in diff_str
    assert "-banana" in diff_str
    assert "+orange" in diff_str

    patch_obj = mpatch.Patch.from_texts("fruits.txt", old_text, new_text)
    assert patch_obj[0].removed_lines == ["banana"]
    assert patch_obj[0].added_lines == ["orange"]


def test_python_readme_tour_aider_wildcards():
    original_code = textwrap.dedent("""\
        def process_data(data):
            validate(data)
            # Step 1: Normalize
            normalized = [x.strip() for x in data]
            # Step 2: Transform
            transformed = [x.upper() for x in normalized]
            # Step 3: Output
            return transformed
    """)

    diff = textwrap.dedent("""\
        process.py
        <<<<<<< SEARCH
        def process_data(data):
            ...
            # Step 2: Transform
            transformed = [x.upper() for x in normalized]
        =======
        def process_data(data):
            ...
            # Step 2: Transform
            transformed = [x.lower() for x in normalized]
        >>>>>>> REPLACE
    """)

    result = mpatch.patch_content(diff, original=original_code)
    assert "validate(data)" in result
    assert "# Step 1: Normalize" in result
    assert "normalized = [x.strip() for x in data]" in result
    assert "transformed = [x.lower() for x in normalized]" in result
    assert "# Step 3: Output" in result
    assert "return transformed" in result


def test_python_readme_tour_three_way_merge():
    base = "Apples\nBananas\nCherries\nDates\n"
    ours = "Apples\nBlueberries\nCherries\nDates\n"
    theirs = "Apples\nBananas\nCherries\nDragonfruit\n"

    merged, is_conflicted = mpatch.merge_three_way(base, ours, theirs)
    assert not is_conflicted
    assert merged == "Apples\nBlueberries\nCherries\nDragonfruit\n"

    conflict_merged, is_conflicted = mpatch.merge_three_way(
        "val = 1\n",
        "val = 2\n",
        "val = 3\n",
        labels=("base", "ours", "theirs"),
    )
    assert is_conflicted
    assert "<<<<<<< ours\nval = 2\n" in conflict_merged
    assert "||||||| base\nval = 1\n" in conflict_merged
    assert "=======\nval = 3\n" in conflict_merged
    assert ">>>>>>> theirs\n" in conflict_merged


def test_python_readme_tour_inline_diff_and_utilities(tmp_path: Path):
    expected = ["def calculate(val: int, factor: float = 1.0) -> float:"]
    actual = ["def calculate(val: int, factor: float = 2.5) -> float:"]

    diff_view = mpatch.format_inline_diff(expected, actual)
    assert "calculate" in diff_view
    assert "1.0" in diff_view
    assert "2.5" in diff_view

    fmt = mpatch.detect_patch("```diff\n--- a/f\n+++ b/f\n```")
    assert fmt == "Markdown"

    # suggest_close_file_paths & ensure_path_is_safe
    src_dir = tmp_path / "src"
    src_dir.mkdir()
    (src_dir / "calculate.rs").write_text("fn calculate() {}\n")

    suggestions = mpatch.suggest_close_file_paths("calculate.rs", src_dir, limit=3)
    assert len(suggestions) >= 1
    assert "calculate.rs" in str(suggestions[0])

    safe_path = mpatch.ensure_path_is_safe(src_dir, "utils/helpers.py")
    assert safe_path.is_absolute()

    # Wildcard and path inspection
    assert mpatch.is_ellipsis_line("// ... existing code ...") is True
    assert mpatch.is_ellipsis_line("const copy = [...items];") is False

    path = mpatch.extract_file_path_from_line("In `src/server.ts`, replace the handler:")
    assert Path(path).as_posix() == "src/server.ts"
    assert mpatch.is_plausible_file_path("src/server.ts") is True
