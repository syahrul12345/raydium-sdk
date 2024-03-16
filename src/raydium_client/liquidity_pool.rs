use eyre::Result;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Deserialize, Serialize)]
pub struct LiqPoolInformation {
    pub official: Vec<LiquidityPool>,
    #[serde(rename = "unOfficial")]
    pub unofficial: Vec<LiquidityPool>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiquidityPool {
    #[serde(with = "pubkey")]
    pub id: Pubkey,
    #[serde(with = "pubkey")]
    pub base_mint: Pubkey,
    #[serde(with = "pubkey")]
    pub quote_mint: Pubkey,
    #[serde(with = "pubkey")]
    pub lp_mint: Pubkey,
    pub base_decimals: u8,
    pub quote_decimals: u8,
    pub lp_decimals: u8,
    pub version: u8,
    #[serde(with = "pubkey")]
    pub program_id: Pubkey,
    #[serde(with = "pubkey")]
    pub authority: Pubkey,
    #[serde(with = "pubkey")]
    pub open_orders: Pubkey,
    #[serde(with = "pubkey")]
    pub target_orders: Pubkey,
    #[serde(with = "pubkey")]
    pub base_vault: Pubkey,
    #[serde(with = "pubkey")]
    pub quote_vault: Pubkey,
    #[serde(with = "pubkey")]
    pub withdraw_queue: Pubkey,
    #[serde(with = "pubkey")]
    pub lp_vault: Pubkey,
    pub market_version: u8,
    #[serde(with = "pubkey")]
    pub market_program_id: Pubkey,
    #[serde(with = "pubkey")]
    pub market_id: Pubkey,
    #[serde(with = "pubkey")]
    pub market_authority: Pubkey,
    #[serde(with = "pubkey")]
    pub market_base_vault: Pubkey,
    #[serde(with = "pubkey")]
    pub market_quote_vault: Pubkey,
    #[serde(with = "pubkey")]
    pub market_bids: Pubkey,
    #[serde(with = "pubkey")]
    pub market_asks: Pubkey,
    #[serde(with = "pubkey")]
    pub market_event_queue: Pubkey,
}

pub fn get_pool_info(token_a: &Pubkey, token_b: &Pubkey) -> Result<Option<LiquidityPool>> {
    tracing::info!("fn: get_pool_info(token_a={},token_b={})", token_a, token_b,);
    let pools: LiqPoolInformation = serde_json::from_str(&std::fs::read_to_string("pools.json")?)?;
    let mut pools = pools
        .official
        .into_iter()
        .chain(pools.unofficial);

    match pools.find(|pool| pool.base_mint == *token_a && pool.quote_mint == *token_b) {
        Some(pool) => Ok(Some(pool)),
        None => Ok(pools.find(|pool| pool.base_mint == *token_b && pool.quote_mint == *token_a)),
    }
}

pub mod pubkey {
    use serde::{self, Deserialize, Deserializer, Serializer};
    pub use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;

    pub fn serialize<S>(pubkey: &Pubkey, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", pubkey);
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Pubkey, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Pubkey::from_str(&s).map_err(serde::de::Error::custom)
    }
}
