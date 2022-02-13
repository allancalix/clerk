use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::PathBuf;

use anyhow::Result;
use rplaid::client::Environment;
use rplaid::model::*;

use crate::plaid::Link;
use crate::CLIENT_NAME;

const CONFIG_NAME: &str = "config.toml";
const DATA_FILE_NAME: &str = "state.json";

#[derive(Debug, Default, Clone)]
pub(crate) struct ConfigFile {
    path: PathBuf,
    conf: Conf,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct Conf {
    pub(crate) rules: Vec<String>,
    pub(crate) plaid: PlaidOpts,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub(crate) struct PlaidOpts {
    pub(crate) client_id: String,
    pub(crate) secret: String,
    pub(crate) env: Environment,
}

impl ConfigFile {
    pub(crate) fn default_config_path() -> Result<std::path::PathBuf> {
        Ok(dirs::config_dir()
                .unwrap_or(std::env::current_dir()?)
                .join(CLIENT_NAME)
                .join(CONFIG_NAME))
    }

    pub(crate) fn read(path: Option<&str>) -> Result<Self> {
        let p = match path {
            Some(p) => p.into(),
            None => ConfigFile::default_config_path()?,
        };

        let mut fd = OpenOptions::new().read(true).open(&p).map_err(|e| {
            match e.kind() {
                std::io::ErrorKind::NotFound => {
                    let ctx = format!("no configuration file found at: {:?}", p);
                    anyhow::Error::new(e).context(ctx)
                },
                _ => {
                    let ctx = format!("Failed to read configuration {}: {}.", p.display(), e);
                    anyhow::Error::new(e).context(ctx)
                },
            }
        })?;
        let mut content = String::new();
        fd.read_to_string(&mut content)?;

        let config: Conf = toml::from_str(&content)?;
        Ok(ConfigFile {
            path: p,
            conf: config,
        })
    }

    pub(crate) fn read_from_file(mut fd: std::fs::File, path: PathBuf) -> Result<Self> {
        let mut content = String::new();
        fd.read_to_string(&mut content)?;

        let config: Conf = toml::from_str(&content)?;
        Ok(ConfigFile {
            path,
            conf: config,
        })
    }

    pub(crate) fn update(&self, conf: &Conf) -> Result<()> {
        let mut fd = OpenOptions::new()
                .write(true)
                .open(&self.path)?;

        let contents = toml::to_string_pretty(conf)?;

        // Overwrite existing file contents.
        fd.set_len(0)?;
        write!(fd, "{}", contents)?;

        Ok(())
    }

    pub(crate) fn config(&self) -> &Conf {
        &self.conf
    }

    pub(crate) fn rules(&self) -> Vec<PathBuf> {
        let mut rules = vec![];
        let mut root = self.path.clone();
        root.pop();

        for rule in &self.config().rules {
            let mut path = PathBuf::from(rule);
            if path.is_relative() {
                path = PathBuf::from(root.clone()).join(rule)
            };

            rules.push(path);
        }

        rules
    }
}

pub(crate) struct AppData {
    handle: std::fs::File,
    store: AppStorage,
}

impl AppData {
    fn default_data_path() -> Result<std::path::PathBuf> {
        Ok(dirs::data_dir()
            .unwrap_or(std::env::current_dir()?)
            .join(CLIENT_NAME)
            .join(DATA_FILE_NAME))
    }
    pub(crate) fn new() -> Result<AppData> {
        let data_path = AppData::default_data_path()?;

        // Create default data directory if none exists.
        {
            let mut dir = data_path.clone();
            dir.pop();
            std::fs::create_dir_all(dir)?;
        }

        let mut fd = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(data_path)?;
        let mut content = String::new();
        fd.read_to_string(&mut content)?;
        let store: AppStorage = serde_json::from_str(&content).unwrap_or_default();

        Ok(Self { store, handle: fd })
    }

    pub(crate) fn set_txns(&mut self, txns: Vec<Transaction>) -> Result<()> {
        self.store.transactions = txns;
        let json = serde_json::to_string_pretty(&self.store)?;
        self.handle.seek(SeekFrom::Start(0))?;
        write!(self.handle, "{}", json)?;
        self.handle.flush()?;

        Ok(())
    }

    pub(crate) fn links(&self) -> Vec<Link> {
        self.store.links.clone()
    }

    pub(crate) fn update_links(&mut self, links: Vec<Link>) -> Result<()> {
        self.store.links = links;

        // Overwrite existing file contents.
        self.handle.set_len(0)?;
        write!(self.handle, "{}", serde_json::to_string_pretty(&self.store)?)?;
        self.handle.flush()?;
        Ok(())
    }

    pub(crate) fn links_by_env(&self, env: &Environment) -> Vec<Link> {
        self.store.links
            .clone()
            .into_iter()
            .filter(|link| link.env == *env)
            .collect()
    }

    pub(crate) fn add_link(&mut self, link: Link) -> Result<()> {
        match self.store.links.iter().position(|link| link.item_id == link.item_id) {
            Some(pos) => {
                self.store.links[pos] = link;
            },
            None => {
                self.store.links.push(link);
            },
        };
        self.handle.seek(SeekFrom::Start(0))?;
        write!(
            self.handle,
            "{}",
            serde_json::to_string_pretty(&self.store)?
        )?;
        self.handle.flush()?;

        Ok(())
    }

    pub(crate) fn transactions(&self) -> &Vec<Transaction> {
        &self.store.transactions
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AppStorage {
    links: Vec<Link>,
    transactions: Vec<Transaction>,
}
