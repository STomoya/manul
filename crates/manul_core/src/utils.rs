use glob::{MatchOptions, glob_with};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::PathBuf;
use std::str::FromStr;

/// Define the PathType enum with Python bindings.
/// This enum allows users to specify whether they want to filter for files, directories, or both when using the find_paths function.
#[pyclass(name = "PathType", eq, eq_int, from_py_object)]
#[derive(PartialEq, Clone)]
pub enum PyPathType {
    FilesOnly,
    DirectoriesOnly,
    Both,
}

#[pymethods]
impl PyPathType {
    #[new]
    /// Create a new PathType from a string. The string can be "file", "directory", or "both" (case-insensitive).
    /// # Arguments
    /// * `value` - The string representation of the PathType.
    pub fn new(value: &str) -> PyResult<Self> {
        PyPathType::from_str(value).map_err(|e: String| PyValueError::new_err(e))
    }

    fn __str__(&self) -> String {
        match self {
            PyPathType::FilesOnly => "file".into(),
            PyPathType::DirectoriesOnly => "directory".into(),
            PyPathType::Both => "both".into(),
        }
    }

    fn __repr__(&self) -> String {
        let self_string = self.__str__();
        format!("PathType(\"{}\")", self_string)
    }
}

impl FromStr for PyPathType {
    type Err = String;

    /// Create a new PathType from a string. The string can be "file", "directory", or "both" (case-insensitive).
    /// # Arguments
    /// * `s` - The string representation of the PathType.
    /// # Errors
    /// This function will return an error if the input string does not match any of the valid
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "file" => Ok(PyPathType::FilesOnly),
            "f" => Ok(PyPathType::FilesOnly),
            "directory" => Ok(PyPathType::DirectoriesOnly),
            "d" => Ok(PyPathType::DirectoriesOnly),
            "both" => Ok(PyPathType::Both),
            _ => Err(format!("Invalid PathType: {}", s)),
        }
    }
}

/// Define the SortStrategy enum with Python bindings.
/// This enum allows users to specify how results should be sorted when using the find_paths function.
/// # Arguments
/// * `value` - The string representation of the SortStrategy.
#[pyclass(name = "SortStrategy", eq, eq_int, from_py_object)]
#[derive(PartialEq, Clone)]
pub enum PySortStrategy {
    // We want to use None but None is a reserved keyword in Python, so we use No instead and map it to "none" in the string representation.
    No,
    Standard,
    Natural,
}

#[pymethods]
impl PySortStrategy {
    #[new]
    /// Create a new SortStrategy from a string. The string can be "none", "standard", or "natural" (case-insensitive).
    /// # Arguments
    /// * `value` - The string representation of the SortStrategy.
    pub fn new(value: &str) -> PyResult<Self> {
        PySortStrategy::from_str(value).map_err(|e: String| PyValueError::new_err(e))
    }

    fn __str__(&self) -> String {
        match self {
            PySortStrategy::No => "none".into(),
            PySortStrategy::Standard => "standard".into(),
            PySortStrategy::Natural => "natural".into(),
        }
    }

    fn __repr__(&self) -> String {
        let self_string = self.__str__();
        format!("SortStrategy(\"{}\")", self_string)
    }
}

impl FromStr for PySortStrategy {
    type Err = String;

    /// Create a new SortStrategy from a string. The string can be "none", "standard", or "natural" (case-insensitive).
    /// # Arguments
    /// * `s` - The string representation of the SortStrategy.
    /// # Errors
    /// This function will return an error if the input string does not match any of the valid options.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(PySortStrategy::No),
            "standard" => Ok(PySortStrategy::Standard),
            "natural" => Ok(PySortStrategy::Natural),
            _ => Err(format!("Invalid SortStrategy: {}", s)),
        }
    }
}

/// The core Rust function that performs the globbing, filtering, and sorting logic.
/// This function is not exposed to Python directly, but is called by the Python wrapper.
/// # Arguments
/// * `pattern` - The glob pattern to search for.
/// * `keyword` - An optional keyword to filter results by name.
/// * `path_type` - An optional PathType to filter by files, directories, or both.
/// * `sort_strategy` - An optional SortStrategy to determine how results are sorted.
/// # Panics
/// This function will return an error if the glob pattern is invalid.
fn glob_pipeline(
    pattern: &str,
    keyword: Option<&str>,
    path_type: Option<PyPathType>,
    sort_strategy: Option<PySortStrategy>,
    include_hidden: bool,
) -> Result<Vec<PathBuf>, String> {
    let target_type = path_type.unwrap_or(PyPathType::Both);
    let target_sort = sort_strategy.unwrap_or(PySortStrategy::No);

    let options = MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: !include_hidden, // If include_hidden is false, we require a literal leading dot to exclude hidden files
    };

    let entries =
        glob_with(pattern, options).map_err(|e| format!("Invalid glob pattern: {}", e))?;

    let mut results: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|path| {
            match target_type {
                PyPathType::FilesOnly if !path.is_file() => return false,
                PyPathType::DirectoriesOnly if !path.is_dir() => return false,
                _ => {}
            }

            if let Some(kw) = keyword {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.contains(kw))
                    .unwrap_or(false)
            } else {
                true
            }
        })
        .collect();

    match target_sort {
        PySortStrategy::Natural => {
            results.sort_unstable_by(|a, b| {
                let a_str = a.to_string_lossy();
                let b_str = b.to_string_lossy();
                natord::compare(&a_str, &b_str)
            });
        }
        PySortStrategy::Standard => results.sort_unstable(),
        PySortStrategy::No => {}
    }

    Ok(results)
}

/// The Python wrapper function that is exposed to Python. This function handles the conversion of arguments and return values between Rust and Python.
/// # Arguments
/// * `pattern` - The glob pattern to search for.
/// * `keyword` - An optional keyword to filter results by name.
/// * `path_type` - An optional PathType to filter by files, directories, or both.
/// * `sort_strategy` - An optional SortStrategy to determine how results are sorted.
/// # Returns
/// A list of strings representing the paths that match the glob pattern and filters. This will be converted to a Python list of strings by PyO3.
#[pyfunction]
#[pyo3(signature = (pattern, keyword=None, path_type=None, sort_strategy=None, include_hidden=false))]
pub fn find_paths(
    pattern: &str,
    keyword: Option<&str>,
    path_type: Option<PyPathType>,
    sort_strategy: Option<PySortStrategy>,
    include_hidden: bool,
) -> PyResult<Vec<String>> {
    // PyO3 automatically converts Vec<String> into a Python list[str]

    // Call the pure Rust pipeline
    match glob_pipeline(pattern, keyword, path_type, sort_strategy, include_hidden) {
        Ok(paths) => {
            // Convert Rust PathBufs back to standard Strings for Python
            let string_paths = paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            Ok(string_paths)
        }
        Err(e) => {
            // If the glob pattern is invalid, throw a standard Python ValueError
            Err(PyValueError::new_err(e))
        }
    }
}
