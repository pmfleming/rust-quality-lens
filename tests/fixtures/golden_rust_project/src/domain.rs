pub struct Config {
    pub value: i32,
}

pub struct TupleConfig(pub i32, pub i32);

pub struct Marker;

pub enum Mode {
    Fast,
    Slow,
}

pub enum RichEnum {
    Unit,
    Tuple(i32, i32, i32),
    Struct { alpha: i32, beta: i32, gamma: i32 },
    Many1,
    Many2,
    Many3,
    Many4,
    Many5,
    Many6,
    Many7,
    Many8,
    Many9,
    Many10,
    Many11,
    Many12,
    Many13,
}

pub mod inline_a {
    pub struct Shared {
        pub a: i32,
    }

    impl Shared {
        pub fn one(&self) -> i32 {
            self.a
        }
    }
}

pub mod inline_b {
    pub struct Shared {
        pub b: i32,
    }

    impl Shared {
        pub fn one(&self) -> i32 {
            self.b
        }

        pub fn two(&self) -> i32 {
            self.b + 1
        }
    }
}

#[allow(clippy::needless_return)]
pub fn clippy_only(value: i32) -> i32 {
    return value;
}

#[test_case]
fn macro_case_test() {}

#[test_log::test]
fn namespaced_macro_test() {}

proptest! {
    #[test]
    fn generated_prop_test(value in 0..10) {
        let _ = value;
    }
}
