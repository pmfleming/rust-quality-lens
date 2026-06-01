pub struct Config {
    pub value: i32,
}

impl Config {
    pub fn beta_method(&self) -> i32 {
        self.value
    }

    pub fn extra_beta_method(&self) -> i32 {
        self.value + 1
    }
}

pub fn beta_value() -> i32 {
    20
}
