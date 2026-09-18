use manul_logger::logger::{LayerConfig, LayerDestination, LogFormat, init_tracing};

// Runs in its own process: install a global subscriber through a path that bypasses
// `init_tracing`'s internal `Once`, so `Registry::default().try_init()` is guaranteed
// to fail with a `SetGlobalDefaultError`, exercising the `RegistryInit` branch.
#[test]
fn test_init_tracing_registry_error() {
    tracing::subscriber::set_global_default(tracing_subscriber::fmt().finish())
        .expect("failed to install the conflicting subscriber");

    let config = LayerConfig::new(
        "test_layer".to_string(),
        "off".to_string(),
        LogFormat::Compact,
        LayerDestination::Console,
        None,
        None,
        false,
        None,
    );

    let err = match init_tracing(vec![config]) {
        Err(e) => e,
        Ok(_) => panic!("expected the conflicting subscriber to cause a failure"),
    };
    assert!(err.to_string().contains("global default"));
}
