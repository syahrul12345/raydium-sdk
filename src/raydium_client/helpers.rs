use eyre::Result;
use solana_sdk::{instruction::Instruction, pubkey::Pubkey, signer::Signer};

use spl_token_client::{
    client::{SendTransaction, SimulateTransaction},
    token::{Token, TokenError},
};
use std::sync::Arc;

pub struct AtaCreationBundle {
    pub token_in: AtaInfo,
    pub token_out: AtaInfo,
}

pub struct AtaInfo {
    pub instruction: Option<Instruction>,
    pub ata_pubkey: Pubkey,
    pub balance: u64,
}

pub async fn get_or_create_ata_for_token_in_and_out_with_balance<
    S: Signer + 'static,
    PC: SendTransaction + SimulateTransaction + 'static,
>(
    token_in: &Token<PC>,
    token_out: &Token<PC>,
    payer: Arc<S>,
) -> Result<AtaCreationBundle> {
    let (create_token_in_ata_ix, token_in_ata, token_in_balance) =
        token_ata_creation_instruction(token_in, &payer).await?;
    let (create_token_out_ata_ix, token_out_ata, token_out_balance) =
        token_ata_creation_instruction(token_out, &payer).await?;

    Ok(AtaCreationBundle {
        token_in: AtaInfo {
            instruction: create_token_in_ata_ix,
            ata_pubkey: token_in_ata,
            balance: token_in_balance,
        },
        token_out: AtaInfo {
            instruction: create_token_out_ata_ix,
            ata_pubkey: token_out_ata,
            balance: token_out_balance,
        },
    })
}

async fn token_ata_creation_instruction<
    S: Signer + 'static,
    PC: SendTransaction + SimulateTransaction + 'static,
>(
    token: &Token<PC>,
    payer: &Arc<S>,
) -> Result<(Option<Instruction>, Pubkey, u64)> {
    let payer_token_account = token.get_associated_token_address(&payer.pubkey());
    let (instruction, amount) = match token.get_account_info(&payer_token_account).await {
        Ok(res) => (None, res.base.amount),
        Err(TokenError::AccountNotFound) | Err(TokenError::AccountInvalidOwner) => {
            tracing::info!("User does not have ATA {payer_token_account} for token. Creating");
            (
                Some(
                    spl_associated_token_account::instruction::create_associated_token_account(
                        &payer.pubkey(),
                        &payer.pubkey(),
                        token.get_address(),
                        &spl_token::ID,
                    ),
                ),
                0,
            )
        }
        Err(error) => {
            tracing::error!("Error retrieving user's input-tokens ATA: {}", error);
            return Err(eyre::ErrReport::msg(format!(
                "Error retrieving user's input-tokens ATA: {}",
                error
            )));
        }
    };
    Ok((instruction, payer_token_account, amount))
}
