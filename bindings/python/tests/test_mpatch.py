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


# --- Format Detection Tests ---


def test_detect_patch_format():
    assert mpatch.detect_patch(MD_DIFF) == "Markdown"
    assert mpatch.detect_patch(RAW_DIFF) == "Unified"
    assert mpatch.detect_patch(CONFLICT_DIFF) == "Conflict"
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
