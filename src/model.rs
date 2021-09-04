use serde::{Deserialize, Serialize};
use std::io::prelude::*;
use std::io::SeekFrom;

pub(crate) struct Config {
    handle: std::fs::File,
    state: AppState,
}

impl Config {
    pub(crate) fn from_path(path: &str) -> Self {
        let mut fd = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
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
