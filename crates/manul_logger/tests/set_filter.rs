use manul_logger::logger::{LayerConfig, LayerDestination, LogFormat, init_tracing, set_filter};

// Runs in its own process (separate integration-test binary): `init_tracing` installs a
// real process-wide global subscriber via a `Once`, which would collide with other tests.
#[test]
fn test_set_filter_reloads_a_running_layer_and_rejects_unknown_names() {
    let dir = std::env::temp_dir().join(format!(
        "manul_logger_test_set_filter_{}",
        std::process::id()
    ));
    let config = LayerConfig::new(
        "adjustable".to_string(),
        "off".to_string(),
        LogFormat::Compact,
        LayerDestination::File,
        Some(dir.to_string_lossy().into_owned()),
        Some("test".to_string()),
        false,
        None,
        None,
        false,
    );

    let guards = init_tracing(vec![config]).expect("init_tracing should succeed");

    tracing::info!("should be filtered out");

    set_filter("adjustable", "info").expect("reloading a known layer should succeed");

    tracing::info!("should be captured");

    drop(guards); // Flush the non-blocking writer's buffered output.

    let mut contents = String::new();
    for entry in std::fs::read_dir(&dir).expect("log dir should exist") {
        contents.push_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap());
    }
    assert!(!contents.contains("should be filtered out"));
    assert!(contents.contains("should be captured"));

    let err = set_filter("unknown", "info").expect_err("unknown layer names should be rejected");
    assert!(err.to_string().contains("Unknown layer"));

    let _ = std::fs::remove_dir_all(&dir);
}
