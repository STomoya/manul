use manul_logger::logger::{LayerConfig, LayerDestination, LogFormat, init_tracing};

// Runs in its own process (separate integration-test binary) because `init_tracing`
// installs a real process-wide global tracing subscriber via a `Once`, which would
// collide with the global subscribers other unit tests install (e.g. `#[traced_test]`).
#[test]
fn test_init_tracing_success_and_idempotent() {
    let dir = std::env::temp_dir().join(format!("manul_logger_test_{}", std::process::id()));
    let config = LayerConfig::new(
        "test_layer".to_string(),
        "off".to_string(),
        LogFormat::Compact,
        LayerDestination::File,
        Some(dir.to_string_lossy().into_owned()),
        Some("test".to_string()),
        false,
        None,
        None,
    );

    let first = init_tracing(vec![config.clone()]);
    assert!(first.is_ok());
    assert_eq!(first.unwrap().guards.len(), 1);

    // A second call must not re-initialize the global subscriber and should
    // return successfully with no new guards instead of erroring.
    let second = init_tracing(vec![config]);
    assert!(second.is_ok());
    assert!(second.unwrap().guards.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}
