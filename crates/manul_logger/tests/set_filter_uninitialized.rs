use manul_logger::logger::set_filter;

// Runs in its own process: must not observe any other test's `init_tracing` call.
#[test]
fn test_set_filter_before_init_tracing_reports_not_initialized() {
    let err = set_filter("anything", "info").expect_err("should fail before init_tracing runs");
    assert!(err.to_string().contains("not been initialized"));
}
