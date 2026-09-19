use _manul::logger::{PyLayerConfig, PyLayerDestination, PyLogFormat, init_tracing, set_filter};
use pyo3::Python;
use pyo3::exceptions::PyRuntimeError;

// Runs in its own process: `init_tracing` installs a real process-wide global subscriber
// via a `Once`, which would collide with other tests' calls to it.
#[test]
fn test_set_filter_wrapper_reloads_a_known_layer_and_rejects_unknown_names() {
    Python::initialize();

    let dir =
        std::env::temp_dir().join(format!("manul_pyo3_test_set_filter_{}", std::process::id()));
    let config = PyLayerConfig {
        name: "wrapper_adjustable".to_string(),
        filter_directive: "off".to_string(),
        format: PyLogFormat::Compact,
        destination: PyLayerDestination::File,
        file_dir: Some(dir.to_string_lossy().into_owned()),
        file_prefix: Some("test".to_string()),
        include_span_events: false,
        max_log_files: None,
    };

    let _guard = init_tracing(vec![config]).expect("init_tracing should succeed");

    set_filter("wrapper_adjustable", "info").expect("reloading a known layer should succeed");

    let err = set_filter("missing_layer", "info").expect_err("unknown layer names should error");
    Python::attach(|py| {
        assert!(err.is_instance_of::<PyRuntimeError>(py));
    });

    let _ = std::fs::remove_dir_all(&dir);
}
