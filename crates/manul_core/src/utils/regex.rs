use aho_corasick::AhoCorasick;
use regex::{Regex, RegexSet};
use std::collections::HashMap;

/// Scans text and returns byte offset tuples (start, end) of every match.
pub fn find_all_offsets(text: &str, pattern: &str) -> Result<Vec<(usize, usize)>, regex::Error> {
    let re = Regex::new(pattern)?;
    let offsets = re.find_iter(text).map(|m| (m.start(), m.end())).collect();
    Ok(offsets)
}

/// Matches a list of patterns and returns the indices of those that matched.
pub fn match_any(text: &str, patterns: &[&str]) -> Result<Vec<usize>, regex::Error> {
    let set = RegexSet::new(patterns)?;
    let matches = set.matches(text).into_iter().collect();
    Ok(matches)
}

/// Replaces multiple substrings simultaneously using Aho-Corasick.
pub fn replace_many(
    text: &str,
    keys: &[&str],
    values: &[&str],
) -> Result<String, aho_corasick::BuildError> {
    let ac = AhoCorasick::builder().build(keys)?;
    // replace_all allocates exactly what is needed and builds the string in one pass
    Ok(ac.replace_all(text, values))
}

/// Optimized regex substitution using Rust's Cow (Copy-on-Write).
pub fn sub_optimized(text: &str, pattern: &str, replacement: &str) -> Result<String, regex::Error> {
    let re = Regex::new(pattern)?;
    // `replace_all` returns a `Cow::Borrowed` if no match is found (zero allocation),
    // and a `Cow::Owned` (single optimized allocation builder) if matches exist.
    Ok(re.replace_all(text, replacement).into_owned())
}

/// Extracts named capture groups into a list of HashMaps.
pub fn extract_structured(
    text: &str,
    pattern: &str,
) -> Result<Vec<HashMap<String, String>>, regex::Error> {
    let re = Regex::new(pattern)?;
    let mut results = Vec::new();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_all_offsets() {
        let offsets = find_all_offsets("hello world hello", "hello").unwrap();
        assert_eq!(offsets, vec![(0, 5), (12, 17)]);
    }

    #[test]
    fn test_find_all_offsets_invalid_pattern() {
        assert!(find_all_offsets("text", "(unclosed").is_err());
    }

    #[test]
    fn test_match_any() {
        let matches = match_any("hello world", &["hello", "xyz", "world"]).unwrap();
        assert_eq!(matches, vec![0, 2]);
    }

    #[test]
    fn test_match_any_invalid_pattern() {
        assert!(match_any("text", &["(unclosed"]).is_err());
    }

    #[test]
    fn test_replace_many() {
        let result = replace_many("foo bar", &["foo", "bar"], &["baz", "qux"]).unwrap();
        assert_eq!(result, "baz qux");
    }

    #[test]
    fn test_sub_optimized() {
        let result = sub_optimized("hello world", "world", "there").unwrap();
        assert_eq!(result, "hello there");
    }

    #[test]
    fn test_sub_optimized_no_match() {
        let result = sub_optimized("hello world", "xyz", "there").unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_sub_optimized_invalid_pattern() {
        assert!(sub_optimized("text", "(unclosed", "x").is_err());
    }

    #[test]
    fn test_extract_structured() {
        let result = extract_structured("John:25", r"(?P<name>\w+):(?P<age>\d+)").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].get("name").unwrap(), "John");
        assert_eq!(result[0].get("age").unwrap(), "25");
    }

    #[test]
    fn test_extract_structured_no_named_groups() {
        let result = extract_structured("John:25", r"\w+:\d+").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_structured_invalid_pattern() {
        assert!(extract_structured("text", "(unclosed").is_err());
    }
}
