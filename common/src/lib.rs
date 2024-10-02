use std::env;

pub mod config;
pub mod db;
pub mod metrics;

pub fn init_logger() {
    env::set_var(
        env_logger::DEFAULT_FILTER_ENV,
        env::var_os(env_logger::DEFAULT_FILTER_ENV)
            .unwrap_or_else(|| "warn,sqlx::query=warn".into()),
    );
    env_logger::init();
}
