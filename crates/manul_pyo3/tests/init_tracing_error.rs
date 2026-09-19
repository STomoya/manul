use _manul::logger::{PyLayerConfig, PyLayerDestination, PyLogFormat, init_tracing};
use pyo3::Python;
use pyo3::exceptions::PyRuntimeError;

// Runs in its own process: install a global subscriber through a path that bypasses
// `init_tracing`'s internal `Once`, so the underlying `manul_logger::init_tracing` call
// is guaranteed to fail, and the wrapper must surface that as a `PyRuntimeError`.
#[test]
fn test_init_tracing_wrapper_maps_registry_error_to_runtime_error() {
    tracing::subscriber::set_global_default(tracing_subscriber::fmt().finish())
        .expect("failed to install the conflicting subscriber");

    Python::initialize();

    let config = PyLayerConfig {
        name: "test_layer".to_string(),
        filter_directive: "off".to_string(),
        format: PyLogFormat::Compact,
        destination: PyLayerDestination::Console,
        file_dir: None,
        file_prefix: None,
        include_span_events: false,
        max_log_files: None,
        sample_directive: None,
    };

    let err = match init_tracing(vec![config]) {
        Err(e) => e,
        Ok(_) => panic!("expected the conflicting subscriber to cause a failure"),
    };

    Python::attach(|py| {
        assert!(err.is_instance_of::<PyRuntimeError>(py));
    });
}
