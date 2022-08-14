use anchor_lang::prelude::*;
use anchor_spl::token::{TokenAccount, Mint, Token, Transfer};

declare_id!("3KWHX1cnPbRP2hhoaTuNvkGTgr6mSNpBHdegMgkVAJTB");

#[program]
pub mod vesting_program {
    use anchor_spl::token::{self, Transfer};

    use super::*;

    // CREATE
    pub fn create(ctx: Context<Create>, release_interval: Vec<u64>, amount_interval: Vec<u64>, _seedphase: String) -> Result<()> {
        let vesting_contract = &mut ctx.accounts.vesting_contract;
        vesting_contract.dst_token_account = ctx.accounts.dst_token_account.key();
        vesting_contract.src_token_account = ctx.accounts.src_token_account.key();
        vesting_contract.mint_address = ctx.accounts.mint_address.key();
        
        // schedules
        require!(release_interval.len() == amount_interval.len(), ErrorCode::InvalidIntervalInput);

        let mut schedules: Vec<Schedule> = vec![];

        for i in 0..release_interval.len() {
            let schedule = Schedule {
                release_time: release_interval[i],
                amount: amount_interval[i]
            };
            schedules.push(schedule)
        }
        vesting_contract.schedules = schedules;

        // Transfer amount to escrow
        let total_amount: u64 = amount_interval.iter().sum();
        token::transfer(
            ctx.accounts.transfer_into_escrow(),
            total_amount
        )?;

        Ok(())
    }

    // UNLOCK
    pub fn unlock(ctx: Context<Unlock>, seedphrase: String) -> Result<()> {
        let mut total_amount_to_transfer: u64 = 0;

        for s in ctx.accounts.vesting_contract.schedules.iter_mut() {
            if ctx.accounts.clock.unix_timestamp as u64 > s.release_time {
                total_amount_to_transfer += s.amount;
                s.amount = 0;
            }
        }

        require!(total_amount_to_transfer > 0, ErrorCode::ZeroUnlockAmount);

        let (_key, bump) = Pubkey::find_program_address(&[
            seedphrase.as_bytes()
            ], ctx.program_id);

        let signer_seed = [
            seedphrase.as_bytes(),
            &[bump]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vesting_token_account.to_account_info().clone(),
            to: ctx.accounts.dst_token_account.to_account_info().clone(),
            authority: ctx.accounts.vesting_contract.to_account_info().clone(),
        };
        
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info().clone(), 
                cpi_accounts,
                &[&signer_seed[..]]
            ), 
            total_amount_to_transfer
        )?;
        
        Ok(())
    }

    // CHANGE_DESTINATION
    pub fn change_destination(ctx: Context<ChangeDestination>, _seedphrase: String) -> Result<()> {
        let vesting_contract = &mut ctx.accounts.vesting_contract;
        vesting_contract.dst_token_account = ctx.accounts.new_dst_token_account.key();
    
        Ok(())
    }

    // CLOSE
    pub fn close_account(ctx: Context<CloseAccount>, seedphrase: String) -> Result<()> {
        let mut amount_pass_unlock: u64 = 0;
        let mut total_amount_to_transfer: u64 = 0;

        for s in ctx.accounts.vesting_contract.schedules.iter_mut() {
            if ctx.accounts.clock.unix_timestamp as u64 > s.release_time {
                amount_pass_unlock += s.amount;
                s.amount = 0;
            } else {
                total_amount_to_transfer += s.amount
            }
        }

        require!(amount_pass_unlock == 0, ErrorCode::UnlockAmountFirst);
        
        // Transfer remaining amount to src
        let (_key, bump) = Pubkey::find_program_address(&[
            seedphrase.as_bytes()
            ], ctx.program_id);

        let signer_seed = [
            seedphrase.as_bytes(),
            &[bump]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vesting_token_account.to_account_info().clone(),
            to: ctx.accounts.src_token_account.to_account_info().clone(),
            authority: ctx.accounts.vesting_contract.to_account_info().clone(),
        };
        
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info().clone(), 
                cpi_accounts,
                &[&signer_seed[..]]
            ), 
            total_amount_to_transfer
        )?;
        
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(release_interval: Vec<u64>, amount_interval: Vec<u64>, seedphase: String )]
pub struct Create<'info> {
    // Initializer === employer (system account)
    #[account(mut)]
    pub initializer: Signer<'info>,
    // vesting_contract: this program account
    #[account(
        // init to initialize VestingContract
        init,
        // Read impli for detailed breakdown
        space = VestingContract::LEN() + Schedule::LEN() * release_interval.len(), 
        // seeds make sure vesting_contract is pointing to the correct account
        seeds = [seedphase.as_ref()],
        bump,
        payer = initializer
    )]
    pub vesting_contract: Account<'info, VestingContract>,
    // vesting_token_account: authority vesting_contract (PDA) 
    #[account(
        init,
        seeds = [mint_address.key().as_ref(), vesting_contract.key().as_ref()],
        bump,
        payer = initializer,
        // assign mint value
        token::mint = mint_address,
        // assign authority value
        token::authority = vesting_contract
    )]
    pub vesting_token_account: Account<'info, TokenAccount>,
    // Employer token account (Supplier)
    #[account(mut, token::authority = initializer.key())]
    pub src_token_account: Account<'info, TokenAccount>,
    // Employee token account (Receiver)
    #[account(token::mint = mint_address.key())]
    pub dst_token_account: Account<'info, TokenAccount>,
    pub mint_address: Account<'info, Mint>,
    // Required because rent is deducted from initializer
    pub system_program: Program<'info, System>,
    // Required because token is deducted from src_token_account => vesting_token_account
    pub token_program: Program<'info, Token>,
    // Required to calculate rent
    pub rent: Sysvar<'info, Rent>
}

impl<'info> Create<'info> {
    fn transfer_into_escrow(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.src_token_account.to_account_info().clone(),
            to: self.vesting_token_account.to_account_info().clone(),
            authority: self.initializer.to_account_info().clone(),
        };
        CpiContext::new(self.token_program.to_account_info().clone(), cpi_accounts)
    }
}

#[derive(Accounts)]
#[instruction(seedphrase: String)]
pub struct Unlock<'info> {
    // vesting_contract.schedule required
    // Constraint added to make sure correct accounts is pass in
    #[account(
        mut,
        seeds = [seedphrase.as_ref()], bump,
        constraint = vesting_contract.dst_token_account == dst_token_account.key(),
    )]
    pub vesting_contract: Account<'info, VestingContract>,
    // vesting_token_account required to transfer token out
    // Constraint added to make sure correct mint
    #[account(
        mut,
        seeds = [vesting_contract.mint_address.as_ref(), vesting_contract.key().as_ref()],
        bump,
        constraint = vesting_token_account.mint == vesting_contract.mint_address.key()
    )]
    pub vesting_token_account: Account<'info, TokenAccount>,
    // dst_token_account required to receive token
    #[account(mut)]
    pub dst_token_account: Box<Account<'info, TokenAccount>>,
    // clock used to check if vesting_contract.schedule has pass 
    pub clock: Sysvar<'info, Clock>,
    // Required because token is deducted from vesting_token_account => dst_token_account
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(seedphrase: String)]
pub struct ChangeDestination<'info> {
    // Required to update dst_token_account
    // Constraint added to make sure correct accounts is pass in
    #[account(
        mut,
        seeds = [seedphrase.as_ref()], bump, 
        constraint = vesting_contract.dst_token_account == current_dst_token_account.key(),
    )]
    pub vesting_contract: Account<'info, VestingContract>,
    // Current dst token owner MUST sign transaction
    pub current_dst_token_account_owner: Signer<'info>,
    // Check if the authority of current_dst_token_account is the signer
    #[account(token::authority = current_dst_token_account_owner.key())]
    pub current_dst_token_account: Box<Account<'info, TokenAccount>>,
    // Make sure new_dst_token_account has the correct mint
    #[account(token::mint = vesting_contract.mint_address)]
    pub new_dst_token_account: Account<'info, TokenAccount>,
}

#[derive(Accounts)]
#[instruction(seedphrase: String)]
pub struct CloseAccount<'info> {
    // Initializer === employer
    #[account(mut)]
    pub initializer: Signer<'info>,
    // Required to check src_token_account is correct
    // close = initializer ==> transfer rent back to initializer
    #[account(
        mut, seeds = [seedphrase.as_ref()], bump, 
        constraint = vesting_contract.src_token_account == src_token_account.key(),
        close = initializer
    )]
    pub vesting_contract: Account<'info, VestingContract>,
    // vesting_token_account required to transfer token out
    #[account(
        mut,
        seeds = [vesting_contract.mint_address.as_ref(), vesting_contract.key().as_ref()],
        bump,
    )]
    pub vesting_token_account: Account<'info, TokenAccount>,
    // Check if signer is really the employer
    #[account(mut, token::authority = initializer.key(), token::mint = vesting_contract.mint_address)]
    pub src_token_account: Box<Account<'info, TokenAccount>>,
    // Clock to make sure no claimable amount remain
    pub clock: Sysvar<'info, Clock>,
    // Required to transfer token back to src
    pub token_program: Program<'info, Token>,
}

// This is a normal struct
// Incoming data are serialize, deserialize required
// Required to re-serialize after 
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Schedule {
    pub release_time: u64,
    pub amount: u64
}

#[account]
pub struct VestingContract {
    pub dst_token_account: Pubkey,
    pub src_token_account: Pubkey,
    pub mint_address: Pubkey,
    pub schedules: Vec<Schedule>
}

const DISCRIMINATOR: usize = 8; // Required as prefix for ALL accounts
const PUBKEY: usize = 32;
const U64: usize = 8;
const VEC_PREFIX: usize = 4; // Required as prefix for vectors and strings

impl VestingContract {
    fn LEN() -> usize {
        DISCRIMINATOR + PUBKEY + PUBKEY + PUBKEY + VEC_PREFIX
    }
}

impl Schedule {
    fn LEN() -> usize {
        U64 + U64
    }
}

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid releaseInterval and amountInterval. Must be the same length.")]
    InvalidIntervalInput,
    #[msg("No outstanding unlockable balance.")]
    ZeroUnlockAmount,
    #[msg("There are outstanding unlockable balance. Please unlock balance first")]
    UnlockAmountFirst,
}