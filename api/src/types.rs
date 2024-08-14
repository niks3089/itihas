use chrono::NaiveDate;
use chrono::{DateTime, Utc};
use dao::generated::token_transfers;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

const FORMAT: &str = "%d/%m/%Y";

fn deserialize_date<'de, D>(deserializer: D) -> Result<NaiveDate, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    NaiveDate::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
}

#[derive(Deserialize)]
pub struct TransactionDateQuery {
    #[serde(deserialize_with = "deserialize_date")]
    pub day: NaiveDate,
}

#[derive(Deserialize)]
pub struct TransactionIdQuery {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transaction {
    pub signature: String,
    pub src_address: String,
    pub token_type: String,
    pub dest_address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src_ata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dest_ata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint_address: Option<String>,
    pub slot: i64,
    pub amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub block_time: DateTime<Utc>,
}

impl From<token_transfers::Model> for Transaction {
    fn from(model: token_transfers::Model) -> Self {
        Transaction {
            signature: bs58::encode(model.signature).into_string(),
            src_address: bs58::encode(model.src_address).into_string(),
            token_type: model.token_type,
            dest_address: bs58::encode(model.dest_address).into_string(),
            src_ata: model.src_ata.map(|ata| bs58::encode(ata).into_string()),
            dest_ata: model.dest_ata.map(|ata| bs58::encode(ata).into_string()),
            mint_address: model
                .mint_address
                .map(|mint| bs58::encode(mint).into_string()),
            slot: model.slot,
            amount: model.amount,
            error: model.error,
            block_time: model.block_time.into(),
        }
    }
}
