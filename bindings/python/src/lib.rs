use ::mpatch::ApplyOptions;
use pyo3::prelude::*;
use pyo3::IntoPyObjectExt;
use std::path::PathBuf;

// Define custom Python exceptions for fine-grained error handling
pyo3::create_exception!(mpatch, MpatchError, pyo3::exceptions::PyException);
pyo3::create_exception!(mpatch, ParseError, MpatchError);
pyo3::create_exception!(mpatch, ApplyError, MpatchError);
pyo3::create_exception!(mpatch, PathTraversalError, ApplyError);

fn map_parse_err(err: ::mpatch::ParseError) -> PyErr {
    ParseError::new_err(err.to_string())
}

fn map_oneshot_err(err: ::mpatch::OneShotError) -> PyErr {
    match err {
        ::mpatch::OneShotError::Parse(e) => ParseError::new_err(e.to_string()),
        ::mpatch::OneShotError::NoPatchesFound
        | ::mpatch::OneShotError::MultiplePatchesFound(_) => ParseError::new_err(err.to_string()),
        ::mpatch::OneShotError::Apply(e) => ApplyError::new_err(e.to_string()),
        _ => MpatchError::new_err(err.to_string()),
    }
}

// --- Class Wrappers ---

/// Represents a single hunk of changes within a patch.
#[pyclass(module = "mpatch", name = "Hunk", eq, from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyHunk {
    inner: ::mpatch::Hunk,
}

#[pymethods]
impl PyHunk {
    /// Creates a new Hunk.
    ///
    /// Args:
    ///     lines (list[str]): The raw lines of the hunk, each prefixed with ' ', '+', or '-'.
    ///     old_start_line (int | None, optional): The starting line number in the original file. Defaults to None.
    ///     new_start_line (int | None, optional): The starting line number in the new file. Defaults to None.
    #[new]
    #[pyo3(signature = (lines, *, old_start_line=None, new_start_line=None))]
    fn py_new(
        lines: Vec<String>,
        old_start_line: Option<usize>,
        new_start_line: Option<usize>,
    ) -> Self {
        Self {
            inner: ::mpatch::Hunk {
                lines,
                old_start_line,
                new_start_line,
            },
        }
    }

    #[getter]
    /// The raw lines of the hunk, each prefixed with ' ', '+', or '-'.
    fn lines(&self) -> Vec<String> {
        self.inner.lines.clone()
    }

    #[getter]
    /// The starting line number in the original file (1-based).
    fn old_start_line(&self) -> Option<usize> {
        self.inner.old_start_line
    }

    #[getter]
    /// The starting line number in the new file (1-based).
    fn new_start_line(&self) -> Option<usize> {
        self.inner.new_start_line
    }

    #[getter]
    /// Extracts the context lines from the hunk (lines starting with ' ').
    fn context_lines(&self) -> Vec<String> {
        self.inner
            .context_lines()
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[getter]
    /// Extracts the added lines from the hunk (lines starting with '+').
    fn added_lines(&self) -> Vec<String> {
        self.inner
            .added_lines()
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[getter]
    /// Extracts the removed lines from the hunk (lines starting with '-').
    fn removed_lines(&self) -> Vec<String> {
        self.inner
            .removed_lines()
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[getter]
    /// Extracts the lines that need to be matched in the target file.
    fn match_block(&self) -> Vec<String> {
        self.inner
            .get_match_block()
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[getter]
    /// Extracts the lines that will replace the matched block in the target file.
    fn replace_block(&self) -> Vec<String> {
        self.inner
            .get_replace_block()
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[getter]
    /// Checks if the hunk contains any effective changes (additions or deletions).
    fn has_changes(&self) -> bool {
        self.inner.has_changes()
    }

    /// Creates a new Hunk that reverses the changes in this one.
    fn invert(&self) -> Self {
        Self {
            inner: self.inner.invert(),
        }
    }

    // --- Pythonic Dunder Methods ---

    fn __len__(&self) -> usize {
        self.inner.lines.len()
    }

    fn __bool__(&self) -> bool {
        self.inner.has_changes()
    }

    fn __invert__(&self) -> Self {
        self.invert()
    }

    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        idx: pyo3::Bound<'py, pyo3::PyAny>,
    ) -> PyResult<pyo3::Bound<'py, pyo3::PyAny>> {
        let lines = &self.inner.lines;
        let len = lines.len() as isize;

        if let Ok(slice) = idx.extract::<pyo3::Bound<'py, pyo3::types::PySlice>>() {
            let indices = slice.indices(len)?;
            let mut result = Vec::with_capacity(indices.slicelength);
            let mut current = indices.start;
            for _ in 0..indices.slicelength {
                result.push(lines[current as usize].clone());
                current += indices.step;
            }
            result.into_bound_py_any(py)
        } else if let Ok(idx) = idx.extract::<isize>() {
            let index = if idx < 0 { len + idx } else { idx };
            if index < 0 || index >= len {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "Line index out of range",
                ));
            }
            lines[index as usize].clone().into_bound_py_any(py)
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "Line indices must be integers or slices",
            ))
        }
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        let old = self
            .inner
            .old_start_line
            .map(|v| v.to_string())
            .unwrap_or_else(|| "None".to_string());
        let new = self
            .inner
            .new_start_line
            .map(|v| v.to_string())
            .unwrap_or_else(|| "None".to_string());
        format!(
            "<Hunk old_start={} new_start={} lines={}>",
            old,
            new,
            self.inner.lines.len()
        )
    }
}

/// Represents all the changes to be applied to a single file.
#[pyclass(module = "mpatch", name = "Patch", eq, from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyPatch {
    inner: ::mpatch::Patch,
}

#[pymethods]
impl PyPatch {
    /// Creates a new Patch.
    ///
    /// Args:
    ///     file_path (str | os.PathLike): The relative path of the file to be patched.
    ///     hunks (list[Hunk]): A list of hunks to be applied to the file.
    ///     ends_with_newline (bool, optional): Indicates whether the file should end with a newline. Defaults to True.
    #[new]
    #[pyo3(signature = (file_path, hunks, *, ends_with_newline=true))]
    fn py_new(file_path: PathBuf, hunks: Vec<PyHunk>, ends_with_newline: bool) -> Self {
        Self {
            inner: ::mpatch::Patch {
                file_path,
                hunks: hunks.into_iter().map(|h| h.inner).collect(),
                ends_with_newline,
            },
        }
    }

    #[classmethod]
    #[pyo3(signature = (file_path, old_text, new_text, *, context_len=3))]
    /// Creates a `Patch` object by comparing two texts.
    ///
    /// This function compares `old_text` and `new_text`, generates a unified diff,
    /// and parses it into a structured `Patch` object.
    ///
    /// Args:
    ///     file_path (str | os.PathLike): The file path to associate with the patch.
    ///     old_text (str): The original text content.
    ///     new_text (str): The new, modified text content.
    ///     context_len (int, optional): The number of context lines to include. Defaults to 3.
    ///
    /// Returns:
    ///     Patch: The generated patch object.
    fn from_texts(
        _cls: &pyo3::Bound<'_, pyo3::types::PyType>,
        py: Python<'_>,
        file_path: PathBuf,
        old_text: &str,
        new_text: &str,
        context_len: usize,
    ) -> PyResult<Self> {
        let old_str = old_text.to_string();
        let new_str = new_text.to_string();

        py.detach(move || {
            let patch = ::mpatch::Patch::from_texts(file_path, &old_str, &new_str, context_len)
                .map_err(map_parse_err)?;
            Ok(Self { inner: patch })
        })
    }

    #[getter]
    /// The relative path of the file to be patched.
    fn file_path(&self) -> PathBuf {
        self.inner.file_path.clone()
    }

    #[setter]
    fn set_file_path(&mut self, path: PathBuf) {
        self.inner.file_path = path;
    }

    #[getter]
    /// A list of hunks to be applied to the file.
    fn hunks(&self) -> Vec<PyHunk> {
        self.inner
            .hunks
            .iter()
            .map(|h| PyHunk { inner: h.clone() })
            .collect()
    }

    #[getter]
    /// Indicates whether the file should end with a newline.
    fn ends_with_newline(&self) -> bool {
        self.inner.ends_with_newline
    }

    #[getter]
    /// Checks if the patch represents a file creation.
    fn is_creation(&self) -> bool {
        self.inner.is_creation()
    }

    #[getter]
    /// Checks if the patch represents a full file deletion.
    fn is_deletion(&self) -> bool {
        self.inner.is_deletion()
    }

    /// Creates a new Patch that reverses the changes in this one.
    fn invert(&self) -> Self {
        Self {
            inner: self.inner.invert(),
        }
    }

    /// Applies the patch to a file on disk.
    ///
    /// Args:
    ///     target_dir (str | os.PathLike): The base directory to apply the patch.
    ///     fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
    ///     dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.
    ///
    /// Returns:
    ///     PatchResult: The result of the application.
    #[pyo3(signature = (target_dir, *, fuzz_factor=0.7, dry_run=false))]
    fn apply_to_file(
        &self,
        py: Python<'_>,
        target_dir: PathBuf,
        fuzz_factor: f32,
        dry_run: bool,
    ) -> PyResult<PyPatchResult> {
        apply_patch_to_file(py, self, target_dir, fuzz_factor, dry_run)
    }

    /// Applies the patch to a string in memory.
    ///
    /// Args:
    ///     original (str | None, optional): The original content. None for file creation. Defaults to None.
    ///     fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
    ///     dry_run (bool, optional): If True, previews changes without writing. Default is False.
    ///
    /// Returns:
    ///     InMemoryResult: The result of the application.
    #[pyo3(signature = (original=None, *, fuzz_factor=0.7, dry_run=false))]
    fn apply_to_content(
        &self,
        py: Python<'_>,
        original: Option<&str>,
        fuzz_factor: f32,
        dry_run: bool,
    ) -> PyInMemoryResult {
        apply_patch_to_content(py, self, original, fuzz_factor, dry_run)
    }

    // --- Pythonic Dunder Methods ---

    fn __len__(&self) -> usize {
        self.inner.hunks.len()
    }

    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        idx: pyo3::Bound<'py, pyo3::PyAny>,
    ) -> PyResult<pyo3::Bound<'py, pyo3::PyAny>> {
        let hunks = &self.inner.hunks;
        let len = hunks.len() as isize;

        if let Ok(slice) = idx.extract::<pyo3::Bound<'py, pyo3::types::PySlice>>() {
            let indices = slice.indices(len)?;
            let mut result = Vec::with_capacity(indices.slicelength);
            let mut current = indices.start;
            for _ in 0..indices.slicelength {
                result.push(PyHunk {
                    inner: hunks[current as usize].clone(),
                });
                current += indices.step;
            }
            result.into_bound_py_any(py)
        } else if let Ok(idx) = idx.extract::<isize>() {
            let index = if idx < 0 { len + idx } else { idx };
            if index < 0 || index >= len {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "Hunk index out of range",
                ));
            }
            PyHunk {
                inner: hunks[index as usize].clone(),
            }
            .into_bound_py_any(py)
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "Hunk indices must be integers or slices",
            ))
        }
    }

    fn __bool__(&self) -> bool {
        !self.inner.hunks.is_empty()
    }

    fn __invert__(&self) -> Self {
        self.invert()
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        format!(
            "<Patch file_path=\"{}\" hunks={}>",
            self.inner.file_path.display(),
            self.inner.hunks.len()
        )
    }
}

/// Provides detailed reasons for hunk failures in applying patches.
#[pyclass(module = "mpatch", name = "HunkFailure", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyHunkFailure {
    inner: ::mpatch::HunkFailure,
}

#[pymethods]
impl PyHunkFailure {
    #[getter]
    /// The 1-based index of the hunk that failed.
    fn hunk_index(&self) -> usize {
        self.inner.hunk_index
    }

    #[getter]
    /// The reason for the failure.
    fn reason(&self) -> String {
        self.inner.reason.to_string()
    }

    #[getter]
    /// The specific error type as a string.
    fn error_type(&self) -> String {
        match &self.inner.reason {
            ::mpatch::HunkApplyError::ContextNotFound => "ContextNotFound".to_string(),
            ::mpatch::HunkApplyError::AmbiguousExactMatch(_) => "AmbiguousExactMatch".to_string(),
            ::mpatch::HunkApplyError::AmbiguousFuzzyMatch(_) => "AmbiguousFuzzyMatch".to_string(),
            ::mpatch::HunkApplyError::FuzzyMatchBelowThreshold { .. } => {
                "FuzzyMatchBelowThreshold".to_string()
            }
        }
    }

    #[getter]
    /// The best score found, if the error was a fuzzy match failure.
    fn best_score(&self) -> Option<f64> {
        if let ::mpatch::HunkApplyError::FuzzyMatchBelowThreshold { best_score, .. } =
            &self.inner.reason
        {
            Some(*best_score)
        } else {
            None
        }
    }

    #[getter]
    /// The threshold that was not met, if the error was a fuzzy match failure.
    fn threshold(&self) -> Option<f64> {
        if let ::mpatch::HunkApplyError::FuzzyMatchBelowThreshold { threshold, .. } =
            &self.inner.reason
        {
            // Round to 4 decimal places to hide f32 -> f64 conversion artifacts in Python
            Some((*threshold as f64 * 10000.0).round() / 10000.0)
        } else {
            None
        }
    }

    #[getter]
    /// The ambiguous line indices found, if the error was due to an ambiguous match.
    fn ambiguous_matches(&self) -> Option<Vec<usize>> {
        match &self.inner.reason {
            ::mpatch::HunkApplyError::AmbiguousExactMatch(lines) => Some(lines.clone()),
            ::mpatch::HunkApplyError::AmbiguousFuzzyMatch(locs) => {
                Some(locs.iter().map(|(start, _)| *start).collect())
            }
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "<HunkFailure hunk_index={} reason=\"{}\">",
            self.inner.hunk_index, self.inner.reason
        )
    }
}

/// Status of a single hunk's application attempt.
#[pyclass(module = "mpatch", name = "HunkApplyStatus", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyHunkApplyStatus {
    status: String,
    location_start: Option<usize>,
    location_length: Option<usize>,
    match_type: Option<String>,
    replaced_lines: Option<Vec<String>>,
    error_reason: Option<String>,
}

#[pymethods]
impl PyHunkApplyStatus {
    #[getter]
    /// The status: 'Applied', 'Skipped', or 'Failed'.
    fn status(&self) -> String {
        self.status.clone()
    }

    #[getter]
    /// The starting line index in the target file where the hunk was applied.
    fn location_start(&self) -> Option<usize> {
        self.location_start
    }

    #[getter]
    /// The number of lines in the target file that were replaced.
    fn location_length(&self) -> Option<usize> {
        self.location_length
    }

    #[getter]
    /// The type of match used ('Exact', 'ExactIgnoringWhitespace', or 'Fuzzy').
    fn match_type(&self) -> Option<String> {
        self.match_type.clone()
    }

    #[getter]
    /// The original lines from the target file that were replaced.
    fn replaced_lines(&self) -> Option<Vec<String>> {
        self.replaced_lines.clone()
    }

    #[getter]
    /// The error reason if the status is 'Failed'.
    fn error_reason(&self) -> Option<String> {
        self.error_reason.clone()
    }

    fn __repr__(&self) -> String {
        format!("<HunkApplyStatus status=\"{}\">", self.status)
    }
}

/// Status report detailing the applied success of hunks.
#[pyclass(module = "mpatch", name = "ApplyResult", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyApplyResult {
    inner: ::mpatch::ApplyResult,
}

#[pymethods]
impl PyApplyResult {
    #[getter]
    /// True if all hunks in the patch were applied successfully or skipped.
    fn all_applied_cleanly(&self) -> bool {
        self.inner.all_applied_cleanly()
    }

    #[getter]
    /// True if any hunk in the patch failed to apply.
    fn has_failures(&self) -> bool {
        self.inner.has_failures()
    }

    #[getter]
    /// The number of hunks that failed to apply.
    fn failure_count(&self) -> usize {
        self.inner.failure_count()
    }

    #[getter]
    /// The number of hunks that were applied successfully or skipped.
    fn success_count(&self) -> usize {
        self.inner.success_count()
    }

    #[getter]
    /// A list of all hunks that failed to apply.
    fn failures(&self) -> Vec<PyHunkFailure> {
        self.inner
            .failures()
            .into_iter()
            .map(|f| PyHunkFailure { inner: f })
            .collect()
    }

    #[getter]
    /// The detailed status for every hunk in the patch.
    fn hunk_results(&self) -> Vec<PyHunkApplyStatus> {
        self.inner
            .hunk_results
            .iter()
            .map(|s| match s {
                ::mpatch::HunkApplyStatus::Applied {
                    location,
                    match_type,
                    replaced_lines,
                } => {
                    let match_str = match match_type {
                        ::mpatch::MatchType::Exact => "Exact",
                        ::mpatch::MatchType::ExactIgnoringWhitespace => "ExactIgnoringWhitespace",
                        ::mpatch::MatchType::Fuzzy { .. } => "Fuzzy",
                    };
                    PyHunkApplyStatus {
                        status: "Applied".to_string(),
                        location_start: Some(location.start_index),
                        location_length: Some(location.length),
                        match_type: Some(match_str.to_string()),
                        replaced_lines: Some(replaced_lines.clone()),
                        error_reason: None,
                    }
                }
                ::mpatch::HunkApplyStatus::SkippedNoChanges => PyHunkApplyStatus {
                    status: "Skipped".to_string(),
                    location_start: None,
                    location_length: None,
                    match_type: None,
                    replaced_lines: None,
                    error_reason: None,
                },
                ::mpatch::HunkApplyStatus::Failed(err) => PyHunkApplyStatus {
                    status: "Failed".to_string(),
                    location_start: None,
                    location_length: None,
                    match_type: None,
                    replaced_lines: None,
                    error_reason: Some(err.to_string()),
                },
            })
            .collect()
    }

    fn __bool__(&self) -> bool {
        self.inner.all_applied_cleanly()
    }

    fn __len__(&self) -> usize {
        self.inner.hunk_results.len()
    }

    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        idx: pyo3::Bound<'py, pyo3::PyAny>,
    ) -> PyResult<pyo3::Bound<'py, pyo3::PyAny>> {
        let results = self.hunk_results();
        let len = results.len() as isize;

        if let Ok(slice) = idx.extract::<pyo3::Bound<'py, pyo3::types::PySlice>>() {
            let indices = slice.indices(len)?;
            let mut result = Vec::with_capacity(indices.slicelength);
            let mut current = indices.start;
            for _ in 0..indices.slicelength {
                result.push(results[current as usize].clone());
                current += indices.step;
            }
            result.into_bound_py_any(py)
        } else if let Ok(idx) = idx.extract::<isize>() {
            let index = if idx < 0 { len + idx } else { idx };
            if index < 0 || index >= len {
                return Err(pyo3::exceptions::PyIndexError::new_err(
                    "Hunk result index out of range",
                ));
            }
            results[index as usize].clone().into_bound_py_any(py)
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "Indices must be integers or slices",
            ))
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "<ApplyResult successes={} failures={}>",
            self.success_count(),
            self.failure_count()
        )
    }
}

/// Represents the success result of an internal string apply logic.
#[pyclass(module = "mpatch", name = "InMemoryResult", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyInMemoryResult {
    inner: ::mpatch::InMemoryResult,
}

#[pymethods]
impl PyInMemoryResult {
    #[getter]
    /// The new content after applying the patch.
    fn new_content(&self) -> String {
        self.inner.new_content.clone()
    }

    #[getter]
    /// Detailed results for each hunk within the patch operation.
    fn report(&self) -> PyApplyResult {
        PyApplyResult {
            inner: self.inner.report.clone(),
        }
    }

    fn __bool__(&self) -> bool {
        self.inner.report.all_applied_cleanly()
    }

    fn __repr__(&self) -> String {
        format!(
            "<InMemoryResult all_applied_cleanly={}>",
            self.inner.report.all_applied_cleanly()
        )
    }
}

/// Represents a single File I/O apply result.
#[pyclass(module = "mpatch", name = "PatchResult", eq, skip_from_py_object)]
#[derive(Clone, PartialEq)]
pub struct PyPatchResult {
    inner: ::mpatch::PatchResult,
}

#[pymethods]
impl PyPatchResult {
    #[getter]
    /// Detailed results for each hunk within the patch operation.
    fn report(&self) -> PyApplyResult {
        PyApplyResult {
            inner: self.inner.report.clone(),
        }
    }

    #[getter]
    /// The unified diff of the proposed changes (only populated when dry_run is true).
    fn diff(&self) -> Option<String> {
        self.inner.diff.clone()
    }

    fn __bool__(&self) -> bool {
        self.inner.report.all_applied_cleanly()
    }

    fn __repr__(&self) -> String {
        format!(
            "<PatchResult all_applied_cleanly={} dry_run={}>",
            self.inner.report.all_applied_cleanly(),
            self.inner.diff.is_some()
        )
    }
}

/// Result from batch-applying patches to a directory on disk.
#[pyclass(module = "mpatch", name = "BatchResult")]
pub struct PyBatchResult {
    inner: ::mpatch::BatchResult,
}

#[pymethods]
impl PyBatchResult {
    #[getter]
    /// True if all patches in the batch were applied without hard errors.
    fn all_succeeded(&self) -> bool {
        self.inner.all_succeeded()
    }

    #[getter]
    /// A list of operations that resulted in a hard error.
    fn hard_failures(&self) -> Vec<(String, String)> {
        self.inner
            .hard_failures()
            .into_iter()
            .map(|(path, err)| (path.to_string_lossy().into_owned(), err.to_string()))
            .collect()
    }

    #[getter]
    /// A dict of results for each patch operation attempted.
    fn results<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let dict = pyo3::types::PyDict::new(py);
        for (path, res) in &self.inner.results {
            let path_str = path.to_string_lossy().into_owned();
            match res {
                Ok(patch_res) => {
                    let py_res = pyo3::Bound::new(
                        py,
                        PyPatchResult {
                            inner: patch_res.clone(),
                        },
                    )?;
                    dict.set_item(path_str, py_res)?;
                }
                Err(err) => {
                    dict.set_item(path_str, err.to_string())?;
                }
            }
        }
        Ok(dict)
    }

    fn __bool__(&self) -> bool {
        self.inner.all_succeeded()
    }

    fn __len__(&self) -> usize {
        self.inner.results.len()
    }

    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        key: &str,
    ) -> PyResult<pyo3::Bound<'py, pyo3::PyAny>> {
        let target = PathBuf::from(key);
        for (path, res) in &self.inner.results {
            if path == &target {
                match res {
                    Ok(patch_res) => {
                        return PyPatchResult {
                            inner: patch_res.clone(),
                        }
                        .into_bound_py_any(py);
                    }
                    Err(err) => {
                        return err.to_string().into_bound_py_any(py);
                    }
                }
            }
        }
        Err(pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }

    fn __contains__(&self, key: &str) -> bool {
        let target = PathBuf::from(key);
        self.inner.results.iter().any(|(path, _)| path == &target)
    }

    fn __repr__(&self) -> String {
        format!(
            "<BatchResult all_succeeded={} hard_failures={}>",
            self.inner.all_succeeded(),
            self.inner.hard_failures().len()
        )
    }
}

// --- Top-Level Functions ---

#[pyfunction]
#[pyo3(signature = (diff))]
/// Automatically detects the format of the input text.
///
/// Args:
///     diff (str): The patch content.
///
/// Returns:
///     str: 'Markdown', 'Unified', 'Conflict', or 'Unknown'.
fn detect_patch(diff: &str) -> String {
    match ::mpatch::detect_patch(diff) {
        ::mpatch::PatchFormat::Markdown => "Markdown".to_string(),
        ::mpatch::PatchFormat::Unified => "Unified".to_string(),
        ::mpatch::PatchFormat::Conflict => "Conflict".to_string(),
        ::mpatch::PatchFormat::Unknown => "Unknown".to_string(),
        _ => "Unknown".to_string(),
    }
}

#[pyfunction]
#[pyo3(signature = (diff))]
/// Automatically detects the format of the input text and parses it into a list of patches.
///
/// Args:
///     diff (str): The patch content (Markdown, Unified, or Conflict Markers).
///
/// Returns:
///     list[Patch]: A list of parsed patches.
fn parse_auto(py: Python<'_>, diff: &str) -> PyResult<Vec<PyPatch>> {
    let diff_str = diff.to_string();
    py.detach(move || {
        ::mpatch::parse_auto(&diff_str)
            .map_err(map_parse_err)
            .map(|patches| patches.into_iter().map(|p| PyPatch { inner: p }).collect())
    })
}

#[pyfunction]
#[pyo3(signature = (diff))]
/// Parses a string containing one or more markdown diff blocks into a list of patches.
///
/// Args:
///     diff (str): The markdown content.
///
/// Returns:
///     list[Patch]: A list of parsed patches.
fn parse_diffs(py: Python<'_>, diff: &str) -> PyResult<Vec<PyPatch>> {
    let diff_str = diff.to_string();
    py.detach(move || {
        ::mpatch::parse_diffs(&diff_str)
            .map_err(map_parse_err)
            .map(|patches| patches.into_iter().map(|p| PyPatch { inner: p }).collect())
    })
}

#[pyfunction]
#[pyo3(signature = (diff))]
/// Parses a string containing raw unified diff content into a list of patches.
///
/// Args:
///     diff (str): The raw unified diff content.
///
/// Returns:
///     list[Patch]: A list of parsed patches.
fn parse_patches(py: Python<'_>, diff: &str) -> PyResult<Vec<PyPatch>> {
    let diff_str = diff.to_string();
    py.detach(move || {
        ::mpatch::parse_patches(&diff_str)
            .map_err(map_parse_err)
            .map(|patches| patches.into_iter().map(|p| PyPatch { inner: p }).collect())
    })
}

#[pyfunction]
#[pyo3(signature = (diff))]
/// Parses a string containing "Conflict Marker" style diffs (<<<<, ====, >>>>).
///
/// Args:
///     diff (str): The conflict marker content.
///
/// Returns:
///     list[Patch]: A list of parsed patches.
fn parse_conflict_markers(py: Python<'_>, diff: &str) -> Vec<PyPatch> {
    let diff_str = diff.to_string();
    py.detach(move || {
        ::mpatch::parse_conflict_markers(&diff_str)
            .into_iter()
            .map(|p| PyPatch { inner: p })
            .collect()
    })
}

#[pyfunction]
#[pyo3(signature = (patches))]
/// Inverts a list of patches (swaps additions and deletions).
///
/// Args:
///     patches (list[Patch]): The patches to invert.
///
/// Returns:
///     list[Patch]: The inverted patches.
fn invert_patches(patches: Vec<PyPatch>) -> Vec<PyPatch> {
    patches.into_iter().map(|p| p.invert()).collect()
}

#[pyfunction]
#[pyo3(signature = (file_path, old_text, new_text, *, context_len=3))]
/// Creates a unified diff string by comparing two texts.
///
/// This function compares `old_text` and `new_text` and generates a standard
/// unified diff representation. It is useful for generating patches programmatically
/// that can be applied later or displayed to the user.
///
/// Args:
///     file_path (str | os.PathLike): The file path to associate with the patch headers.
///     old_text (str): The original text content.
///     new_text (str): The new, modified text content.
///     context_len (int, optional): Number of context lines to include before and after changes. Defaults to 3.
///
/// Returns:
///     str: The generated unified diff string.
///
/// Example:
///     >>> import mpatch
///     >>> diff = mpatch.create_unified_diff("main.py", "print('old')\n", "print('new')\n")
///     >>> print(diff)
///     --- a/main.py
///     +++ b/main.py
///     @@ -1,1 +1,1 @@
///     -print('old')
///     +print('new')
fn create_unified_diff(
    py: Python<'_>,
    file_path: PathBuf,
    old_text: &str,
    new_text: &str,
    context_len: usize,
) -> PyResult<String> {
    let old_str = old_text.to_string();
    let new_str = new_text.to_string();
    py.detach(move || {
        let patch = ::mpatch::Patch::from_texts(file_path, &old_str, &new_str, context_len)
            .map_err(map_parse_err)?;
        Ok(patch.to_string())
    })
}

#[pyfunction]
#[pyo3(signature = (diff, original=None, *, fuzz_factor=0.7, dry_run=false))]
/// Applies a diff to a string in memory.
///
/// Args:
///     diff (str): The patch content (Markdown, Unified, or Conflict Markers).
///     original (str | None, optional): The original content. None for file creation. Defaults to None.
///     fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
///     dry_run (bool, optional): If True, returns what would happen without making changes. Default is False.
///
/// Returns:
///     str: The new patched content.
fn patch_content(
    py: Python<'_>,
    diff: &str,
    original: Option<&str>,
    fuzz_factor: f32,
    dry_run: bool,
) -> PyResult<String> {
    let options = ApplyOptions::builder()
        .fuzz_factor(fuzz_factor)
        .dry_run(dry_run)
        .build();

    let diff_str = diff.to_string();
    let orig_str = original.map(String::from);

    py.detach(move || {
        ::mpatch::patch_content_str(&diff_str, orig_str.as_deref(), &options)
            .map_err(map_oneshot_err)
    })
}

#[pyfunction]
#[pyo3(signature = (diff, target_dir, *, fuzz_factor=0.7, dry_run=false))]
/// Applies a diff containing multiple patches to a target directory.
///
/// Args:
///     diff (str): The patch content containing one or more file changes.
///     target_dir (str | os.PathLike): The base directory to apply the patches.
///     fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
///     dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.
///
/// Returns:
///     bool: True if all patches succeeded, False if any hard errors occurred.
fn apply_directory(
    py: Python<'_>,
    diff: &str,
    target_dir: PathBuf,
    fuzz_factor: f32,
    dry_run: bool,
) -> PyResult<bool> {
    let options = ApplyOptions::builder()
        .fuzz_factor(fuzz_factor)
        .dry_run(dry_run)
        .build();

    let diff_str = diff.to_string();

    let success = py.detach(move || -> PyResult<bool> {
        let patches = ::mpatch::parse_auto(&diff_str).map_err(map_parse_err)?;
        let result = ::mpatch::apply_patches_to_dir(&patches, &target_dir, options);
        Ok(result.all_succeeded())
    })?;

    Ok(success)
}

#[pyfunction]
#[pyo3(signature = (patch, original=None, *, fuzz_factor=0.7, dry_run=false))]
/// Applies a Patch object to a string content in memory.
///
/// Args:
///     patch (Patch): The patch to apply.
///     original (str | None, optional): The original content. None for file creation. Defaults to None.
///     fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
///     dry_run (bool, optional): If True, returns what would happen without making changes. Default is False.
///
/// Returns:
///     InMemoryResult: The result of the application.
fn apply_patch_to_content(
    py: Python<'_>,
    patch: &PyPatch,
    original: Option<&str>,
    fuzz_factor: f32,
    dry_run: bool,
) -> PyInMemoryResult {
    let options = ApplyOptions::builder()
        .fuzz_factor(fuzz_factor)
        .dry_run(dry_run)
        .build();

    let patch_inner = patch.inner.clone();
    let original_str = original.map(String::from);

    let res = py.detach(move || {
        ::mpatch::apply_patch_to_content(&patch_inner, original_str.as_deref(), &options)
    });

    PyInMemoryResult { inner: res }
}

#[pyfunction]
#[pyo3(signature = (patch, target_dir, *, fuzz_factor=0.7, dry_run=false))]
/// Applies a Patch object to a file on disk.
///
/// Args:
///     patch (Patch): The patch to apply.
///     target_dir (str | os.PathLike): The base directory to apply the patch.
///     fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
///     dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.
///
/// Returns:
///     PatchResult: The result of the application.
fn apply_patch_to_file(
    py: Python<'_>,
    patch: &PyPatch,
    target_dir: PathBuf,
    fuzz_factor: f32,
    dry_run: bool,
) -> PyResult<PyPatchResult> {
    let options = ApplyOptions::builder()
        .fuzz_factor(fuzz_factor)
        .dry_run(dry_run)
        .build();

    let patch_inner = patch.inner.clone();

    let result =
        py.detach(move || ::mpatch::apply_patch_to_file(&patch_inner, &target_dir, options));

    match result {
        Ok(res) => Ok(PyPatchResult { inner: res }),
        Err(e) => {
            if let ::mpatch::PatchError::PathTraversal(_) = e {
                Err(PathTraversalError::new_err(e.to_string()))
            } else {
                Err(ApplyError::new_err(e.to_string()))
            }
        }
    }
}

#[pyfunction]
#[pyo3(signature = (patches, target_dir, *, fuzz_factor=0.7, dry_run=false))]
/// Applies a list of patches to a directory on disk.
///
/// Args:
///     patches (list[Patch]): The patches to apply.
///     target_dir (str | os.PathLike): The base directory to apply the patches.
///     fuzz_factor (float, optional): Similarity threshold (0.0 to 1.0). Default is 0.7.
///     dry_run (bool, optional): If True, previews changes without writing to disk. Default is False.
///
/// Returns:
///     BatchResult: The aggregated results of the applications.
fn apply_patches_to_dir(
    py: Python<'_>,
    patches: Vec<PyPatch>,
    target_dir: PathBuf,
    fuzz_factor: f32,
    dry_run: bool,
) -> PyBatchResult {
    let options = ApplyOptions::builder()
        .fuzz_factor(fuzz_factor)
        .dry_run(dry_run)
        .build();

    let patches_inner: Vec<::mpatch::Patch> = patches.into_iter().map(|p| p.inner).collect();

    let result =
        py.detach(move || ::mpatch::apply_patches_to_dir(&patches_inner, &target_dir, options));

    PyBatchResult { inner: result }
}

#[pymodule]
fn mpatch(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("MpatchError", py.get_type::<MpatchError>())?;
    m.add("ParseError", py.get_type::<ParseError>())?;
    m.add("ApplyError", py.get_type::<ApplyError>())?;
    m.add("PathTraversalError", py.get_type::<PathTraversalError>())?;

    m.add_class::<PyHunk>()?;
    m.add_class::<PyPatch>()?;
    m.add_class::<PyApplyResult>()?;
    m.add_class::<PyHunkFailure>()?;
    m.add_class::<PyHunkApplyStatus>()?;
    m.add_class::<PyInMemoryResult>()?;
    m.add_class::<PyPatchResult>()?;
    m.add_class::<PyBatchResult>()?;

    m.add_function(wrap_pyfunction!(patch_content, m)?)?;
    m.add_function(wrap_pyfunction!(apply_directory, m)?)?;
    m.add_function(wrap_pyfunction!(detect_patch, m)?)?;
    m.add_function(wrap_pyfunction!(parse_auto, m)?)?;
    m.add_function(wrap_pyfunction!(parse_diffs, m)?)?;
    m.add_function(wrap_pyfunction!(parse_patches, m)?)?;
    m.add_function(wrap_pyfunction!(parse_conflict_markers, m)?)?;
    m.add_function(wrap_pyfunction!(invert_patches, m)?)?;
    m.add_function(wrap_pyfunction!(create_unified_diff, m)?)?;
    m.add_function(wrap_pyfunction!(apply_patch_to_content, m)?)?;
    m.add_function(wrap_pyfunction!(apply_patch_to_file, m)?)?;
    m.add_function(wrap_pyfunction!(apply_patches_to_dir, m)?)?;

    // Add library version
    m.add("VERSION", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
