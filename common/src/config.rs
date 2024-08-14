use figment::providers::Format;
use figment::providers::{Env, Json};
use figment::Figment;
use git2::Repository;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

fn get_relative_git_path(file_path: &str) -> PathBuf {
    let repo = Repository::discover(".").expect("Failed to discover Git repository");
    let git_root = repo.workdir().expect("Failed to get Git repository root");

    git_root.join(file_path)
}

fn get_local_config_file_path() -> PathBuf {
    let file_path = get_relative_git_path("common/src/local_config.json");
    if !Path::new(&file_path).exists() {
        panic!(
            "Configuration file does not exist: {}",
            file_path.to_string_lossy()
        );
    }
    file_path
}

pub fn load_config_using_env_prefix<T: DeserializeOwned>(env_prefix: &str) -> T {
    let mut config = Figment::new().join(Env::prefixed(env_prefix));
    if let Ok("local") = std::env::var("ENV").as_deref() {
        config = config.join(Json::file(get_local_config_file_path()));
    }
    config.extract::<T>().unwrap()
}
