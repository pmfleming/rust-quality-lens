// Executable candidate data, not independently reviewed ranking labels.
// Contract: clamp to [0,10], preserving NaN and signed zero.
fn before(value: f64) -> f64 {
    if value < 0.0 {
        return 0.0;
    }
    if value > 10.0 {
        return 10.0;
    }
    value
}
fn equivalent(value: f64) -> f64 {
    value.clamp(0.0, 10.0)
}
fn incorrect_smaller(value: f64) -> f64 {
    value.max(0.0).min(10.0)
}

fn owned_before(value: &String) -> String {
    value.clone()
}
#[cfg(not(broken_ownership))]
fn owned_after(value: &str) -> String {
    value.to_owned()
}
#[cfg(broken_ownership)]
fn owned_after(value: &str) -> &str {
    value
}

fn main() {
    let original = String::from("still owned by the caller");
    let independent: String = owned_after(&original);
    assert_eq!(independent, owned_before(&original));
    assert_eq!(original, "still owned by the caller");
    for value in [
        f64::NEG_INFINITY,
        -11.0,
        -1.0,
        -0.0,
        0.0,
        1.0,
        9.0,
        10.0,
        11.0,
        f64::INFINITY,
        f64::NAN,
    ] {
        let expected = before(value);
        let actual = equivalent(value);
        if expected.is_nan() {
            assert!(actual.is_nan());
        } else {
            assert_eq!(expected.to_bits(), actual.to_bits());
        }
    }
    assert!(before(f64::NAN).is_nan());
    assert!(!incorrect_smaller(f64::NAN).is_nan());
}
