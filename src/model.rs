use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::SeekFrom;

use anyhow::Result;

use crate::CLIENT_NAME;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct Conf {
    pub(crate) plaid_client_id: String,
    pub(crate) plaid_secret: String,
    pub(crate) plaid_env: String,
}

pub(crate) struct Config {
    handle: std::fs::File,
    state: AppState,
}

impl Config {
    pub(crate) fn new() -> Self {
        let data_path = dirs::data_dir()
            .unwrap()
            .join(CLIENT_NAME)
            .join("state.json");
        let mut fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(data_path)
            .unwrap();

        let mut content = String::new();
        fd.read_to_string(&mut content).unwrap();
        let state: AppState = serde_json::from_str(&content).unwrap_or_default();

        Self { handle: fd, state }
    }

    pub(crate) fn links(&self) -> Vec<Link> {
        self.state.links.clone()
    }

    pub(crate) fn add_link(&mut self, link: Link) {
        self.state.links.push(link);
        self.handle.seek(SeekFrom::Start(0)).unwrap();
        write!(
            self.handle,
            "{}",
            serde_json::to_string_pretty(&self.state).unwrap()
        )
        .unwrap();
        self.handle.flush().unwrap();
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AppState {
    links: Vec<Link>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct Link {
    pub(crate) access_token: String,
    pub(crate) item_id: String,
    pub(crate) state: String,
    pub(crate) env: rplaid::Environment,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub(crate) struct AppData {
    pub(crate) transactions: Vec<rplaid::Transaction>,
}

impl AppData {
    pub(crate) fn new() -> Result<AppData> {
        let data_path = dirs::data_dir()
            .unwrap()
            .join(CLIENT_NAME)
            .join("transactions.json");
        let mut fd = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(data_path)?;
        let mut content = String::new();
        fd.read_to_string(&mut content)?;
        let txns: Vec<rplaid::Transaction> = serde_json::from_str(&content).unwrap_or_default();

        Ok(Self { transactions: txns })
    }

    pub(crate) fn set_txns(&mut self, txns: Vec<rplaid::Transaction>) -> Result<()> {
        let data_path = dirs::data_dir()
            .unwrap()
            .join(CLIENT_NAME)
            .join("transactions.json");
        let mut fd = OpenOptions::new()
            .write(true)
            .create(true)
            .open(data_path)?;

        let json = serde_json::to_string_pretty(&txns)?;
        fd.seek(SeekFrom::Start(0))?;
        write!(fd, "{}", json)?;
        fd.flush()?;

        Ok(())
    }
}
