use _manul::logger::{_log_sink, PyLayerConfig, PyLayerDestination, PyLogFormat, init_tracing};
use pyo3::Python;
use pyo3::types::{PyDict, PyDictMethods, PyList};

// Runs in its own process: `init_tracing` installs a real process-wide global subscriber
// via a `Once`, which would collide with other tests' calls to it.
#[test]
fn test_sample_directive_thins_events_within_the_named_span_only() {
    Python::initialize();

    let dir = std::env::temp_dir().join(format!("manul_pyo3_test_sampling_{}", std::process::id()));
    let config = PyLayerConfig {
        name: "sampled".to_string(),
        filter_directive: "trace".to_string(),
        format: PyLogFormat::Compact,
        destination: PyLayerDestination::File,
        file_dir: Some(dir.to_string_lossy().into_owned()),
        file_prefix: Some("test".to_string()),
        include_span_events: false,
        max_log_files: None,
        sample_directive: Some("db_query:debug:5".to_string()),
        use_local_time: false,
    };

    let guard = init_tracing(vec![config]).expect("init_tracing should succeed");

    Python::attach(|py| {
        let frame = PyDict::new(py);
        frame.set_item("name", "db_query").unwrap();
        frame.set_item("fields", PyDict::new(py)).unwrap();
        let spans = PyList::new(py, [frame]).unwrap();

        for i in 0..10 {
            _log_sink(
                10,
                &format!("in-span debug {i}"),
                None,
                None,
                None,
                None,
                None,
                Some(spans.clone()),
                None,
            );
        }
        for i in 0..3 {
            _log_sink(
                10,
                &format!("no-span debug {i}"),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
        }
    });

    drop(guard); // Flush the non-blocking writer's buffered output.

    let mut contents = String::new();
    for entry in std::fs::read_dir(&dir).expect("log dir should exist") {
        contents.push_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap());
    }
    assert_eq!(contents.matches("in-span debug").count(), 2); // keeps 1-in-5 of 10
    assert_eq!(contents.matches("no-span debug").count(), 3); // untouched: not in the span

    let _ = std::fs::remove_dir_all(&dir);
}
