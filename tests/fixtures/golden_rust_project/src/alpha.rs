pub struct Config {
    pub value: i32,
}

impl Config {
    pub fn alpha_method(&self) -> i32 {
        self.value
    }
}

pub fn alpha_value() -> i32 {
    10
}
