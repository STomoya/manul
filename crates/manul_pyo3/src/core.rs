use manul_core::utils::{PathType, SortStrategy};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::str::FromStr;

#[pymodule(name = "_core")]
pub mod core_bindings {
    #[allow(non_upper_case_globals)]
    #[pymodule_export]
    pub const __version__: &str = ::manul_core::VERSION;

    #[pymodule_export]
    pub use super::{PyPathType, PySortStrategy, find_paths};

    #[pymodule_export]
    pub use super::{extract_structured, find_all_offsets, match_any, replace_many, sub_optimized};
}

/// Python-facing `PathType` enum. Mirrors `manul_core::utils::PathType`.
#[pyclass(name = "PathType", eq, eq_int, from_py_object)]
#[derive(PartialEq, Clone, Debug)]
pub enum PyPathType {
    FilesOnly,
    DirectoriesOnly,
    Both,
}

impl From<PathType> for PyPathType {
    fn from(value: PathType) -> Self {
        match value {
            PathType::FilesOnly => PyPathType::FilesOnly,
            PathType::DirectoriesOnly => PyPathType::DirectoriesOnly,
            PathType::Both => PyPathType::Both,
        }
    }
}

impl From<PyPathType> for PathType {
    fn from(value: PyPathType) -> Self {
        match value {
            PyPathType::FilesOnly => PathType::FilesOnly,
            PyPathType::DirectoriesOnly => PathType::DirectoriesOnly,
            PyPathType::Both => PathType::Both,
        }
    }
}

#[pymethods]
impl PyPathType {
    #[new]
    pub fn new(value: &str) -> PyResult<Self> {
        PathType::from_str(value)
            .map(PyPathType::from)
            .map_err(PyValueError::new_err)
    }

    fn __str__(&self) -> String {
        PathType::from(self.clone()).to_str().to_string()
    }

    fn __repr__(&self) -> String {
        format!("PathType(\"{}\")", self.__str__())
    }
}

/// Python-facing `SortStrategy` enum. Mirrors `manul_core::utils::SortStrategy`.
#[pyclass(name = "SortStrategy", eq, eq_int, from_py_object)]
#[derive(PartialEq, Clone, Debug)]
pub enum PySortStrategy {
    No,
    Standard,
    Natural,
}

impl From<SortStrategy> for PySortStrategy {
    fn from(value: SortStrategy) -> Self {
        match value {
            SortStrategy::No => PySortStrategy::No,
            SortStrategy::Standard => PySortStrategy::Standard,
            SortStrategy::Natural => PySortStrategy::Natural,
        }
    }
}

impl From<PySortStrategy> for SortStrategy {
    fn from(value: PySortStrategy) -> Self {
        match value {
            PySortStrategy::No => SortStrategy::No,
            PySortStrategy::Standard => SortStrategy::Standard,
            PySortStrategy::Natural => SortStrategy::Natural,
        }
    }
}

#[pymethods]
impl PySortStrategy {
    #[new]
    pub fn new(value: &str) -> PyResult<Self> {
        SortStrategy::from_str(value)
            .map(PySortStrategy::from)
            .map_err(PyValueError::new_err)
    }

    fn __str__(&self) -> String {
        SortStrategy::from(self.clone()).to_str().to_string()
    }

    fn __repr__(&self) -> String {
        format!("SortStrategy(\"{}\")", self.__str__())
    }
}

#[pyfunction]
#[pyo3(signature = (pattern, keyword=None, path_type=None, sort_strategy=None, include_hidden=false))]
pub fn find_paths(
    pattern: &str,
    keyword: Option<&str>,
    path_type: Option<PyPathType>,
    sort_strategy: Option<PySortStrategy>,
    include_hidden: bool,
) -> PyResult<Vec<String>> {
    let core_path_type = path_type.map(PathType::from);
    let core_sort_strategy = sort_strategy.map(SortStrategy::from);

    match manul_core::utils::glob_pipeline(
        pattern,
        keyword,
        core_path_type,
        core_sort_strategy,
        include_hidden,
    ) {
        Ok(paths) => {
            let string_paths = paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            Ok(string_paths)
        }
        Err(e) => Err(PyValueError::new_err(e)),
    }
}

#[pyfunction]
pub fn find_all_offsets(text: &str, pattern: &str) -> PyResult<Vec<(usize, usize)>> {
    manul_core::utils::find_all_offsets(text, pattern)
        .map_err(|e| PyValueError::new_err(format!("Invalid regex: {}", e)))
}

#[pyfunction]
pub fn match_any(text: &str, patterns: Vec<String>) -> PyResult<Vec<usize>> {
    let pattern_refs: Vec<&str> = patterns.iter().map(|s| s.as_str()).collect();

    manul_core::utils::match_any(text, &pattern_refs)
        .map_err(|e| PyValueError::new_err(format!("Invalid regex set: {}", e)))
}

#[pyfunction]
pub fn replace_many(text: &str, replacements: &Bound<'_, PyDict>) -> PyResult<String> {
    let mut keys = Vec::new();
    let mut values = Vec::new();

    for (k, v) in replacements.iter() {
        let key_str: String = k.extract()?;
        let val_str: String = v.extract()?;
        keys.push(key_str);
        values.push(val_str);
    }

    let keys_ref: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let values_ref: Vec<&str> = values.iter().map(|s| s.as_str()).collect();

    manul_core::utils::replace_many(text, &keys_ref, &values_ref)
        .map_err(|e| PyValueError::new_err(format!("Aho-Corasick build error: {}", e)))
}

#[pyfunction]
pub fn sub_optimized(text: &str, pattern: &str, replacement: &str) -> PyResult<String> {
    manul_core::utils::sub_optimized(text, pattern, replacement)
        .map_err(|e| PyValueError::new_err(format!("Invalid regex: {}", e)))
}

#[pyfunction]
pub fn extract_structured(text: &str, pattern: &str) -> PyResult<Vec<HashMap<String, String>>> {
    manul_core::utils::extract_structured(text, pattern)
        .map_err(|e| PyValueError::new_err(format!("Invalid regex: {}", e)))
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
        path.push(format!("manul_pyo3_test_{}_{}", name, millis));
        path
    }

    #[test]
    fn test_py_path_type_conversions() {
        assert_eq!(PathType::from(PyPathType::FilesOnly), PathType::FilesOnly);
        assert_eq!(PyPathType::from(PathType::Both), PyPathType::Both);
    }

    #[test]
    fn test_py_path_type_new_and_repr() {
        Python::initialize();
        let value = PyPathType::new("file").unwrap();
        assert_eq!(value, PyPathType::FilesOnly);
        assert_eq!(value.__str__(), "file");
        assert_eq!(value.__repr__(), "PathType(\"file\")");
        assert!(PyPathType::new("bogus").is_err());
    }

    #[test]
    fn test_py_sort_strategy_new_and_repr() {
        Python::initialize();
        let value = PySortStrategy::new("natural").unwrap();
        assert_eq!(value, PySortStrategy::Natural);
        assert_eq!(value.__str__(), "natural");
        assert_eq!(value.__repr__(), "SortStrategy(\"natural\")");
        assert!(PySortStrategy::new("bogus").is_err());
    }

    #[test]
    fn test_find_paths_wrapper() {
        Python::initialize();
        let dir = get_temp_dir("find_paths");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("test.txt")).unwrap();

        let pattern = format!("{}/*.txt", dir.to_string_lossy());
        let result = find_paths(&pattern, None, None, None, false).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].ends_with("test.txt"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_paths_invalid_pattern() {
        Python::initialize();
        let result = find_paths("***[invalid", None, None, None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_regex_wrapper_smoke() {
        Python::initialize();
        assert_eq!(
            find_all_offsets("aa bb aa", "aa").unwrap(),
            vec![(0, 2), (6, 8)]
        );
        assert_eq!(
            match_any("hello", vec!["hello".to_string()]).unwrap(),
            vec![0]
        );
        assert_eq!(
            sub_optimized("hello world", "world", "there").unwrap(),
            "hello there"
        );
    }
}
