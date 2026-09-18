use glob::{MatchOptions, glob_with};
use std::path::PathBuf;
use std::str::FromStr;

/// The kind of filesystem entry to keep when filtering glob results.
#[derive(PartialEq, Clone, Debug)]
pub enum PathType {
    FilesOnly,
    DirectoriesOnly,
    Both,
}

impl PathType {
    pub fn to_str(&self) -> &'static str {
        match self {
            PathType::FilesOnly => "file",
            PathType::DirectoriesOnly => "directory",
            PathType::Both => "both",
        }
    }
}

impl FromStr for PathType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "file" => Ok(PathType::FilesOnly),
            "f" => Ok(PathType::FilesOnly),
            "directory" => Ok(PathType::DirectoriesOnly),
            "d" => Ok(PathType::DirectoriesOnly),
            "both" => Ok(PathType::Both),
            _ => Err(format!("Invalid PathType: {}", s)),
        }
    }
}

/// How glob results should be ordered.
#[derive(PartialEq, Clone, Debug)]
pub enum SortStrategy {
    No,
    Standard,
    Natural,
}

impl SortStrategy {
    pub fn to_str(&self) -> &'static str {
        match self {
            SortStrategy::No => "none",
            SortStrategy::Standard => "standard",
            SortStrategy::Natural => "natural",
        }
    }
}

impl FromStr for SortStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(SortStrategy::No),
            "standard" => Ok(SortStrategy::Standard),
            "natural" => Ok(SortStrategy::Natural),
            _ => Err(format!("Invalid SortStrategy: {}", s)),
        }
    }
}

/// Performs the globbing, filtering, and sorting logic.
/// # Arguments
/// * `pattern` - The glob pattern to search for.
/// * `keyword` - An optional keyword to filter results by name.
/// * `path_type` - An optional PathType to filter by files, directories, or both.
/// * `sort_strategy` - An optional SortStrategy to determine how results are sorted.
pub fn glob_pipeline(
    pattern: &str,
    keyword: Option<&str>,
    path_type: Option<PathType>,
    sort_strategy: Option<SortStrategy>,
    include_hidden: bool,
) -> Result<Vec<PathBuf>, String> {
    let target_type = path_type.unwrap_or(PathType::Both);
    let target_sort = sort_strategy.unwrap_or(SortStrategy::No);

    let options = MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: !include_hidden,
    };

    let entries =
        glob_with(pattern, options).map_err(|e| format!("Invalid glob pattern: {}", e))?;

    let mut results: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|path| {
            match target_type {
                PathType::FilesOnly if !path.is_file() => return false,
                PathType::DirectoriesOnly if !path.is_dir() => return false,
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
        SortStrategy::Natural => {
            results.sort_unstable_by(|a, b| {
                let a_str = a.to_string_lossy();
                let b_str = b.to_string_lossy();
                natord::compare(&a_str, &b_str)
            });
        }
        SortStrategy::Standard => results.sort_unstable(),
        SortStrategy::No => {}
    }

    Ok(results)
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

    #[test]
    fn test_path_type_from_str() {
        assert_eq!(PathType::from_str("file").unwrap(), PathType::FilesOnly);
        assert_eq!(PathType::from_str("f").unwrap(), PathType::FilesOnly);
        assert_eq!(
            PathType::from_str("directory").unwrap(),
            PathType::DirectoriesOnly
        );
        assert_eq!(PathType::from_str("d").unwrap(), PathType::DirectoriesOnly);
        assert_eq!(PathType::from_str("both").unwrap(), PathType::Both);
        assert!(PathType::from_str("invalid").is_err());
    }

    #[test]
    fn test_path_type_to_str() {
        assert_eq!(PathType::FilesOnly.to_str(), "file");
        assert_eq!(PathType::DirectoriesOnly.to_str(), "directory");
        assert_eq!(PathType::Both.to_str(), "both");
    }

    #[test]
    fn test_sort_strategy_from_str() {
        assert_eq!(SortStrategy::from_str("none").unwrap(), SortStrategy::No);
        assert_eq!(
            SortStrategy::from_str("standard").unwrap(),
            SortStrategy::Standard
        );
        assert_eq!(
            SortStrategy::from_str("natural").unwrap(),
            SortStrategy::Natural
        );
        assert!(SortStrategy::from_str("invalid").is_err());
    }

    #[test]
    fn test_sort_strategy_to_str() {
        assert_eq!(SortStrategy::No.to_str(), "none");
        assert_eq!(SortStrategy::Standard.to_str(), "standard");
        assert_eq!(SortStrategy::Natural.to_str(), "natural");
    }

    #[test]
    fn test_glob_pipeline_path_types() {
        let dir = get_temp_dir("path_types");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("test_file.txt")).unwrap();
        fs::create_dir(dir.join("test_dir")).unwrap();

        let pattern = format!("{}/*", dir.to_string_lossy());

        let files = glob_pipeline(&pattern, None, Some(PathType::FilesOnly), None, false).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].is_file());

        let dirs =
            glob_pipeline(&pattern, None, Some(PathType::DirectoriesOnly), None, false).unwrap();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].is_dir());

        let both = glob_pipeline(&pattern, None, Some(PathType::Both), None, false).unwrap();
        assert_eq!(both.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

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

    #[test]
    fn test_glob_pipeline_sorting() {
        let dir = get_temp_dir("sorting");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("file_10.txt")).unwrap();
        File::create(dir.join("file_2.txt")).unwrap();
        File::create(dir.join("file_1.txt")).unwrap();

        let pattern = format!("{}/*", dir.to_string_lossy());

        let standard =
            glob_pipeline(&pattern, None, None, Some(SortStrategy::Standard), false).unwrap();
        let standard_names: Vec<_> = standard
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            standard_names,
            vec!["file_1.txt", "file_10.txt", "file_2.txt"]
        );

        let natural =
            glob_pipeline(&pattern, None, None, Some(SortStrategy::Natural), false).unwrap();
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

    #[test]
    fn test_glob_pipeline_hidden() {
        let dir = get_temp_dir("hidden");
        fs::create_dir_all(&dir).unwrap();
        File::create(dir.join("visible.txt")).unwrap();
        File::create(dir.join(".hidden.txt")).unwrap();

        let pattern = format!("{}/*", dir.to_string_lossy());

        let no_hidden = glob_pipeline(&pattern, None, None, None, false).unwrap();
        assert_eq!(no_hidden.len(), 1);
        assert_eq!(
            no_hidden[0].file_name().unwrap().to_str().unwrap(),
            "visible.txt"
        );

        let with_hidden = glob_pipeline(&pattern, None, None, None, true).unwrap();
        assert_eq!(with_hidden.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_glob_pipeline_invalid_pattern() {
        let result = glob_pipeline("***[invalid", None, None, None, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid glob pattern"));
    }
}
