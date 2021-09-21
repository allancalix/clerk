use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::Path;

use anyhow::{anyhow, Result};
use ketos::{FromValue, Interpreter, Value};
use regex::Regex;
use rplaid::model::*;

#[derive(Debug, ForeignValue, FromValue, FromValueRef, StructValue, Clone)]
pub struct TransactionValue {
    pub source_account: String,
    pub dest_account: String,
    pub pending: bool,
    pub payee: String,
    pub amount: f64,
    pub date: String,
    pub plaid_id: String,
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
        })
    }

    pub fn apply(&self, xact: &Transaction) -> Result<TransactionValue> {
        let processor: String = match &xact.payment_meta {
            Some(meta) => match &meta.payment_processor {
                Some(processor) => processor.clone(),
                None => String::new(),
            },
            None => String::new(),
        };

        let tx = std::rc::Rc::new(TransactionValue {
            processor,
            payee: xact.name.clone(),
            source_account: xact.account_id.clone(),
            dest_account: "Expenses:Unknown".into(),
            amount: xact.amount,
            date: xact.date.clone(),
            pending: xact.pending,
            plaid_id: xact.transaction_id.clone(),
        });
        let out = self
            .interpreter
            .call("transform", vec![Value::Foreign(tx.clone())])
            .map_err(|e| anyhow!("{}", e))?;
        let v = TransactionValue::from_value(out).map_err(|e| anyhow!("{}", e))?;
        Ok(v)
    }
}
