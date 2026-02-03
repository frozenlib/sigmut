#[test]
fn timeout_invalid_duration_literal_fails() {
    trybuild::TestCases::new()
        .compile_fail("tests/compile_fail/timeout/invalid_duration_literal.rs");
}
