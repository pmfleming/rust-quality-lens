pub mod alpha;
#[path = "attributed.rs"]
pub mod wired;
pub mod beta;
pub mod domain;
pub mod nested;
pub mod service;

macro_generated_modules! {
    mod macro_generated;
}

pub use crate::{
    domain::{Config, TupleConfig},
    service::run_service,
};

use self::domain::Mode;
use crate::{
    alpha::{Config as AlphaConfig, alpha_value},
    beta::{Config as BetaConfig, beta_value},
};

pub fn public_entry() -> i32 {
    crate::service::run_service() + alpha_value() + beta_value()
}

pub fn construct_configs() -> (AlphaConfig, BetaConfig, Config, TupleConfig, Mode) {
    (
        AlphaConfig { value: 1 },
        BetaConfig { value: 2 },
        Config { value: 3 },
        TupleConfig(4, 5),
        Mode::Fast,
    )
}
