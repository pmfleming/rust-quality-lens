use super::domain::Config;

pub fn nested_dep(config: Config) -> i32 {
    config.value
}
