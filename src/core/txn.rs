use std::collections::{HashMap, HashSet};

use serde::{Serialize, Deserialize}; 

type Bytes = Vec<u8>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Status {
    Resolved,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    status: Status,
    payee: Option<String>,
    narration: String,
    tags: HashSet<String>,
    links: HashSet<String>,
    meta: HashMap<String, Bytes>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Posting {
    account: Account,
    units: Amount,
    // TODO(allancalix): cost, price
    status: Status,
    meta: HashMap<String, Bytes>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Currency(String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Amount {
    value: i32,
    currency: Currency,
}
