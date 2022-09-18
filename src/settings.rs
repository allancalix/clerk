use config::{Config, Environment, File};
use rplaid::client;
use serde::Deserialize;

use crate::CLIENT_NAME;

const CONFIG_NAME: &str = "config.toml";

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub db_file: String,
    pub plaid: Plaid,
}

#[derive(Debug, Deserialize)]
pub struct Plaid {
    pub client_id: String,
    pub secret: String,
    pub env: client::Environment,
}

impl Settings {
    pub fn new(config_path: Option<&str>) -> Result<Self, config::ConfigError> {
        let mut s = Config::builder()
            .set_default("db_file", default_data_path())?
            .add_source(Environment::with_prefix("CLERK"));

        if let Some(path) = config_path {
            s = s.add_source(File::with_name(path));
        } else {
            s = s.add_source(File::with_name(&default_config_path()));
	}

        s.build()?.try_deserialize()
    }
}

fn default_data_path() -> String {
    dirs::data_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()))
        .join(CLIENT_NAME)
        .join(format!("{}.db", CLIENT_NAME))
        .display()
        .to_string()
}

pub(crate) fn default_config_path() -> String {
    dirs::config_dir()
        .unwrap_or_else(|| std::env::current_dir().expect("read current working dir"))
        .join(CLIENT_NAME)
        .join(CONFIG_NAME)
        .display()
        .to_string()
}
