mod helpers;
mod liquidity_pool;
use eyre::Result;
use raydium_contract_instructions::amm_instruction as amm;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Signature, signer::Signer,
    transaction::Transaction,
};
use spl_token::instruction;
use spl_token_client::{
    client::{ProgramClient, ProgramRpcClientSendTransaction},
    token::Token,
};
use std::sync::Arc;

use crate::raydium_client::liquidity_pool::get_pool_info;

use self::helpers::get_or_create_ata_for_token_in_and_out_with_balance;

const LAMPORTS_PER_SOLANA: u64 = 1000000000;

struct RaydiumCliemt<S, PC> {
    payer: Arc<S>,
    program_client: Arc<PC>,
}

impl<S: Signer + 'static, PC: ProgramClient<ProgramRpcClientSendTransaction> + 'static>
    RaydiumCliemt<S, PC>
{
    pub fn new(payer: Arc<S>, program_client: Arc<PC>) -> Self {
        Self {
            payer,
            program_client,
        }
    }
    pub async fn swap(
        &self,
        token_in: Pubkey,
        token_out: Pubkey,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<Signature> {
        let token_in = Token::new(
            self.program_client.clone(),
            &spl_token::ID,
            &token_in,
            None,
            self.payer.clone(),
        );
        let token_out = Token::new(
            self.program_client.clone(),
            &spl_token::ID,
            &token_out,
            None,
            self.payer.clone(),
        );
        let mut instructions: Vec<Instruction> = vec![];
        let ata_creation_bundle = get_or_create_ata_for_token_in_and_out_with_balance(
            &token_in,
            &token_out,
            self.payer.clone(),
        )
        .await
        .unwrap();

        {
            instructions.push(
                solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(25000),
            );
            instructions.push(
                solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
                    600000,
                ),
            );
        }
        //Create input ATAs if instruction exist
        if ata_creation_bundle.token_in.instruction.is_some() {
            println!(
                "Creating ata for token-in {:?}. ATA is {:?}",
                token_in.get_address(),
                ata_creation_bundle.token_in.ata_pubkey
            );
            instructions.push(ata_creation_bundle.token_in.instruction.unwrap())
        }
        if ata_creation_bundle.token_out.instruction.is_some() {
            println!(
                "Creating ata for token-out {:?} ATA is {:?}",
                token_out.get_address(),
                ata_creation_bundle.token_out.ata_pubkey
            );
            instructions.push(ata_creation_bundle.token_out.instruction.unwrap())
        }

        //Send some sol from account to the ata and then call sync native
        if token_in.is_native() && ata_creation_bundle.token_in.balance < amount_in {
            println!("Input token is native");
            let transfer_amount = amount_in - ata_creation_bundle.token_in.balance;
            let transfer_instruction = solana_sdk::system_instruction::transfer(
                &self.payer.pubkey(),
                &ata_creation_bundle.token_in.ata_pubkey,
                transfer_amount,
            );
            let sync_instruction = spl_token::instruction::sync_native(
                &spl_token::ID,
                &ata_creation_bundle.token_in.ata_pubkey,
            )?;
            instructions.push(transfer_instruction);
            instructions.push(sync_instruction);
        } else {
            //An SPL token is an input. If the ATA token address does not exist, it means that the balance is definately 0.
            if ata_creation_bundle.token_in.balance < amount_in {
                tracing::info!("Input token not native. Checking sufficient balance");
                return Err(eyre::ErrReport::msg(format!(
                    "Insufficient token_in balance. Have {:?} Required {:?}",
                    ata_creation_bundle.token_in.balance, amount_in
                )));
            }
        }

        let pool_info = get_pool_info(token_in.get_address(), token_out.get_address())?.ok_or(
            eyre::ErrReport::msg(format!(
                "No pool found for token_in {:?} and token_out {:?}",
                token_in.get_address(),
                token_out.get_address()
            )),
        )?;
        if pool_info.base_mint == *token_in.get_address() {
            tracing::info!("Initializing swap with input tokens as pool base token");
            let swap_instruction = amm::swap_base_in(
                &amm::ID,
                &pool_info.id,
                &pool_info.authority,
                &pool_info.open_orders,
                &pool_info.target_orders,
                &pool_info.base_vault,
                &pool_info.quote_vault,
                &pool_info.market_program_id,
                &pool_info.market_id,
                &pool_info.market_bids,
                &pool_info.market_asks,
                &pool_info.market_event_queue,
                &pool_info.market_base_vault,
                &pool_info.market_quote_vault,
                &pool_info.market_authority,
                &ata_creation_bundle.token_in.ata_pubkey,
                &ata_creation_bundle.token_out.ata_pubkey,
                &self.payer.pubkey(),
                amount_in,
                min_amount_out,
            )?;
            instructions.push(swap_instruction);
        } else {
            tracing::info!("Initializing swap with input tokens as pool quote token");
            let swap_instruction = amm::swap_base_out(
                &amm::ID,
                &pool_info.id,
                &pool_info.authority,
                &pool_info.open_orders,
                &pool_info.target_orders,
                &pool_info.base_vault,
                &pool_info.quote_vault,
                &pool_info.market_program_id,
                &pool_info.market_id,
                &pool_info.market_bids,
                &pool_info.market_asks,
                &pool_info.market_event_queue,
                &pool_info.market_base_vault,
                &pool_info.market_quote_vault,
                &pool_info.market_authority,
                &ata_creation_bundle.token_in.ata_pubkey,
                &ata_creation_bundle.token_out.ata_pubkey,
                &self.payer.pubkey(),
                amount_in,
                min_amount_out,
            )?;
            instructions.push(swap_instruction);
        }
        if token_out.is_native() {
            println!("Token out is native, closing account and claiming all wrapped sol");
            instructions.push(instruction::close_account(
                &spl_token::ID,
                &ata_creation_bundle.token_out.ata_pubkey,
                &self.payer.pubkey(),
                &self.payer.pubkey(),
                &[&self.payer.pubkey()],
            )?)
        }

        let recent_blockhash = self.program_client.get_latest_blockhash().await.unwrap();
        let transaction = Transaction::new_signed_with_payer(
            &instructions,
            Some(&self.payer.pubkey()),
            &vec![&self.payer],
            recent_blockhash,
        );
        let res = self.program_client.send_transaction(&transaction).await;
        match res {
            Ok(res) => match res {
                spl_token_client::client::RpcClientResponse::Signature(signature) => {
                    tracing::info!("Succesfully executed radium swap. Singature {}", signature);
                    return Ok(signature);
                }
                spl_token_client::client::RpcClientResponse::Transaction(_) => {
                    return Err(eyre::ErrReport::msg(
                        "Solana rpc client returned wrong response type. Received: Transaction",
                    ))
                }
                spl_token_client::client::RpcClientResponse::Simulation(_) => {
                    return Err(eyre::ErrReport::msg(
                        "Solana rpc client returned wrong response type. Received: Simulation",
                    ))
                }
            },
            Err(e) => {
                println!("{:?}", e);
                return Err(eyre::ErrReport::msg("Failed to send transaction"));
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use solana_client::nonblocking::rpc_client::RpcClient;
    use solana_sdk::{
        commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair,
        signer::EncodableKey,
    };
    use spl_token_client::client::{ProgramRpcClient, ProgramRpcClientSendTransaction};

    use crate::raydium_client::{RaydiumCliemt, LAMPORTS_PER_SOLANA};

    #[tokio::test]
    async fn test_usdc_sol_swap() {
        let payer =
            Keypair::read_from_file("/Users/muhdsyahrulnizam/.config/solana/id.json").unwrap();
        let client = Arc::new(RpcClient::new_with_commitment(
            "https://api.mainnet-beta.solana.com".to_string(),
            CommitmentConfig {
                commitment: solana_sdk::commitment_config::CommitmentLevel::Confirmed,
            },
        ));
        let program_client = Arc::new(ProgramRpcClient::new(
            client.clone(),
            ProgramRpcClientSendTransaction,
        ));

        let raydium_client = RaydiumCliemt::new(payer.into(), program_client);
        let hash = raydium_client
            .swap(
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                    .parse::<Pubkey>()
                    .unwrap(),
                "So11111111111111111111111111111111111111112"
                    .parse::<Pubkey>()
                    .unwrap(),
                18328898,
                0,
            )
            .await
            .unwrap();
        println!(
            "Succesfuly executed swap from usdc to solana. Signature {:?}",
            hash
        );
    }
    #[tokio::test]
    async fn test_sol_usdc_swap() {
        let payer =
            Keypair::read_from_file("/Users/muhdsyahrulnizam/.config/solana/id.json").unwrap();
        let client = Arc::new(RpcClient::new_with_commitment(
            "https://api.mainnet-beta.solana.com".to_string(),
            CommitmentConfig {
                commitment: solana_sdk::commitment_config::CommitmentLevel::Confirmed,
            },
        ));
        let program_client = Arc::new(ProgramRpcClient::new(
            client.clone(),
            ProgramRpcClientSendTransaction,
        ));

        let raydium_client = RaydiumCliemt::new(payer.into(), program_client);
        let hash = raydium_client
            .swap(
                "So11111111111111111111111111111111111111112"
                    .parse::<Pubkey>()
                    .unwrap(),
                "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                    .parse::<Pubkey>()
                    .unwrap(),
                LAMPORTS_PER_SOLANA / 100 * 10,
                0,
            )
            .await
            .unwrap();
        println!(
            "Succesfuly executed swap from solana to usdc. Signature {:?}",
            hash
        );
    }
}
