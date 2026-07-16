use crate::domain::Config;

impl crate::domain::Config {
    pub fn service_visible_value(&self) -> i32 {
        self.value
    }
}

pub mod local;

pub fn run_service() -> i32 {
    crate::domain::clippy_only(7)
}

pub fn takes_config(config: Config) -> i32 {
    config.value
}

pub fn dependency_forms(mode: crate::domain::Mode) -> i32 {
    let _config = crate::alpha::Config { value: 9 };
    match mode {
        crate::domain::Mode::Fast => crate::beta::beta_value(),
        crate::domain::Mode::Slow => 0,
    }
}

pub fn self_dependency() -> i32 {
    self::local::local_value()
}

pub fn external_dependency(input: &str) -> usize {
    let parsed = serde_json::from_str::<crate::domain::Config>(input).ok();
    tracing::debug!("external dependency path");
    parsed.map(|config| config.value as usize).unwrap_or_default()
}

pub fn macro_dependency() {
    crate::domain::audit!();
}
