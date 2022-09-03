use std::collections::{HashMap, HashSet};

use chrono::naive::NaiveDate;
use ulid::Ulid;

type Bytes = Vec<u8>;

#[derive(Debug, Clone)]
pub enum Status {
    Resolved,
    Pending,
}

impl ToString for Status {
    fn to_string(&self) -> String {
        match self {
            Status::Resolved => "RESOLVED",
            Status::Pending => "PENDING",
        }
        .to_string()
    }
}

impl From<String> for Status {
    fn from(value: String) -> Status {
        match value.as_str() {
            "RESOLVED" => Status::Resolved,
            "PENDING" => Status::Pending,
            // TODO(allancalix): change to try_from instead?
            _ => unreachable!("unexpected status value"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: Ulid,
    pub status: Status,
    pub date: NaiveDate,
    pub payee: Option<String>,
    pub narration: String,
    pub tags: HashSet<String>,
    pub links: HashSet<String>,
    pub meta: HashMap<String, Bytes>,
}
