use aho_corasick::AhoCorasick;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use regex::{Regex, RegexSet};
use std::collections::HashMap;

// =====================================================================
// PURE RUST CORE FUNCTIONS
// These are isolated from Python. You can write standard #[test] functions
// against these to ensure your logic is sound and highly performant.
// =====================================================================

/// Core: Scans text and returns byte offset tuples (start, end)
fn core_find_all_offsets(text: &str, pattern: &str) -> Result<Vec<(usize, usize)>, regex::Error> {
    let re = Regex::new(pattern)?;
    let offsets = re.find_iter(text).map(|m| (m.start(), m.end())).collect();
    Ok(offsets)
}

/// Core: Matches a list of patterns and returns the indices of those that matched
fn core_match_any(text: &str, patterns: &[&str]) -> Result<Vec<usize>, regex::Error> {
    let set = RegexSet::new(patterns)?;
    let matches = set.matches(text).into_iter().collect();
    Ok(matches)
}

/// Core: Replaces multiple substrings simultaneously using Aho-Corasick
fn core_replace_many(
    text: &str,
    keys: &[&str],
    values: &[&str],
) -> Result<String, aho_corasick::BuildError> {
    let ac = AhoCorasick::builder().build(keys)?;
    // replace_all allocates exactly what is needed and builds the string in one pass
    Ok(ac.replace_all(text, values))
}

/// Core: Optimized regex substitution using Rust's Cow (Copy-on-Write)
fn core_sub_optimized(
    text: &str,
    pattern: &str,
    replacement: &str,
) -> Result<String, regex::Error> {
    let re = Regex::new(pattern)?;
    // `replace_all` returns a `Cow::Borrowed` if no match is found (zero allocation),
    // and a `Cow::Owned` (single optimized allocation builder) if matches exist.
    Ok(re.replace_all(text, replacement).into_owned())
}

/// Core: Extracts named capture groups into a list of HashMaps
fn core_extract_structured(
    text: &str,
    pattern: &str,
) -> Result<Vec<HashMap<String, String>>, regex::Error> {
    let re = Regex::new(pattern)?;
    let mut results = Vec::new();

    // Extract capture group names (ignoring None for unnamed groups)
    let capture_names: Vec<&str> = re.capture_names().flatten().collect();

    for cap in re.captures_iter(text) {
        let mut map = HashMap::new();
        for &name in &capture_names {
            if let Some(match_str) = cap.name(name) {
                map.insert(name.to_string(), match_str.as_str().to_string());
            }
        }
        if !map.is_empty() {
            results.push(map);
        }
    }

    Ok(results)
}

// =====================================================================
// PYO3 PYTHON WRAPPERS
// These functions extract data from Python objects, pass them to the
// pure Rust functions, and handle translating Rust Errors to Python Exceptions.
// =====================================================================

/// Wrapper for `core_find_all_offsets`
#[pyfunction]
pub fn find_all_offsets(text: &str, pattern: &str) -> PyResult<Vec<(usize, usize)>> {
    core_find_all_offsets(text, pattern)
        .map_err(|e| PyValueError::new_err(format!("Invalid regex: {}", e)))
}

/// Wrapper for `core_match_any`
#[pyfunction]
pub fn match_any(text: &str, patterns: Vec<String>) -> PyResult<Vec<usize>> {
    // Convert Vec<String> to Vec<&str> to satisfy the core function
    let pattern_refs: Vec<&str> = patterns.iter().map(|s| s.as_str()).collect();

    core_match_any(text, &pattern_refs)
        .map_err(|e| PyValueError::new_err(format!("Invalid regex set: {}", e)))
}

/// Wrapper for `core_replace_many`
#[pyfunction]
pub fn replace_many(text: &str, replacements: &Bound<'_, PyDict>) -> PyResult<String> {
    // 1. Change these to store owned Strings
    let mut keys = Vec::new();
    let mut values = Vec::new();

    for (k, v) in replacements.iter() {
        // 2. Extract as String instead of &str
        let key_str: String = k.extract()?;
        let val_str: String = v.extract()?;
        keys.push(key_str);
        values.push(val_str);
    }

    // 3. Convert Vec<String> to Vec<&str> for the core function
    // This works because the Strings in 'keys' live through this whole call.
    let keys_ref: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let values_ref: Vec<&str> = values.iter().map(|s| s.as_str()).collect();

    core_replace_many(text, &keys_ref, &values_ref)
        .map_err(|e| PyValueError::new_err(format!("Aho-Corasick build error: {}", e)))
}

/// Wrapper for `core_sub_optimized`
#[pyfunction]
pub fn sub_optimized(text: &str, pattern: &str, replacement: &str) -> PyResult<String> {
    core_sub_optimized(text, pattern, replacement)
        .map_err(|e| PyValueError::new_err(format!("Invalid regex: {}", e)))
}

/// Wrapper for `core_extract_structured`
/// Note: PyO3 automatically converts Vec<HashMap<String, String>> into Python's List[Dict[str, str]]
#[pyfunction]
pub fn extract_structured(text: &str, pattern: &str) -> PyResult<Vec<HashMap<String, String>>> {
    core_extract_structured(text, pattern)
        .map_err(|e| PyValueError::new_err(format!("Invalid regex: {}", e)))
}
