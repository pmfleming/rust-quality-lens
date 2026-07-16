use golden_rust_project::alpha::alpha_value;

#[test]
fn duplicate_name() {
    assert_eq!(alpha_value(), 10);
}

#[rstest]
fn macro_style_test() {}
