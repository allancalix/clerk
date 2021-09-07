use std::fs::OpenOptions;
use std::io::prelude::*;

use anyhow::Result;
use pest::Parser;

use crate::CLIENT_NAME;

#[derive(Parser)]
#[grammar = "grammar/rules.pest"]

pub struct RulesParser;

pub struct Interpreter {
    transforms: Vec<Box<dyn std::ops::Fn(&mut rplaid::Transaction) -> ()>>,
}

impl Interpreter {
    pub fn from_rules_file() -> Result<Self> {
        let rules_file = dirs::config_dir()
            .unwrap()
            .join(CLIENT_NAME)
            .join("transform.rules");
        let mut fd = OpenOptions::new().read(true).open(rules_file)?;
        let mut content = String::new();
        fd.read_to_string(&mut content)?;

        Self::from_rules(&content)
    }

    pub fn from_rules(content: &str) -> Result<Self> {
        let mut int = Self { transforms: vec![] };
        let mut file = RulesParser::parse(Rule::rules, &content)?;
        for record in file.next().unwrap().into_inner() {
            match record.as_rule() {
                Rule::account_alias => {
                    let mut enclosed = record.into_inner();
                    let account_id = enclosed.next().unwrap().as_str().to_string().clone();
                    let account = enclosed.next().unwrap().as_str().to_string().clone();
                    int.transforms
                        .push(Box::new(move |txn: &mut rplaid::Transaction| {
                            if txn.account_id == account_id {
                                txn.account_id = account.clone();
                            };
                        }));
                }
                _ => (),
            }
        }

        Ok(int)
    }

    pub fn apply(&self, txn: &mut rplaid::Transaction) {
        for f in self.transforms.iter() {
            f(txn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACCOUNT_ALIAS: &str = r#"
account dafsdfkjasfjsafsafsdfkk5Rmg36EIAYjod6 Assets:Bank of America
account merz5mjsafsafsdfkDjsafsafsdfkjsafsafH Liabilities:Chase Freedom
account merz5mjsafsafsdfkDjsafsafsdfkjsafsafH Liabilities:Wells Fargo
"#;

    #[test]
    fn interpreter() {
        let rules = Interpreter::from_rules(ACCOUNT_ALIAS).unwrap();
        let mut txn = rplaid::Transaction {
            account_id: "dafsdfkjasfjsafsafsdfkk5Rmg36EIAYjod6".into(),
            account_owner: None,
            amount: 400.00f64,
            authorized_date: None,
            authorized_datetime: None,
            category: None,
            category_id: None,
            transaction_type: "hello".into(),
            pending_transaction_id: None,
            location: None,
            check_number: None,
            date: "2021-09-06".into(),
            payment_meta: None,
            name: "Payee".into(),
            datetime: None,
            iso_currency_code: None,
            original_description: None,
            unofficial_currency_code: None,
            pending: false,
            transaction_id: "1234".into(),
            merchant_name: None,
            payment_channel: "".into(),
            transaction_code: None,
        };

        rules.apply(&mut txn);
        assert_eq!(txn.account_id, "Assets:Bank of America");
    }
    #[test]
    fn it_works() {
        let mut file = RulesParser::parse(Rule::rules, ACCOUNT_ALIAS).unwrap();
        for record in file.next().unwrap().into_inner() {
            match record.as_rule() {
                Rule::account_alias => {
                    let mut enclosed = record.into_inner();
                    println!("{}", enclosed.next().unwrap().as_str());
                    println!("{}", enclosed.next().unwrap().as_str());
                }

                _ => println!("skip"),
            }
        }
    }

    #[test]
    fn can_parse_account_alias_def() {
        match RulesParser::parse(
            Rule::account_alias,
            r#"account zejzDgdfkjasfjsafsafsdfkk5Rmg36EIAYjod Assets:Chase Checking"#,
        ) {
            Ok(mut pairs) => {
                let mut enclosed = pairs.next().unwrap().into_inner();
                println!("{}", enclosed.next().unwrap().as_str());
                println!("{}", enclosed.next().unwrap().as_str());
            }
            Err(error) => {
                println!("{}", error);
                panic!()
            }
        }
    }
}
