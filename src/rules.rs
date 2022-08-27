use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;

use anyhow::{anyhow, Result};
use ketos::{FromValue, Interpreter, Value};
use regex::Regex;

use crate::core::{Status, Transaction};

#[derive(Debug, ForeignValue, FromValue, FromValueRef, StructValue, Clone)]
pub struct TransactionValue {
    pub source_account: String,
    pub dest_account: String,
    pub pending: bool,
    pub payee: String,
    pub amount: f64,
    pub date: String,
    pub processor: String,
}

#[derive(Debug, ForeignValue, FromValueRef, StructValue, Clone)]
pub struct Regexp {
    text: String,
    query: String,
}

fn contains(query: &Regexp) -> Result<bool, ketos::Error> {
    let re = Regex::new(&query.query).map_err(|e| ketos::Error::Custom(Box::new(e)))?;
    Ok(re.is_match(&query.text))
}

pub struct Transformer {
    interpreter: Interpreter,
    valid: bool,
}

impl Transformer {
    pub fn from_rules<T: AsRef<Path>>(rules: Vec<T>) -> Result<Self> {
        let interp = Interpreter::new();
        for rule in rules.iter() {
            let mut fd = OpenOptions::new()
                .read(true)
                .write(false)
                .create(false)
                .open(rule)?;
            let mut code = String::new();
            fd.read_to_string(&mut code)?;
            interp
                .run_code(&code, None)
                .map_err(|e| anyhow!("error parsing transform rules: {}", e))?;
        }
        interp.scope().register_struct_value::<TransactionValue>();
        interp.scope().register_struct_value::<Regexp>();
        ketos_fn! { interp.scope() => "contains" =>
        fn contains(query: &Regexp) -> bool }

        Ok(Self {
            interpreter: interp,
            valid: !rules.is_empty(),
        })
    }

    pub fn apply(&self, txn: &Transaction) -> Result<TransactionValue> {
        let source_posting = txn.postings.first().unwrap();
        let dest_posting = txn.postings.last().unwrap();
        let tx = TransactionValue {
            processor: "".to_string(),
            payee: txn.narration.clone(),
            date: txn.date.format("%Y-%m-%d").to_string(),
            source_account: source_posting.account.0.clone(),
            dest_account: dest_posting.account.0.clone(),
            amount: source_posting
                .units
                .amount()
                .to_string()
                .parse::<f64>()
                .unwrap(),
            pending: matches!(txn.status, Status::Pending),
        };

        if !self.valid {
            return Ok(tx);
        }

        let out = self
            .interpreter
            .call("transform", vec![Value::Foreign(std::rc::Rc::new(tx))])
            .map_err(|e| anyhow!("{}", e))?;
        let v = TransactionValue::from_value(out).map_err(|e| anyhow!("{}", e))?;
        Ok(v)
    }
}
