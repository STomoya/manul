use glob::{MatchOptions, glob_with};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::path::PathBuf;
use std::str::FromStr;

/// Define the PathType enum with Python bindings.
/// This enum allows users to specify whether they want to filter for files, directories, or both when using the find_paths function.
#[pyclass(name = "PathType", eq, eq_int, from_py_object)]
#[derive(PartialEq, Clone, Debug)]
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
#[derive(PartialEq, Clone, Debug)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn get_temp_dir(name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let mut path = std::env::temp_dir();
        path.push(format!("manul_core_test_{}_{}", name, millis));
        path
    }

    // Pythonic Enum tests

    #[test]
    fn test_path_type_from_str() {
        assert_eq!(PyPathType::from_str("file").unwrap(), PyPathType::FilesOnly);
        assert_eq!(PyPathType::from_str("f").unwrap(), PyPathType::FilesOnly);
        assert_eq!(
            PyPathType::from_str("directory").unwrap(),
            PyPathType::DirectoriesOnly
        );
        assert_eq!(
            PyPathType::from_str("d").unwrap(),
            PyPathType::DirectoriesOnly
        );
        assert_eq!(PyPathType::from_str("both").unwrap(), PyPathType::Both);
        assert!(PyPathType::from_str("invalid").is_err());
    }

    #[test]
    fn test_path_type_new() {
        Python::initialize();
        assert!(PyPathType::new("file").is_ok());
        assert!(PyPathType::new("invalid").is_err());
    }

    #[test]
    fn test_path_type_str_repr() {
        assert_eq!(PyPathType::FilesOnly.__str__(), "file");
        assert_eq!(PyPathType::FilesOnly.__repr__(), "PathType(\"file\")");

        assert_eq!(PyPathType::DirectoriesOnly.__str__(), "directory");
        assert_eq!(
            PyPathType::DirectoriesOnly.__repr__(),
            "PathType(\"directory\")"
        );

        assert_eq!(PyPathType::Both.__str__(), "both");
        assert_eq!(PyPathType::Both.__repr__(), "PathType(\"both\")");
    }

    #[test]
    fn test_sort_strategy_from_str() {
        assert_eq!(
            PySortStrategy::from_str("none").unwrap(),
            PySortStrategy::No
        );
        assert_eq!(
            PySortStrategy::from_str("standard").unwrap(),
            PySortStrategy::Standard
        );
        assert_eq!(
            PySortStrategy::from_str("natural").unwrap(),
            PySortStrategy::Natural
        );
        assert!(PySortStrategy::from_str("invalid").is_err());
    }

    #[test]
    fn test_sort_strategy_new() {
        Python::initialize();
        assert!(PySortStrategy::new("none").is_ok());
        assert!(PySortStrategy::new("invalid").is_err());
    }

    #[test]
    fn test_sort_strategy_str_repr() {
        assert_eq!(PySortStrategy::No.__str__(), "none");
        assert_eq!(PySortStrategy::No.__repr__(), "SortStrategy(\"none\")");

        assert_eq!(PySortStrategy::Standard.__str__(), "standard");
        assert_eq!(
            PySortStrategy::Standard.__repr__(),
            "SortStrategy(\"standard\")"
        );

        assert_eq!(PySortStrategy::Natural.__str__(), "natural");
        assert_eq!(
            PySortStrategy::Natural.__repr__(),
            "SortStrategy(\"natural\")"
        );
    }

    // Rust pipeline tests

    /// Test for dispatching path collection target based on input parameters.
    #[test]
    fn test_glob_pipeline_path_types() {
        let dir = get_temp_dir("path_types");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("test_file.txt")).unwrap();
        fs::create_dir(dir.join("test_dir")).unwrap();

        let pattern = format!("{}/*", dir.to_string_lossy());

        let files =
            glob_pipeline(&pattern, None, Some(PyPathType::FilesOnly), None, false).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].is_file());

        let dirs = glob_pipeline(
            &pattern,
            None,
            Some(PyPathType::DirectoriesOnly),
            None,
            false,
        )
        .unwrap();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].is_dir());

        let both = glob_pipeline(&pattern, None, Some(PyPathType::Both), None, false).unwrap();
        assert_eq!(both.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    /// Test for keyword-based filtering
    #[test]
    fn test_glob_pipeline_keywords() {
        let dir = get_temp_dir("keywords");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("match_this.txt")).unwrap();
        File::create(dir.join("ignore_that.txt")).unwrap();

        let pattern = format!("{}/*", dir.to_string_lossy());

        let matches = glob_pipeline(&pattern, Some("match"), None, None, false).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].file_name().unwrap().to_str().unwrap(),
            "match_this.txt"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Test for natural and standard sorting
    #[test]
    fn test_glob_pipeline_sorting() {
        let dir = get_temp_dir("sorting");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("file_10.txt")).unwrap();
        File::create(dir.join("file_2.txt")).unwrap();
        File::create(dir.join("file_1.txt")).unwrap();

        let pattern = format!("{}/*", dir.to_string_lossy());

        // Standard sort should put "file_10" before "file_2"
        let standard =
            glob_pipeline(&pattern, None, None, Some(PySortStrategy::Standard), false).unwrap();
        let standard_names: Vec<_> = standard
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            standard_names,
            vec!["file_1.txt", "file_10.txt", "file_2.txt"]
        );

        // Natural sort should put "file_2" before "file_10"
        let natural =
            glob_pipeline(&pattern, None, None, Some(PySortStrategy::Natural), false).unwrap();
        let natural_names: Vec<_> = natural
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            natural_names,
            vec!["file_1.txt", "file_2.txt", "file_10.txt"]
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// Test for including/excluding hidden files.
    #[test]
    fn test_glob_pipeline_hidden() {
        let dir = get_temp_dir("hidden");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("visible.txt")).unwrap();
        File::create(dir.join(".hidden.txt")).unwrap();

        let pattern = format!("{}/*", dir.to_string_lossy());

        // By default (include_hidden=false), glob_with MatchOptions should skip the leading dot
        let no_hidden = glob_pipeline(&pattern, None, None, None, false).unwrap();
        assert_eq!(no_hidden.len(), 1);
        assert_eq!(
            no_hidden[0].file_name().unwrap().to_str().unwrap(),
            "visible.txt"
        );

        // include_hidden=true should include the leading dot
        let with_hidden = glob_pipeline(&pattern, None, None, None, true).unwrap();
        assert_eq!(with_hidden.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    /// Test for invalid glob patterns.
    #[test]
    fn test_glob_pipeline_invalid_pattern() {
        let result = glob_pipeline("***[invalid", None, None, None, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid glob pattern"));
    }

    // Python wrapper tests

    /// Test for succesful pipeline execution.
    #[test]
    fn test_find_paths_python_wrapper() {
        Python::initialize();
        let dir = get_temp_dir("find_paths_wrapper");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("test.txt")).unwrap();

        let pattern = format!("{}/*.txt", dir.to_string_lossy());

        let result = find_paths(&pattern, None, None, None, false);
        assert!(result.is_ok());
        let paths = result.unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].ends_with("test.txt"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// Test for failed pipeline execution.
    #[test]
    fn test_find_paths_invalid_pattern() {
        Python::initialize();
        let result = find_paths("***[invalid", None, None, None, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid glob pattern")
        );
    }
}
