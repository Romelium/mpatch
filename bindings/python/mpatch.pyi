# ruff: noqa: E501
import os
import pathlib
from typing import Any, Iterator, overload

class MpatchError(Exception):
    """Base exception for all errors raised by mpatch."""

    ...

class ParseError(MpatchError):
    """Raised when parsing diff content, headers, or conflict markers fails."""

    ...

class ApplyError(MpatchError):
    """Raised when applying a patch fails or applies only partially in strict mode."""

    ...

class PathTraversalError(ApplyError):
    """Raised when a patch target path resolves outside the base directory."""

    ...

VERSION: str
"""The version of the mpatch package."""

class Hunk:
    """Represents a single hunk of changes within a patch."""
    def __init__(
        self,
        lines: list[str],
        *,
        old_start_line: int | None = None,
        new_start_line: int | None = None,
    ) -> None:
        """
        Creates a new Hunk.

        Args:
            lines (list[str]): The raw lines of the hunk, each prefixed with ' ', '+', or '-'.
            old_start_line (int | None, optional): The starting line number in the original file. Defaults to None.
            new_start_line (int | None, optional): The starting line number in the new file. Defaults to None.
        """
        ...
    @property
    def lines(self) -> list[str]:
        """The raw lines of the hunk, each prefixed with ' ', '+', or '-'."""
        ...
    @property
    def old_start_line(self) -> int | None:
        """The starting line number in the original file (1-based)."""
        ...
    @property
    def new_start_line(self) -> int | None:
        """The starting line number in the new file (1-based)."""
        ...
    @property
    def context_lines(self) -> list[str]:
        """Extracts the context lines from the hunk (lines starting with ' ')."""
        ...
    @property
    def added_lines(self) -> list[str]:
        """Extracts the added lines from the hunk (lines starting with '+')."""
        ...
    @property
    def removed_lines(self) -> list[str]:
        """Extracts the removed lines from the hunk (lines starting with '-')."""
        ...
    @property
    def match_block(self) -> list[str]:
        """Extracts the lines that need to be matched in the target file."""
        ...
    @property
    def replace_block(self) -> list[str]:
        """Extracts the lines that will replace the matched block in the target file."""
        ...
    @property
    def has_changes(self) -> bool:
        """Checks if the hunk contains any effective changes (additions or
        deletions).
        """
        ...
    @property
    def required_match_span(self) -> int:
        """Returns the minimum span (in lines of match_block) between the first and last edit site in this hunk."""
        ...
    def invert(self) -> Hunk:
        """Creates a new Hunk that reverses the changes in this one."""
        ...
    def find_location(
        self,
        target_content: str,
        *,
        fuzz_factor: float = 0.7,
    ) -> tuple[int, int, str]:
        """
        Finds the location to apply this hunk to a given text content.

        Args:
            target_content (str): The content to search within.
            fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.

        Returns:
            tuple[int, int, str]: (start_line_index, replaced_length, match_type)
        """
        ...
    def __len__(self) -> int: ...
    @overload
    def __getitem__(self, idx: int) -> str: ...
    @overload
    def __getitem__(self, idx: slice) -> list[str]: ...
    def __iter__(self) -> Iterator[str]: ...
    def __bool__(self) -> bool: ...
    def __invert__(self) -> Hunk: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class Patch:
    """Represents all the changes to be applied to a single file."""
    def __init__(
        self,
        file_path: str | os.PathLike[Any],
        hunks: list[Hunk],
        *,
        ends_with_newline: bool = True,
    ) -> None:
        """
        Creates a new Patch.

        Args:
            file_path (str | os.PathLike): The relative path of the file to be patched.
            hunks (list[Hunk]): A list of hunks to be applied to the file.
            ends_with_newline (bool, optional): Indicates whether the file should end with a newline. Defaults to True.
        """
        ...
    @classmethod
    def from_texts(
        cls,
        file_path: str | os.PathLike[Any],
        old_text: str,
        new_text: str,
        *,
        context_len: int = 3,
    ) -> Patch:
        """
        Creates a `Patch` object by comparing two texts.

        This function compares `old_text` and `new_text`, generates a unified diff,
        and parses it into a structured `Patch` object.

        Args:
            file_path (str | os.PathLike): The file path to associate with the patch.
            old_text (str): The original text content.
            new_text (str): The new, modified text content.
            context_len (int, optional): The number of context lines to include.
                Defaults to 3.

        Returns:
            Patch: The generated patch object.
        """
        ...
    @property
    def file_path(self) -> pathlib.Path:
        """The relative path of the file to be patched."""
        ...
    @file_path.setter
    def file_path(self, path: str | os.PathLike[Any]) -> None:
        """Sets the relative path of the file to be patched."""
        ...
    @property
    def hunks(self) -> list[Hunk]:
        """A list of hunks to be applied to the file."""
        ...
    @hunks.setter
    def hunks(self, hunks: list[Hunk]) -> None:
        """Sets the list of hunks to be applied to the file."""
        ...
    @property
    def ends_with_newline(self) -> bool:
        """Indicates whether the file should end with a newline."""
        ...
    @ends_with_newline.setter
    def ends_with_newline(self, ends_with_newline: bool) -> None:
        """Sets whether the file should end with a newline."""
        ...
    @property
    def is_creation(self) -> bool:
        """Checks if the patch represents a file creation."""
        ...
    @property
    def is_deletion(self) -> bool:
        """Checks if the patch represents a full file deletion."""
        ...
    def invert(self) -> Patch:
        """Creates a new Patch that reverses the changes in this one."""
        ...
    def apply_to_file(
        self,
        target_dir: str | os.PathLike[Any],
        *,
        fuzz_factor: float = 0.7,
        dry_run: bool = False,
        atomic: bool = False,
    ) -> PatchResult:
        """
        Applies the patch to a file on disk (OOP alias for `apply_patch_to_file`).

        Args:
            target_dir (str | os.PathLike): The base directory to apply the patch.
            fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
            dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.
            atomic (bool, optional): If True, only writes changes to disk if all hunks apply cleanly. Default is False.

        Returns:
            PatchResult: The result of the application.

        Raises:
            PathTraversalError: If the patch targets a file outside target_dir.
            ApplyError: If reading or writing the file fails due to an I/O or permission issue.
        """
        ...
    def apply_to_file_atomic(
        self,
        target_dir: str | os.PathLike[Any],
        *,
        fuzz_factor: float = 0.7,
        dry_run: bool = False,
    ) -> PatchResult:
        """
        Applies the patch to a file on disk atomically.

        Args:
            target_dir (str | os.PathLike): The base directory to apply the patch.
            fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
            dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.

        Returns:
            PatchResult: The result of the application.

        Raises:
            PathTraversalError: If the patch targets a file outside target_dir.
            ApplyError: If reading or writing the file fails due to an I/O or permission issue.
        """
        ...
    def apply_to_content(
        self,
        original: str | None = None,
        *,
        fuzz_factor: float = 0.7,
        dry_run: bool = False,
    ) -> InMemoryResult:
        """
        Applies the patch to a string in memory (OOP alias for `apply_patch_to_content`).

        Args:
            original (str | None, optional): The original content. None for file creation. Defaults to None.
            fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
            dry_run (bool, optional): If True, previews changes without writing. Default is False.

        Returns:
            InMemoryResult: The result of the application.
        """
        ...
    def __len__(self) -> int: ...
    @overload
    def __getitem__(self, idx: int) -> Hunk: ...
    @overload
    def __getitem__(self, idx: slice) -> list[Hunk]: ...
    def __iter__(self) -> Iterator[Hunk]: ...
    def __bool__(self) -> bool: ...
    def __invert__(self) -> Patch: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...

class HunkFailure:
    """Provides detailed reasons for hunk failures in applying patches."""
    @property
    def hunk_index(self) -> int:
        """The 1-based index of the hunk that failed."""
        ...
    @property
    def reason(self) -> str:
        """The reason for the failure."""
        ...
    @property
    def error_type(self) -> str:
        """The specific error type (e.g. 'ContextNotFound',
        'FuzzyMatchBelowThreshold').
        """
        ...
    @property
    def best_score(self) -> float | None:
        """The best score found, if the error was a fuzzy match failure."""
        ...
    @property
    def threshold(self) -> float | None:
        """The threshold that was not met, if the error was a fuzzy match failure."""
        ...
    @property
    def location_start(self) -> int | None:
        """The starting line index of the near-miss location, if the error was a fuzzy match failure."""
        ...
    @property
    def location_length(self) -> int | None:
        """The window length of the near-miss location, if the error was a fuzzy match failure."""
        ...
    @property
    def ambiguous_matches(self) -> list[int] | None:
        """The ambiguous line indices found, if the error was due to an ambiguous
        match.
        """
        ...
    def __repr__(self) -> str: ...

class HunkApplyStatus:
    """Status of a single hunk's application attempt."""
    @property
    def status(self) -> str:
        """The status: 'Applied', 'Skipped', or 'Failed'."""
        ...
    @property
    def location_start(self) -> int | None:
        """The 0-based starting line index in the target file where the hunk was applied, or None if skipped/failed."""
        ...
    @property
    def location_length(self) -> int | None:
        """The number of lines in the target file that were replaced."""
        ...
    @property
    def match_type(self) -> str | None:
        """The type of match used ('Exact', 'ExactIgnoringWhitespace', or 'Fuzzy')."""
        ...
    @property
    def replaced_lines(self) -> list[str] | None:
        """The original lines from the target file that were replaced."""
        ...
    @property
    def error_reason(self) -> str | None:
        """The error reason if the status is 'Failed'."""
        ...

class ApplyResult:
    """Status report detailing the applied success of hunks."""
    @property
    def all_applied_cleanly(self) -> bool:
        """True if all hunks in the patch were applied successfully or skipped."""
        ...
    @property
    def has_failures(self) -> bool:
        """True if any hunk in the patch failed to apply."""
        ...
    @property
    def failure_count(self) -> int:
        """The number of hunks that failed to apply."""
        ...
    @property
    def success_count(self) -> int:
        """The number of hunks that were applied successfully or skipped."""
        ...
    @property
    def failures(self) -> list[HunkFailure]:
        """A list of all hunks that failed to apply."""
        ...
    @property
    def hunk_results(self) -> list[HunkApplyStatus]:
        """The detailed status for every hunk in the patch."""
        ...
    def __len__(self) -> int: ...
    @overload
    def __getitem__(self, idx: int) -> HunkApplyStatus: ...
    @overload
    def __getitem__(self, idx: slice) -> list[HunkApplyStatus]: ...
    def __iter__(self) -> Iterator[HunkApplyStatus]: ...
    def __bool__(self) -> bool: ...

class InMemoryResult:
    """Represents the success result of an internal string apply logic."""
    @property
    def new_content(self) -> str:
        """The new content after applying the patch."""
        ...
    @property
    def report(self) -> ApplyResult:
        """Detailed results for each hunk within the patch operation."""
        ...
    def __bool__(self) -> bool: ...

class PatchResult:
    """Represents a single File I/O apply result."""
    @property
    def report(self) -> ApplyResult:
        """Detailed results for each hunk within the patch operation."""
        ...
    @property
    def diff(self) -> str | None:
        """The unified diff of the proposed changes (only populated when dry_run
        is true).
        """
        ...
    def __bool__(self) -> bool: ...

class BatchResult:
    """Result from batch-applying patches to a directory on disk."""
    @property
    def all_succeeded(self) -> bool:
        """True if all patches in the batch were applied without hard errors."""
        ...
    @property
    def all_applied_cleanly(self) -> bool:
        """True if all patches in the batch succeeded and all hunks applied cleanly."""
        ...
    @property
    def has_failures(self) -> bool:
        """True if any patch in the batch had a hard error or any hunk failed to apply."""
        ...
    @property
    def hard_failures(self) -> list[tuple[str, str]]:
        """A list of operations that resulted in a hard error."""
        ...
    @property
    def results(self) -> dict[str, PatchResult | str]:
        """A dict of results for each patch operation attempted."""
        ...
    def __len__(self) -> int: ...
    def __getitem__(self, key: str) -> PatchResult | str: ...
    def __contains__(self, key: str) -> bool: ...
    def __bool__(self) -> bool: ...

def detect_patch(diff: str) -> str:
    """
    Automatically detects the format of the input text.

    Args:
        diff (str): The patch content.

    Returns:
        str: 'Markdown', 'Unified', 'Aider', 'Conflict', or 'Unknown'.
    """
    ...

def parse_auto(diff: str) -> list[Patch]:
    """
    Automatically detects the format of the input text and parses it into a list
    of patches.

    Args:
        diff (str): The patch content (Markdown, Unified, Aider, or Conflict Markers).

    Returns:
        list[Patch]: A list of parsed patches.

    Raises:
        ParseError: If parsing diff headers or hunk syntax fails.
    """
    ...

def parse_single_patch(diff: str) -> Patch:
    """
    Parses a string containing a diff and returns a single Patch object.

    Args:
        diff (str): The patch content (Markdown, Unified, Aider, or Conflict Markers).

    Returns:
        Patch: The single parsed patch.

    Raises:
        ParseError: If parsing fails, or if zero or multiple patches are found.
    """
    ...

def parse_diffs(diff: str) -> list[Patch]:
    """
    Parses a string containing one or more markdown diff blocks into a list of patches.

    Args:
        diff (str): The markdown content.

    Returns:
        list[Patch]: A list of parsed patches.

    Raises:
        ParseError: If parsing diff headers or hunk syntax fails.
    """
    ...

def parse_patches(diff: str) -> list[Patch]:
    """
    Parses a string containing raw unified diff content into a list of patches.

    Args:
        diff (str): The raw unified diff content.

    Returns:
        list[Patch]: A list of parsed patches.

    Raises:
        ParseError: If diff headers are missing or hunk syntax is malformed.
    """
    ...

def parse_conflict_markers(diff: str) -> list[Patch]:
    """
    Parses a string containing "Conflict Marker" style diffs (<<<<, ====, >>>>).

    Args:
        diff (str): The conflict marker content.

    Returns:
        list[Patch]: A list of parsed patches.
    """
    ...

def parse_aider(diff: str) -> list[Patch]:
    """
    Parses a string containing "Aider" style search/replace blocks (<<<<<<< SEARCH, =======, >>>>>>> REPLACE).

    Args:
        diff (str): The Aider search/replace block content.

    Returns:
        list[Patch]: A list of parsed patches.
    """
    ...

def invert_patches(patches: list[Patch]) -> list[Patch]:
    """
    Inverts a list of patches (swaps additions and deletions).

    Args:
        patches (list[Patch]): The patches to invert.

    Returns:
        list[Patch]: The inverted patches.
    """
    ...

def create_unified_diff(
    file_path: str | os.PathLike[Any],
    old_text: str,
    new_text: str,
    *,
    context_len: int = 3,
) -> str:
    """
    Creates a unified diff string by comparing two texts.

    This function compares `old_text` and `new_text` and generates a standard
    unified diff representation. It is useful for generating patches programmatically
    that can be applied later or displayed to the user.

    Args:
        file_path (str | os.PathLike): The file path to associate with the patch
            headers.
        old_text (str): The original text content.
        new_text (str): The new, modified text content.
        context_len (int, optional): Number of context lines to include before
            and after changes. Defaults to 3.

    Returns:
        str: The generated unified diff string.

    Raises:
        ParseError: If diff generation or parsing fails.

    Example:
        >>> import mpatch
        >>> diff = mpatch.create_unified_diff(
        ...     "main.py", "print('old')\\n", "print('new')\\n"
        ... )
        >>> print(diff)
        --- a/main.py
        +++ b/main.py
        @@ -1,1 +1,1 @@
        -print('old')
        +print('new')
    """
    ...

def format_inline_diff(expected_lines: list[str], actual_lines: list[str]) -> str:
    """
    Formats an inline word-level diff between expected lines and actual lines.

    Args:
        expected_lines (list[str]): The expected lines.
        actual_lines (list[str]): The actual lines.

    Returns:
        str: Formatted diff string with sub-line word highlights.
    """
    ...

def merge_three_way(
    base: str,
    ours: str,
    theirs: str,
    *,
    labels: tuple[str, str, str] | None = None,
) -> tuple[str, bool]:
    """
    Performs a 3-way line merge among a common ancestor (base), current content (ours),
    and incoming changes (theirs).

    Args:
        base (str): The common ancestor content.
        ours (str): Current local content.
        theirs (str): Incoming changes.
        labels (tuple[str, str, str] | None, optional): Labels for conflict markers (base, ours, theirs).

    Returns:
        tuple[str, bool]: (merged_content, is_conflicted)
    """
    ...

def patch_content(
    diff: str,
    original: str | None = None,
    *,
    fuzz_factor: float = 0.7,
    dry_run: bool = False,
) -> str:
    """
    Applies a diff to a string in memory.

    Args:
        diff (str): The patch content (Markdown, Unified, Aider, or Conflict Markers).
        original (str | None, optional): The original content. None for file creation. Defaults to None.
        fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
        dry_run (bool, optional): If True, returns what would happen without making changes. Default is False.

    Returns:
        str: The new patched content.

    Raises:
        ParseError: If parsing the diff fails or multiple patches are found.
        ApplyError: If any hunk in the patch fails to apply cleanly.
    """
    ...

def apply_directory(
    diff: str,
    target_dir: str | os.PathLike[Any],
    *,
    fuzz_factor: float = 0.7,
    dry_run: bool = False,
    atomic: bool = False,
) -> bool:
    """
    Applies a diff containing multiple patches to a target directory.

    Args:
        diff (str): The patch content containing one or more file changes (Markdown, Unified, Aider, or Conflict Markers).
        target_dir (str | os.PathLike): The base directory to apply the patches.
        fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
        dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.
        atomic (bool, optional): If True, only writes to disk if all patches and hunks apply cleanly. Default is False.

    Returns:
        bool: True if all patches succeeded, False if any hard errors occurred.

    Raises:
        ParseError: If parsing diff headers or hunk syntax fails.
    """
    ...

def apply_patch_to_content(
    patch: Patch,
    original: str | None = None,
    *,
    fuzz_factor: float = 0.7,
    dry_run: bool = False,
) -> InMemoryResult:
    """
    Applies a Patch object to a string content in memory.

    Args:
        patch (Patch): The patch to apply.
        original (str | None, optional): The original content. None for file creation. Defaults to None.
        fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
        dry_run (bool, optional): If True, returns what would happen without making changes. Default is False.

    Returns:
        InMemoryResult: The result of the application.
    """
    ...

def apply_patch_to_file(
    patch: Patch,
    target_dir: str | os.PathLike[Any],
    *,
    fuzz_factor: float = 0.7,
    dry_run: bool = False,
    atomic: bool = False,
) -> PatchResult:
    """
    Applies a Patch object to a file on disk.

    Args:
        patch (Patch): The patch to apply.
        target_dir (str | os.PathLike): The base directory to apply the patch.
        fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
        dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.
        atomic (bool, optional): If True, only writes to disk if all hunks in the patch apply cleanly. Default is False.

    Returns:
        PatchResult: The result of the application.

    Raises:
        PathTraversalError: If the patch targets a file outside target_dir.
        ApplyError: If reading or writing the file fails due to an I/O or permission issue.
    """

def apply_patch_to_file_atomic(
    patch: Patch,
    target_dir: str | os.PathLike[Any],
    *,
    fuzz_factor: float = 0.7,
    dry_run: bool = False,
) -> PatchResult:
    """
    Applies a Patch object to a file on disk atomically.

    Args:
        patch (Patch): The patch to apply.
        target_dir (str | os.PathLike): The base directory to apply the patch.
        fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
        dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.

    Returns:
        PatchResult: The result of the application.

    Raises:
        PathTraversalError: If the target path resolves outside target_dir.
        ApplyError: If reading or writing the file fails due to an I/O error or permission issue.
    """
    ...

def apply_patches_to_dir(
    patches: list[Patch],
    target_dir: str | os.PathLike[Any],
    *,
    fuzz_factor: float = 0.7,
    dry_run: bool = False,
    atomic: bool = False,
) -> BatchResult:
    """
    Applies a list of patches to a directory on disk.

    Args:
        patches (list[Patch]): The patches to apply.
        target_dir (str | os.PathLike): The base directory to apply the patches.
        fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
        dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.
        atomic (bool, optional): If True, only writes to disk if all patches and hunks apply cleanly. Default is False.

    Returns:
        BatchResult: The aggregated results of the applications.
    """
    ...

def apply_patches_to_dir_atomic(
    patches: list[Patch],
    target_dir: str | os.PathLike[Any],
    *,
    fuzz_factor: float = 0.7,
    dry_run: bool = False,
) -> BatchResult:
    """
    Applies a list of patches to a directory on disk atomically.

    Args:
        patches (list[Patch]): The patches to apply.
        target_dir (str | os.PathLike): The base directory to apply the patches.
        fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
        dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.

    Returns:
        BatchResult: The aggregated results of the applications.
    """
    ...

def suggest_close_file_paths(
    missing_path: str | os.PathLike[Any],
    target_dir: str | os.PathLike[Any],
    *,
    limit: int = 3,
) -> list[pathlib.Path]:
    """
    Finds close matching file paths in a target directory when a patch specifies a missing file.

    Args:
        missing_path (str | os.PathLike): The path of the missing target file.
        target_dir (str | os.PathLike): The base directory to search within.
        limit (int, optional): Maximum number of suggestions to return. Defaults to 3.

    Returns:
        list[pathlib.Path]: List of candidate file paths relative to target_dir.
    """
    ...

def is_ellipsis_line(line: str) -> bool:
    """
    Determines whether a line represents an ellipsis / wildcard indicating omitted code.

    Args:
        line (str): The line to evaluate.

    Returns:
        bool: True if the line represents an ellipsis or omitted code wildcard, False otherwise.
    """
    ...

def is_plausible_file_path(path: str) -> bool:
    """
    Determines whether a string looks like a plausible file path rather than conversational prose.

    Args:
        path (str): The candidate string to check.

    Returns:
        bool: True if plausible file path, False otherwise.
    """
    ...

def extract_file_path_from_line(line: str) -> pathlib.Path | None:
    """
    Extracts a plausible file path from a conversational heading or line preceding a patch block.

    Args:
        line (str): The line to extract from.

    Returns:
        pathlib.Path | None: The extracted path if found, or None.
    """
    ...

def ensure_path_is_safe(
    target_dir: str | os.PathLike[Any],
    relative_path: str | os.PathLike[Any],
) -> pathlib.Path:
    """
    Validates that a relative path does not escape the target base directory.

    Args:
        target_dir (str | os.PathLike): The base directory.
        relative_path (str | os.PathLike): The relative path to check.

    Returns:
        pathlib.Path: The safe canonical path.

    Raises:
        PathTraversalError: If the relative path escapes target_dir.
        ApplyError: If an I/O error occurs during canonicalization.
    """
    ...

def find_hunk_location(
    hunk: Hunk,
    target_content: str,
    *,
    fuzz_factor: float = 0.7,
) -> tuple[int, int, str]:
    """
    Finds the location to apply a hunk to a given text content without modifying it.

    Args:
        hunk (Hunk): The hunk to locate.
        target_content (str): The content to search within.
        fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.

    Returns:
        tuple[int, int, str]: (start_line_index, replaced_length, match_type)

    Raises:
        ApplyError: If no suitable location could be found.
    """
    ...
