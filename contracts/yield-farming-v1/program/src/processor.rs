//! Program state processor
//! In here, All instructions are processed by Processor

use {
    crate::{
        error::FarmError,
        instruction::{FarmInstruction},
        state::{FarmProgram,FarmPool,UserInfo},
        constant::*,
        utils::*
    },
    borsh::{BorshDeserialize, BorshSerialize},
    num_traits::FromPrimitive,
    solana_program::{
        account_info::{
            next_account_info,
            AccountInfo,
        },
        borsh::try_from_slice_unchecked,
        decode_error::DecodeError,
        entrypoint::ProgramResult,
        msg,
        program::{ invoke_signed},
        program_error::PrintProgramError,
        program_error::ProgramError,
        pubkey::Pubkey,
        clock::Clock,
        sysvar::Sysvar,
        program_pack::Pack,
    },
    spl_token::state::{Mint, Account, AccountState}, 
};
use std::str::FromStr;

// useful amm program's state
use cropper_liquidity_pool::amm_stats::{SwapVersion};

/// Program state handler.
/// Main logic of this program
pub struct Processor {}
impl Processor {
    /// All instructions start from here and are processed by their type.
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], input: &[u8]) -> ProgramResult {
        let instruction = FarmInstruction::try_from_slice(input)?;

        // determine instruction type
        match instruction {
            FarmInstruction::SetProgramData{
                super_owner,
                fee_owner,
                allowed_creator,
                amm_program_id,
                farm_fee,
                harvest_fee_numerator,
                harvest_fee_denominator,
            } => {
                // Instruction: Initialize
                Self::process_initialize_or_set_program(
                    program_id, 
                    accounts, 
                    &super_owner,
                    &fee_owner,
                    &allowed_creator,
                    &amm_program_id,
                    farm_fee,
                    harvest_fee_numerator,
                    harvest_fee_denominator)
            }
            FarmInstruction::InitializeFarm{
                nonce,
                start_timestamp,
                end_timestamp
            } => {
                // Instruction: Initialize
                Self::process_initialize_farm(program_id, accounts, nonce, start_timestamp, end_timestamp)
            }
            FarmInstruction::Deposit(amount) => {
                // Instruction: Deposit
                Self::process_deposit(program_id, accounts, amount)
            }
            FarmInstruction::Withdraw(amount) => {
                // Instruction: Withdraw
                Self::process_withdraw(program_id, accounts, amount)
            }
            FarmInstruction::AddReward(amount) => {
                // Instruction: AddReward
                Self::process_add_reward(program_id, accounts, amount)
            }
            FarmInstruction::PayFarmFee(amount) => {
                // Instruction: PayFarmFee
                Self::process_pay_farm_fee(program_id, accounts, amount)
            }
            FarmInstruction::RemoveRewards => {
                Self::process_remove_rewards(program_id, accounts)
            }
        }
    }
    pub fn process_remove_rewards(
        program_id: &Pubkey,        // this program id
        accounts: &[AccountInfo],   // all account informations
    ) -> ProgramResult {
        msg!("removing rewards ...");

        // get account informations
        let account_info_iter = &mut accounts.iter();

        // farm account information to add reward
        let farm_id_info = next_account_info(account_info_iter)?;

        // authority information of this farm account
        let authority_info = next_account_info(account_info_iter)?;

        // remover account information who will remove reward
        let remover_info = next_account_info(account_info_iter)?;

        // reward token account information in the remover's wallet
        let user_reward_token_account_info = next_account_info(account_info_iter)?;

        // reward token account information in the farm pool
        let pool_reward_token_account_info = next_account_info(account_info_iter)?;

        // farm program data account info
        let farm_program_info = next_account_info(account_info_iter)?;

        // spl-token program address
        let token_program_info = next_account_info(account_info_iter)?;

        if *farm_id_info.key != Pubkey::from_str(REMOVE_REWARDS_FARM_ADDRESS).map_err(|_| FarmError::InvalidPubkey)? {
            return Err(FarmError::InvalidSystemProgramId.into());
        }

        // check if given program account is correct
        Self::assert_program_account(program_id, farm_program_info.key)?;

        // check if given program data is initialized
        if Self::is_zero_account(farm_program_info) {
            return Err(FarmError::NotInitializedProgramData.into());
        }
        let program_data = try_from_slice_unchecked::<FarmProgram>(&farm_program_info.data.borrow())?;

        // borrow farm pool account data
        let mut farm_pool = try_from_slice_unchecked::<FarmPool>(&farm_id_info.data.borrow())?;

        if *remover_info.key != program_data.super_owner {
            return Err(FarmError::WrongManager.into());
        }

        //singers - check if depositor is signer
        if !remover_info.is_signer {
            return Err(FarmError::InvalidSigner.into());
        }

        // farm account - check if the given program address and farm account are correct
        if *authority_info.key != Self::authority_id(program_id, farm_id_info.key, farm_pool.nonce)? {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        // token account - check if owner is saved token program
        if  *user_reward_token_account_info.owner != farm_pool.token_program_id ||
            *pool_reward_token_account_info.owner != farm_pool.token_program_id {
                return Err(FarmError::InvalidOwner.into());
        }

        if  farm_pool.pool_reward_token_account != *pool_reward_token_account_info.key {
                return Err(FarmError::InvalidOwner.into());
        }

        let user_reward_token_data = Account::unpack_from_slice(&user_reward_token_account_info.data.borrow())?;
        let pool_reward_token_data = Account::unpack_from_slice(&pool_reward_token_account_info.data.borrow())?;

        if  user_reward_token_data.owner != *remover_info.key ||
            pool_reward_token_data.owner != *authority_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        if *token_program_info.key != farm_pool.token_program_id {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        // remove reward
        Self::token_transfer(
            farm_id_info.key,
            token_program_info.clone(),
            pool_reward_token_account_info.clone(),
            user_reward_token_account_info.clone(),
            authority_info.clone(),
            farm_pool.nonce,
            pool_reward_token_data.amount
        )?;

        farm_pool.remained_reward_amount -= pool_reward_token_data.amount;

        // store farm pool account data to network
        farm_pool
            .serialize(&mut *farm_id_info.data.borrow_mut())
            .map_err(|e| e.into())
    } 
    pub fn process_initialize_or_set_program(
        program_id: &Pubkey,        // this program id
        accounts: &[AccountInfo],   // all account informations
        super_owner: &Pubkey,
        fee_owner: &Pubkey,
        allowed_creator: &Pubkey,
        amm_program_id: &Pubkey,
        farm_fee: u64,
        harvest_fee_numerator: u64,
        harvest_fee_denominator: u64,
    ) -> ProgramResult {
        msg!("initializing program ...");

        // get all account informations from accounts array by using iterator
        let account_info_iter = &mut accounts.iter();
        let program_data_info = next_account_info(account_info_iter)?;
        let owner_info = next_account_info(account_info_iter)?;
        let rent_info = next_account_info(account_info_iter)?;
        let system_info = next_account_info(account_info_iter)?;

        // check if rent sysvar program id is correct
        if *rent_info.key != Pubkey::from_str(RENT_SYSVAR_ID).map_err(|_| FarmError::InvalidPubkey)? {
            return Err(FarmError::InvalidRentSysvarId.into());
        }

        // check if system program id is correct
        if *system_info.key != Pubkey::from_str(SYSTEM_PROGRAM_ID).map_err(|_| FarmError::InvalidPubkey)? {
            return Err(FarmError::InvalidSystemProgramId.into());
        }

        // check if super user is signer
        if !owner_info.is_signer {
            return Err(FarmError::SignatureMissing.into());
        }

        // check if given program data address is correct
        Self::assert_program_account(program_id, program_data_info.key)?;

        let seeds = [
            PREFIX.as_bytes(),
            program_id.as_ref(),
        ];

        let (_pda_key, bump) = Pubkey::find_program_address(&seeds, program_id);
        
        

        if program_data_info.data_is_empty() {
            let size = std::mem::size_of::<FarmProgram>();

            // Create account with enough space
            create_or_allocate_account_raw(
                *program_id,
                &program_data_info.clone(),
                &rent_info.clone(),
                &system_info.clone(),
                &owner_info.clone(),
                size,
                &[
                    PREFIX.as_bytes(),
                    program_id.as_ref(),
                    &[bump],
                ],
            )?;
        }

        let mut program_data = try_from_slice_unchecked::<FarmProgram>(&program_data_info.data.borrow())?;

        // if first initialization
        if Self::is_zero_account(program_data_info) {
            program_data.version = VERSION;
            program_data.reward_multipler = REWARD_MULTIPLER;
            program_data.super_owner = Pubkey::from_str(INITIAL_SUPER_OWNER).map_err(|_| FarmError::InvalidPubkey)?;
        }

        // check if given super user is saved super user
        if *owner_info.key != program_data.super_owner {
            return Err(FarmError::InvalidOwner.into());
        }

        // save given parameters
        program_data.super_owner = *super_owner;
        program_data.fee_owner = *fee_owner;
        program_data.allowed_creator = *allowed_creator;
        program_data.amm_program_id = *amm_program_id;
        program_data.farm_fee = farm_fee;
        program_data.harvest_fee_numerator = harvest_fee_numerator;
        program_data.harvest_fee_denominator = harvest_fee_denominator;

        // serialize/store this initialized data
        program_data
            .serialize(&mut *program_data_info.data.borrow_mut())
            .map_err(|e| e.into())
    } 

    /// process `Initialize` instruction.
    pub fn process_initialize_farm(
        program_id: &Pubkey,        // this program id
        accounts: &[AccountInfo],   // all account informations
        nonce: u8,                  // nonce for authorizing
        start_timestamp: u64,       // start time of this farm
        end_timestamp: u64,         // end time of this farm
    ) -> ProgramResult {
        msg!("initializing farm ...");
        // start initializeing this farm pool ...

        // get all account informations from accounts array by using iterator
        let account_info_iter = &mut accounts.iter();
        
        // farm pool account info to create newly
        let farm_id_info = next_account_info(account_info_iter)?;

        // authority of farm pool account
        let authority_info = next_account_info(account_info_iter)?;

        // creator wallet account information
        let creator_info = next_account_info(account_info_iter)?;

        // lp token account information to store lp token in the pool
        let pool_lp_token_account_info = next_account_info(account_info_iter)?;

        // reward token account information to store reward token in the pool
        let pool_reward_token_account_info = next_account_info(account_info_iter)?;

        // lp token's mint account information
        let pool_lp_mint_info = next_account_info(account_info_iter)?;

        // reward token's mint account information
        let reward_mint_info = next_account_info(account_info_iter)?;

        // amm account information what have lp token mint, token_a mint, token_b mint
        let amm_id_info = next_account_info(account_info_iter)?;

        // farm program data account info
        let farm_program_info = next_account_info(account_info_iter)?;

        // check if given program account is correct
        Self::assert_program_account(program_id, farm_program_info.key)?;

        // check if given program data is initialized
        if Self::is_zero_account(farm_program_info) {
            return Err(FarmError::NotInitializedProgramData.into());
        }

        let program_data = try_from_slice_unchecked::<FarmProgram>(&farm_program_info.data.borrow())?;

        // check if this farm account was created by this program with authority and nonce
        // if fail, returns InvalidProgramAddress error
        if *authority_info.key != Self::authority_id(program_id, farm_id_info.key, nonce)? {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        // check if farm creator is signer of this transaction
        // if not, returns SignatureMissing error
        if !creator_info.is_signer {
            return Err(FarmError::SignatureMissing.into());
        }

        // check if given farm was initialized already
        if !Self::is_zero_account(farm_id_info) {
            return Err(FarmError::AlreadyInUse.into());
        }

        // check if end time is later than start time
        if end_timestamp <= start_timestamp {
            return Err(FarmError::WrongPeriod.into());
        }

        let token_program_pubkey = Pubkey::from_str(TOKEN_PROGRAM_ID).map_err(|_| FarmError::InvalidPubkey)?;

        // token account - check if owner is saved token program
        if  *pool_lp_token_account_info.owner != token_program_pubkey ||
            *pool_reward_token_account_info.owner != token_program_pubkey {
                return Err(FarmError::InvalidOwner.into());
        }

        // borrow lp token mint account data
        let pool_mint = Mint::unpack_from_slice(&pool_lp_mint_info.data.borrow())?; 
        
        let pool_lp_token_data = Account::unpack_from_slice(&pool_lp_token_account_info.data.borrow())?;
        let pool_reward_token_data = Account::unpack_from_slice(&pool_reward_token_account_info.data.borrow())?;
        
        // token account - check if user token's owner is depositor
        if  pool_lp_token_data.owner != *authority_info.key ||
            pool_reward_token_data.owner != *authority_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if token mint is correct
        if  pool_lp_token_data.mint != *pool_lp_mint_info.key ||
            pool_reward_token_data.mint != *reward_mint_info.key {
            return Err(FarmError::WrongPoolMint.into()); 
        }

        if  pool_lp_token_data.delegate.is_some() ||
            pool_reward_token_data.delegate.is_some() {
            return Err(FarmError::InvalidDelegate.into());
        }
        if  pool_lp_token_data.state != AccountState::Initialized ||
            pool_reward_token_data.state != AccountState::Initialized {
            return Err(FarmError::NotInitialized.into());
        }
        if  pool_lp_token_data.close_authority.is_some() ||
            pool_reward_token_data.close_authority.is_some() {
            return Err(FarmError::InvalidCloseAuthority.into());
        }

        if pool_mint.freeze_authority.is_some() {
            return Err(FarmError::InvalidFreezeAuthority.into());
        }

        // borrow farm account data to initialize (mutable)
        let mut farm_pool = try_from_slice_unchecked::<FarmPool>(&farm_id_info.data.borrow())?;

        let amm_program_id = program_data.amm_program_id;

        // check if given amm id is for correct amm program id
        if *amm_id_info.owner != amm_program_id {
            return Err(FarmError::InvalidProgramAddress.into());
        }
        // borrow amm account data to check token's mint address with inputed one (immutable)
        let amm_swap = SwapVersion::unpack(&amm_id_info.data.borrow())?;
        
        // check if lp token mint address is same with amm pool's lp token mint address
        // if not, returns WrongPoolMint error
        if *amm_swap.pool_mint() != *pool_lp_mint_info.key {
            return Err(FarmError::WrongPoolMint.into());
        }

        // check if this creator can create "locked farms" specified by site owner
        if  Self::is_locked_farm(amm_swap.token_a_mint(), amm_swap.token_b_mint())?
        {
            // check if creator is allowed creator
            // if not returns WrongCreator error
            if *creator_info.key != program_data.allowed_creator {
                return Err(FarmError::WrongCreator.into());
            }
        }
        
        // Initialize farm account data
        // if not CRP token pairing,this farm is not allowed until creator pays farm fee
        farm_pool.set_allowed(Self::is_allowed(amm_swap.token_a_mint(), amm_swap.token_b_mint())?);

        // owner of this farm - creator
        farm_pool.owner = *creator_info.key;

        // initialize lp token account to store lp token
        farm_pool.pool_lp_token_account = *pool_lp_token_account_info.key;

        // initialize reward token account to store reward token
        farm_pool.pool_reward_token_account = *pool_reward_token_account_info.key;

        // store nonce to authorize this farm account
        farm_pool.nonce = nonce;

        // store lp token mint address
        farm_pool.pool_mint_address = *pool_lp_mint_info.key;

        // store spl-token program address
        farm_pool.token_program_id = token_program_pubkey;

        // store reward token mint address
        farm_pool.reward_mint_address = *reward_mint_info.key;

        // initialize total reward for unit lp so far
        farm_pool.reward_per_share_net = 0;

        // initialize lastest reward time
        farm_pool.last_timestamp = start_timestamp;

        // store reward per second
        farm_pool.remained_reward_amount = 0;

        // store start time of this farm
        farm_pool.start_timestamp = start_timestamp;

        // store end time of this farm
        farm_pool.end_timestamp = end_timestamp;
        
        // serialize/store this initialized farm again
        farm_pool
            .serialize(&mut *farm_id_info.data.borrow_mut())
            .map_err(|e| e.into())
    } 

    /// process deposit instruction
    /// this function performs stake lp token, harvest reward token
    pub fn process_deposit(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
    ) -> ProgramResult {
        msg!("depositing ...");
        // get account informations
        let account_info_iter = &mut accounts.iter();

        // farm account information to stake/harvest
        let farm_id_info = next_account_info(account_info_iter)?;

        // authority information of this farm account
        let authority_info = next_account_info(account_info_iter)?;

        // depositor's wallet account information
        let depositor_info = next_account_info(account_info_iter)?;

        // depositor's user account information to include deposited balance, reward debt
        let user_info_account_info = next_account_info(account_info_iter)?;

        // lp token account information in the depositor's wallet
        let user_lp_token_account_info = next_account_info(account_info_iter)?;

        // lp token account information in the farm pool
        let pool_lp_token_account_info = next_account_info(account_info_iter)?;

        // reward token account information in the depositor's wallet
        let user_reward_token_account_info = next_account_info(account_info_iter)?;

        // reward token account information in the farm pool
        let pool_reward_token_account_info = next_account_info(account_info_iter)?;

        // lp token mint account information in the farm pool
        let pool_lp_mint_info = next_account_info(account_info_iter)?;

        // fee owner wallet account information to collect fees such as harvest fee
        let reward_ata_info = next_account_info(account_info_iter)?;

        // farm program data account info
        let farm_program_info = next_account_info(account_info_iter)?;

        // spl-token program address
        let token_program_info = next_account_info(account_info_iter)?;

        // clock account information to use timestamp
        let clock_sysvar_info = next_account_info(account_info_iter)?;

        let rent_info = next_account_info(account_info_iter)?;
        let system_info = next_account_info(account_info_iter)?;

        msg!("validating ... ");

        // check if rent sysvar program id is correct
        if *rent_info.key != Pubkey::from_str(RENT_SYSVAR_ID).map_err(|_| FarmError::InvalidPubkey)? {
            return Err(FarmError::InvalidRentSysvarId.into());
        }

        // check if system program id is correct
        if *system_info.key != Pubkey::from_str(SYSTEM_PROGRAM_ID).map_err(|_| FarmError::InvalidPubkey)? {
            return Err(FarmError::InvalidSystemProgramId.into());
        }

        // check if clock sysvar program id is correct
        if *clock_sysvar_info.key != Pubkey::from_str(CLOCK_SYSVAR_ID).map_err(|_| FarmError::InvalidPubkey)? {
            return Err(FarmError::InvalidClockSysvarId.into());
        }

        // check if given program account is correct
        Self::assert_program_account(program_id, farm_program_info.key)?;

        // check if given program data is initialized
        if Self::is_zero_account(farm_program_info) {
            return Err(FarmError::NotInitializedProgramData.into());
        }

        msg!("getting data ... ");

        let program_data = try_from_slice_unchecked::<FarmProgram>(&farm_program_info.data.borrow())?;
        
        // get clock from clock sysvar account information
        let clock = &Clock::from_account_info(clock_sysvar_info)?;

        // get current timestamp(second)
        let cur_timestamp: u64 = clock.unix_timestamp as u64;

        // borrow farm pool account data
        let mut farm_pool = try_from_slice_unchecked::<FarmPool>(&farm_id_info.data.borrow())?;

        if user_info_account_info.data_is_empty() {
            msg!("creating user info account ... ");

            let seeds = [
                PREFIX.as_bytes(),
                farm_id_info.key.as_ref(),
                depositor_info.key.as_ref(),
            ];

            let (found_user_info_key, bump) = Pubkey::find_program_address(&seeds, program_id);

            if found_user_info_key != *user_info_account_info.key {
                return Err(FarmError::InvalidProgramAddress.into());
            }

            let size = std::mem::size_of::<UserInfo>();
            // Create account with enough space
            create_or_allocate_account_raw(
                *program_id,
                &user_info_account_info.clone(),
                &rent_info.clone(),
                &system_info.clone(),
                &depositor_info.clone(),
                size,
                &[
                    PREFIX.as_bytes(),
                    farm_id_info.key.as_ref(),
                    depositor_info.key.as_ref(),
                    &[bump],
                ],
            )?;
        }

        msg!("getting user data ... ");

        // borrow user info for this pool
        let mut user_info = try_from_slice_unchecked::<UserInfo>(&user_info_account_info.data.borrow())?;

        msg!("validating user & farm ... ");

        //singers - check if depositor is signer
        if !depositor_info.is_signer {
            return Err(FarmError::InvalidSigner.into());
        }

        // farm account - check if the given program address and farm account are correct
        if *authority_info.key != Self::authority_id(program_id, farm_id_info.key, farm_pool.nonce)? {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        // farm account - check if this farm was allowed already
        if !farm_pool.is_allowed() {
            return Err(FarmError::NotAllowed.into());
        }
        

        // farm account - This farm was not started yet
        if cur_timestamp < farm_pool.start_timestamp {
            return Err(FarmError::NotStarted.into());
        }

        // farm account - The period of this farm was ended
        if cur_timestamp > farm_pool.end_timestamp {
            return Err(FarmError::FarmEnded.into());
        }

        // user info account - check if user info account's owner is program id
        if user_info_account_info.owner != program_id {
            return Err(FarmError::InvalidOwner.into());
        }

        let is_user_info_zero_account = Self::is_zero_account(user_info_account_info);
        // user info account - check if this depositor is new user
        if is_user_info_zero_account {
            // save user's wallet address
            user_info.wallet = *depositor_info.key;

            // save user's farm account address
            user_info.farm_id = *farm_id_info.key;
        }

        // user info account - check if this is for given farm account
        if user_info.farm_id != *farm_id_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        // user info account - check if this user info is for depositor
        if user_info.wallet != *depositor_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if owner is saved token program
        if  *user_lp_token_account_info.owner != farm_pool.token_program_id ||
            *pool_lp_token_account_info.owner != farm_pool.token_program_id ||
            *user_reward_token_account_info.owner != farm_pool.token_program_id ||
            *pool_reward_token_account_info.owner != farm_pool.token_program_id {
                return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if pool lp token account & pool reward token account is for given farm account
        if  farm_pool.pool_lp_token_account != *pool_lp_token_account_info.key ||
            farm_pool.pool_reward_token_account != *pool_reward_token_account_info.key{
                return Err(FarmError::InvalidOwner.into());
        }

        msg!("getting token informations ... ");

        let user_lp_token_data = Account::unpack_from_slice(&user_lp_token_account_info.data.borrow())?;
        let pool_lp_token_data = Account::unpack_from_slice(&pool_lp_token_account_info.data.borrow())?;
        let user_reward_token_data = Account::unpack_from_slice(&user_reward_token_account_info.data.borrow())?;
        let pool_reward_token_data = Account::unpack_from_slice(&pool_reward_token_account_info.data.borrow())?;
        let reward_ata_data = Account::unpack_from_slice(&reward_ata_info.data.borrow())?;

        // farm account - check fee owner
        if program_data.fee_owner != reward_ata_data.owner {
            return Err(FarmError::InvalidFeeAccount.into());
        }

        // token account - check if user token's owner is depositor
        if  user_lp_token_data.owner != *depositor_info.key ||
            user_reward_token_data.owner != *depositor_info.key ||
            pool_lp_token_data.owner != *authority_info.key ||
            pool_reward_token_data.owner != *authority_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if user has enough token amount
        if user_lp_token_data.amount < amount {
            return Err(FarmError::NotEnoughBalance.into());
        }

        // pool mint - check if pool mint is current program's mint address
        if *pool_lp_mint_info.key != farm_pool.pool_mint_address {
            return Err(FarmError::WrongPoolMint.into());
        }

        // token program - check if given token program is correct
        if *token_program_info.key != farm_pool.token_program_id {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        msg!("updating pool ... ");

        //update this pool with up-to-date, distribute reward token 
        Self::update_pool(
            &mut farm_pool,
            cur_timestamp,
            pool_lp_token_data.amount,
            pool_reward_token_data.amount,
        )?;

        // harvest user's pending rewards
        if user_info.deposit_balance > 0 {
            msg!("harvesting ... ");
            Self::harvest(
                &farm_id_info.clone(), 
                &token_program_info.clone(), 
                &pool_reward_token_account_info.clone(), 
                &reward_ata_info.clone(), 
                &user_reward_token_account_info.clone(), 
                &authority_info.clone(), 
                &program_data, 
                &farm_pool, 
                &mut user_info
            )?;
        }

        // deposit (stake lp token)
        if amount > 0 {
            msg!("deposting token ... ");

            // transfer lp token amount from user's lp token account to pool's lp token pool
            Self::token_transfer(
                farm_id_info.key,
                token_program_info.clone(), 
                user_lp_token_account_info.clone(), 
                pool_lp_token_account_info.clone(), 
                depositor_info.clone(), 
                farm_pool.nonce, 
                amount
            )?;

            // update user's deposited balance
            user_info.deposit_balance += amount;
        }
        
        // update reward debt
        user_info.reward_debt = farm_pool.get_new_reward_debt(&user_info)?;

        // save user's new info to network
        user_info
            .serialize(&mut *user_info_account_info.data.borrow_mut())?;

        // save new farm account data to network
        farm_pool
            .serialize(&mut *farm_id_info.data.borrow_mut())
            .map_err(|e| e.into())
        
    }

    /// process withdraw
    pub fn process_withdraw(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
    ) -> ProgramResult {
        msg!("withdrawing ...");
        // get account informations
        let account_info_iter = &mut accounts.iter();

        // farm account information to unstake/harvest
        let farm_id_info = next_account_info(account_info_iter)?;

        // authority information of this farm account
        let authority_info = next_account_info(account_info_iter)?;

        // withdrawer's wallet account information
        let withdrawer_info = next_account_info(account_info_iter)?;

        // withdrawer's user account information to include deposited balance, reward debt
        let user_info_account_info = next_account_info(account_info_iter)?;

        // lp token account information in the withdrawer's wallet
        let user_lp_token_account_info = next_account_info(account_info_iter)?;

        // lp token account information in the farm pool
        let pool_lp_token_account_info = next_account_info(account_info_iter)?;

        // reward token account information in the withdrawer's wallet
        let user_reward_token_account_info = next_account_info(account_info_iter)?;

        // reward token account information in the farm pool
        let pool_reward_token_account_info = next_account_info(account_info_iter)?;

        // lp token mint account information in the farm pool
        let pool_lp_mint_info = next_account_info(account_info_iter)?;

        // fee owner wallet account information to collect fees such as harvest fee
        let reward_ata_info = next_account_info(account_info_iter)?;

        // farm program data account info
        let farm_program_info = next_account_info(account_info_iter)?;

        // spl-token program address
        let token_program_info = next_account_info(account_info_iter)?;

        // clock account information to use timestamp
        let clock_sysvar_info = next_account_info(account_info_iter)?;

        // check if clock sysvar program id is correct
        if *clock_sysvar_info.key != Pubkey::from_str(CLOCK_SYSVAR_ID).map_err(|_| FarmError::InvalidPubkey)? {
            return Err(FarmError::InvalidClockSysvarId.into());
        }

        // check if given program account is correct
        Self::assert_program_account(program_id, farm_program_info.key)?;

        // check if given program data is initialized
        if Self::is_zero_account(farm_program_info) {
            return Err(FarmError::NotInitializedProgramData.into());
        }

        let program_data = try_from_slice_unchecked::<FarmProgram>(&farm_program_info.data.borrow())?;

        // get clock from clock sysvar account information
        let clock = &Clock::from_account_info(clock_sysvar_info)?;

        // get current timestamp(second)
        let cur_timestamp: u64 = clock.unix_timestamp as u64;
        // borrow farm pool account data
        let mut farm_pool = try_from_slice_unchecked::<FarmPool>(&farm_id_info.data.borrow())?;

        if user_info_account_info.data_is_empty() {
            return Err(FarmError::InvalidProgramAddress.into());
        }
        // borrow user info for this pool
        let mut user_info = try_from_slice_unchecked::<UserInfo>(&user_info_account_info.data.borrow())?;

        //singers - check if depositor is signer
        if !withdrawer_info.is_signer {
            return Err(FarmError::InvalidSigner.into());
        }
        // farm account - check if the given program address and farm account are correct
        if *authority_info.key != Self::authority_id(program_id, farm_id_info.key, farm_pool.nonce)? {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        // farm account - check if this farm was allowed already
        if !farm_pool.is_allowed() {
            return Err(FarmError::NotAllowed.into());
        }

        // farm account - This farm was not started yet
        if cur_timestamp < farm_pool.start_timestamp {
            return Err(FarmError::NotStarted.into());
        }

        // user info account - check if user info account's owner is program id
        if user_info_account_info.owner != program_id {
            return Err(FarmError::InvalidOwner.into());
        }
        // user info account - check if this is for given farm account
        if user_info.farm_id != *farm_id_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        // user info account - check if this user info is for depositor
        if user_info.wallet != *withdrawer_info.key {
            return Err(FarmError::InvalidOwner.into());
        }
        // token account - check if owner is saved token program
        if  *user_lp_token_account_info.owner != farm_pool.token_program_id ||
            *pool_lp_token_account_info.owner != farm_pool.token_program_id ||
            *user_reward_token_account_info.owner != farm_pool.token_program_id ||
            *pool_reward_token_account_info.owner != farm_pool.token_program_id {
                return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if pool lp token account & pool reward token account is for given farm account
        if  farm_pool.pool_lp_token_account != *pool_lp_token_account_info.key ||
            farm_pool.pool_reward_token_account != *pool_reward_token_account_info.key{
                return Err(FarmError::InvalidOwner.into());
        }

        let user_lp_token_data = Account::unpack_from_slice(&user_lp_token_account_info.data.borrow())?;
        let pool_lp_token_data = Account::unpack_from_slice(&pool_lp_token_account_info.data.borrow())?;
        let user_reward_token_data = Account::unpack_from_slice(&user_reward_token_account_info.data.borrow())?;
        let pool_reward_token_data = Account::unpack_from_slice(&pool_reward_token_account_info.data.borrow())?;
        let reward_ata_data = Account::unpack_from_slice(&reward_ata_info.data.borrow())?;

        // farm account - check fee owner
        if program_data.fee_owner != reward_ata_data.owner {
            return Err(FarmError::InvalidFeeAccount.into());
        }
        
        // token account - check if user token's owner is depositor
        if  user_lp_token_data.owner != *withdrawer_info.key ||
            user_reward_token_data.owner != *withdrawer_info.key ||
            pool_lp_token_data.owner != *authority_info.key ||
            pool_reward_token_data.owner != *authority_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if user has enough token amount
        if pool_lp_token_data.amount < amount {
            return Err(FarmError::NotEnoughBalance.into());
        }

        if  *pool_reward_token_account_info.key != farm_pool.pool_reward_token_account ||
            *pool_lp_token_account_info.key != farm_pool.pool_lp_token_account {
            return Err(FarmError::InvalidTokenAccount.into());
        }

        // pool mint - check if pool mint is current program's mint address
        if *pool_lp_mint_info.key != farm_pool.pool_mint_address {
            return Err(FarmError::WrongPoolMint.into());
        }

        // token program - check if given token program is correct
        if *token_program_info.key != farm_pool.token_program_id {
            return Err(FarmError::InvalidProgramAddress.into());
        }
        
        // if amount > deposited balance, amount is deposited balance
        let mut _amount = amount;
        if user_info.deposit_balance < amount {
            _amount = user_info.deposit_balance;
        }

        // if deposited balance is zero, can't withdraw and returns ZeroDepositBalance error
        if user_info.deposit_balance == 0 {
            return Err(FarmError::ZeroDepositBalance.into());
        }

        //borrow pool lp token mint account data
        //let pool_mint = Mint::unpack_from_slice(&pool_lp_mint_info.data.borrow())?;

        //update this pool with up-to-date , distribute reward
        Self::update_pool(
            &mut farm_pool,
            cur_timestamp,
            pool_lp_token_data.amount,
            pool_reward_token_data.amount,
        )?;

        // harvest user's pending rewards
        if user_info.deposit_balance > 0 {
            Self::harvest(
                &farm_id_info.clone(), 
                &token_program_info.clone(), 
                &pool_reward_token_account_info.clone(), 
                &reward_ata_info.clone(), 
                &user_reward_token_account_info.clone(), 
                &authority_info.clone(), 
                &program_data, 
                &farm_pool, 
                &mut user_info
            )?;
            
        }

        // unstake lp token
        if _amount > 0 {
            Self::token_transfer(
                farm_id_info.key,
                token_program_info.clone(), 
                pool_lp_token_account_info.clone(),
                user_lp_token_account_info.clone(), 
                authority_info.clone(), 
                farm_pool.nonce, 
                _amount
            )?;
        }
        
        // store user's wallet address
        user_info.wallet = *withdrawer_info.key;

        // store farm account address
        user_info.farm_id = *farm_id_info.key;

        // update deposited balance
        user_info.deposit_balance -= _amount;

        // update reward debt
        user_info.reward_debt = farm_pool.get_new_reward_debt(&user_info)?;

        // store user's information to network
        user_info
            .serialize(&mut *user_info_account_info.data.borrow_mut())?;

        // store farm account data to network
        farm_pool
            .serialize(&mut *farm_id_info.data.borrow_mut())
            .map_err(|e| e.into())
        
    }
    /// farm creator can add reward token to his farm
    /// but can't remove once added
    pub fn process_add_reward(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
    ) -> ProgramResult {
        msg!("adding reward ...");
        // get account informations
        let account_info_iter = &mut accounts.iter();

        // farm account information to add reward
        let farm_id_info = next_account_info(account_info_iter)?;

        // authority information of this farm account
        let authority_info = next_account_info(account_info_iter)?;

        // creator account information who will add reward
        let creator_info = next_account_info(account_info_iter)?;

        // lp token account information in the creator's wallet
        let user_reward_token_account_info = next_account_info(account_info_iter)?;

        // reward token account information in the farm pool
        let pool_reward_token_account_info = next_account_info(account_info_iter)?;

        // lp token account information in the farm pool
        let pool_lp_token_account_info = next_account_info(account_info_iter)?;

        // lp token account information in the farm pool
        let pool_lp_mint_info = next_account_info(account_info_iter)?;

        // farm program data account info
        let farm_program_info = next_account_info(account_info_iter)?;

        // spl-token program address
        let token_program_info = next_account_info(account_info_iter)?;

        // clock account information to use timestamp
        let clock_sysvar_info = next_account_info(account_info_iter)?;

        // check if clock sysvar program id is correct
        if *clock_sysvar_info.key != Pubkey::from_str(CLOCK_SYSVAR_ID).map_err(|_| FarmError::InvalidPubkey)? {
            return Err(FarmError::InvalidClockSysvarId.into());
        }

        // check if given program account is correct
        Self::assert_program_account(program_id, farm_program_info.key)?;

        // check if given program data is initialized
        if Self::is_zero_account(farm_program_info) {
            return Err(FarmError::NotInitializedProgramData.into());
        }

        // get clock from clock sysvar account information
        let clock = &Clock::from_account_info(clock_sysvar_info)?;

        // get current timestamp(second)
        let cur_timestamp: u64 = clock.unix_timestamp as u64;

        
        // borrow farm pool account data
        let mut farm_pool = try_from_slice_unchecked::<FarmPool>(&farm_id_info.data.borrow())?;

        // check if given creator is farm owner
        // if not, returns WrongManager error
        if *creator_info.key != farm_pool.owner {
            return Err(FarmError::WrongManager.into());
        }

        //singers - check if depositor is signer
        if !creator_info.is_signer {
            return Err(FarmError::InvalidSigner.into());
        }

        // farm account - check if the given program address and farm account are correct
        if *authority_info.key != Self::authority_id(program_id, farm_id_info.key, farm_pool.nonce)? {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        // check if this farm ends
        if cur_timestamp > farm_pool.end_timestamp {
            return Err(FarmError::FarmEnded.into());
        }

        // token account - check if owner is saved token program
        if  *user_reward_token_account_info.owner != farm_pool.token_program_id ||
            *pool_reward_token_account_info.owner != farm_pool.token_program_id ||
            *pool_lp_token_account_info.owner != farm_pool.token_program_id {
                return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if pool lp token account & pool reward token account is for given farm account
        if  farm_pool.pool_reward_token_account != *pool_reward_token_account_info.key ||
            farm_pool.pool_lp_token_account != *pool_lp_token_account_info.key {
                return Err(FarmError::InvalidOwner.into());
        }

        let user_reward_token_data = Account::unpack_from_slice(&user_reward_token_account_info.data.borrow())?;
        let pool_reward_token_data = Account::unpack_from_slice(&pool_reward_token_account_info.data.borrow())?;
        let pool_lp_token_data = Account::unpack_from_slice(&pool_lp_token_account_info.data.borrow())?;

        // token account - check if user token's owner is depositor
        if  user_reward_token_data.owner != *creator_info.key ||
            pool_reward_token_data.owner != *authority_info.key || 
            pool_lp_token_data.owner != *authority_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if user has enough token amount
        if user_reward_token_data.amount < amount {
            return Err(FarmError::NotEnoughBalance.into());
        }

        // pool mint - check if pool mint is current program's mint address
        if *pool_lp_mint_info.key != farm_pool.pool_mint_address {
            return Err(FarmError::WrongPoolMint.into());
        }

        // token program - check if given token program is correct
        if *token_program_info.key != farm_pool.token_program_id {
            return Err(FarmError::InvalidProgramAddress.into());
        }


        // add reward
        if amount > 0 {

            //update this pool with up-to-date, distribute reward token 
            Self::update_pool(
                &mut farm_pool,
                cur_timestamp,
                pool_lp_token_data.amount,
                pool_reward_token_data.amount
            )?;

            // transfer reward token amount from user's reward token account to pool's reward token account
            Self::token_transfer(
                farm_id_info.key,
                token_program_info.clone(), 
                user_reward_token_account_info.clone(), 
                pool_reward_token_account_info.clone(), 
                creator_info.clone(), 
                farm_pool.nonce, 
                amount
            )?;

            farm_pool.remained_reward_amount += amount;
        }

        // store farm pool account data to network
        farm_pool
            .serialize(&mut *farm_id_info.data.borrow_mut())
            .map_err(|e| e.into())
        
    }
    /// process PayFarmFee instruction
    /// If this farm is not CRP token pairing , farm creator has to pay farm fee
    /// So this farm is allowed to stake/unstake/harvest
    pub fn process_pay_farm_fee(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        amount: u64,
    ) -> ProgramResult {
        msg!("paying farm fee ...");

        // get account informations
        let account_info_iter = &mut accounts.iter();

        // farm account information to pay farm fee
        let farm_id_info = next_account_info(account_info_iter)?;

        // authority information of this farm account
        let authority_info = next_account_info(account_info_iter)?;

        // creator account information who will add reward
        let creator_info = next_account_info(account_info_iter)?;

        // USDC token account in the creator's wallet to pay farm fee as USDC stable coin
        let user_usdc_token_account_info = next_account_info(account_info_iter)?;

        // fee owner wallet account to collect all fees
        let usdc_ata_info = next_account_info(account_info_iter)?;

        // farm program data account info
        let farm_program_info = next_account_info(account_info_iter)?;

        // spl-token program address
        let token_program_info = next_account_info(account_info_iter)?;

        // check if given program account is correct
        Self::assert_program_account(program_id, farm_program_info.key)?;

        // check if given program data is initialized
        if Self::is_zero_account(farm_program_info) {
            return Err(FarmError::NotInitializedProgramData.into());
        }

        let program_data = try_from_slice_unchecked::<FarmProgram>(&farm_program_info.data.borrow())?;

        // borrow farm pool account data
        let mut farm_pool = try_from_slice_unchecked::<FarmPool>(&farm_id_info.data.borrow())?;

        // check if given creator is owner of this farm
        // if not, returns WrongManager error
        if *creator_info.key != farm_pool.owner {
            return Err(FarmError::WrongManager.into());
        }

        // check if given program address and farm account address are correct
        // if not returns InvalidProgramAddress
        if *authority_info.key != Self::authority_id(program_id, farm_id_info.key, farm_pool.nonce)? {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        //singers - check if depositor is signer
        if !creator_info.is_signer {
            return Err(FarmError::InvalidSigner.into());
        }

        // token account - check if owner is saved token program
        if  *user_usdc_token_account_info.owner != farm_pool.token_program_id {
                return Err(FarmError::InvalidOwner.into());
        }

        let user_usdc_token_data = Account::unpack_from_slice(&user_usdc_token_account_info.data.borrow())?;
        let usdc_ata_data = Account::unpack_from_slice(&usdc_ata_info.data.borrow())?;

        // farm account - check fee owner
        if program_data.fee_owner != usdc_ata_data.owner {
            return Err(FarmError::InvalidFeeAccount.into());
        }

        // token account - check if user token's owner is depositor
        if  user_usdc_token_data.owner != *creator_info.key {
            return Err(FarmError::InvalidOwner.into());
        }

        // token account - check if user has enough token amount
        if user_usdc_token_data.amount < program_data.farm_fee {
            return Err(FarmError::NotEnoughBalance.into());
        }

        // token program - check if given token program is correct
        if *token_program_info.key != farm_pool.token_program_id {
            return Err(FarmError::InvalidProgramAddress.into());
        }

        // check if amount is same with FARM FEE
        // if not, returns InvalidFarmFee error
        if amount < program_data.farm_fee {
            return Err(FarmError::InvalidFarmFee.into());
        }

        // transfer fee amount from user's USDC token account to fee owner's account
        Self::token_transfer(
            farm_id_info.key,
            token_program_info.clone(), 
            user_usdc_token_account_info.clone(), 
            usdc_ata_info.clone(), 
            creator_info.clone(), 
            farm_pool.nonce, 
            amount
        )?;

        // allow this farm to stake/unstake/harvest
        farm_pool.set_allowed(1);

        // store farm account data to network
        farm_pool
            .serialize(&mut *farm_id_info.data.borrow_mut())
            .map_err(|e| e.into())
        
    }

    // update pool information with up-to-date, distribute reward token
    pub fn update_pool<'a>(
        farm_pool: &mut FarmPool, 
        cur_timestamp: u64, 
        lp_balance: u64, 
        reward_balance: u64, 
    ) -> Result<(), ProgramError>{
        // check if valid current timestamp
        if farm_pool.last_timestamp >= cur_timestamp {
            return Ok(());
        }

        if lp_balance == 0 {
            farm_pool.last_timestamp = cur_timestamp;
            return Ok(());
        }
        // update reward per share net and last distributed timestamp
        farm_pool.update_share(cur_timestamp, lp_balance, reward_balance)?;
        farm_pool.last_timestamp = cur_timestamp;
        Ok(())
    }
    pub fn harvest<'a>(
        farm_id_info: &AccountInfo<'a>,
        token_program_info: &AccountInfo<'a>,
        pool_reward_token_account_info: &AccountInfo<'a>,
        reward_ata_info: &AccountInfo<'a>,
        user_reward_token_account_info: &AccountInfo<'a>,
        authority_info: &AccountInfo<'a>,
        program_data:&FarmProgram,
        farm_pool:&FarmPool,
        user_info:&mut UserInfo
    )->Result<(), ProgramError>{
        // get pending amount
        let mut pending: u64 = farm_pool.pending_rewards(user_info)?;
        msg!("deposit={}", user_info.deposit_balance);
        msg!("reward_debt={}", user_info.reward_debt);
        msg!("pending={}", pending);

        let pool_reward_token_data = Account::unpack_from_slice(&pool_reward_token_account_info.data.borrow())?;

        if pool_reward_token_data.amount < pending {
            pending = pool_reward_token_data.amount;
        }
        
        // harvest
        if pending > 0 {
            // harvest fee
            let harvest_fee = farm_pool.get_harvest_fee(pending, &program_data)?;
            
            // transfer harvest fee to fee owner wallet
            Self::token_transfer(
                farm_id_info.key,
                token_program_info.clone(), 
                pool_reward_token_account_info.clone(), 
                reward_ata_info.clone(), 
                authority_info.clone(), 
                farm_pool.nonce, 
                harvest_fee
            )?;

            // real pending amount except fee
            let _pending = pending - harvest_fee;

            // transfer real pending amount from reward pool to user reward token account
            Self::token_transfer(
                farm_id_info.key,
                token_program_info.clone(), 
                pool_reward_token_account_info.clone(), 
                user_reward_token_account_info.clone(), 
                authority_info.clone(), 
                farm_pool.nonce, 
                _pending
            )?;

            user_info.reward_debt += pending;
        }

        Ok(())
    }
    /// get authority by given program address.
    pub fn authority_id(
        program_id: &Pubkey,
        my_info: &Pubkey,
        nonce: u8,
    ) -> Result<Pubkey, FarmError> {
        Pubkey::create_program_address(&[&my_info.to_bytes()[..32], &[nonce]], program_id)
            .or(Err(FarmError::InvalidProgramAddress))
    }
    pub fn is_zero_account(account_info:&AccountInfo)->bool{
        let account_data: &[u8] = &account_info.data.borrow();
        let len = account_data.len();
        let mut is_zero = true;
        for i in 0..len-1 {
            if account_data[i] != 0 {
                is_zero = false;
            }
        }
        is_zero
    }
    pub fn is_allowed(token_a_mint:&Pubkey, token_b_mint:&Pubkey)->Result<u8, ProgramError> {
        let mut is_allowed = 0;
        if  *token_a_mint == Pubkey::from_str(CRP_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)?  ||
            *token_b_mint == Pubkey::from_str(CRP_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)?  ||
            *token_a_mint == Pubkey::from_str(USDC_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)? || 
            *token_b_mint == Pubkey::from_str(USDC_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)? ||
            *token_a_mint == Pubkey::from_str(USDT_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)? || 
            *token_b_mint == Pubkey::from_str(USDT_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)?
        {
            is_allowed = 1;
        }
        Ok(is_allowed)
    }
    pub fn is_locked_farm(token_a_mint:&Pubkey, token_b_mint:&Pubkey)->Result<bool, ProgramError> {
        let mut result = false;
        let sol_mint = Pubkey::from_str(SOL_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)?;
        let eth_mint = Pubkey::from_str(ETH_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)?;
        let crp_mint = Pubkey::from_str(CRP_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)?;
        let usdc_mint = Pubkey::from_str(USDC_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)?;
        let usdt_mint = Pubkey::from_str(USDT_MINT_ADDRESS).map_err(|_| FarmError::InvalidPubkey)?;

        if  (*token_a_mint == sol_mint && *token_b_mint == usdc_mint) ||
            (*token_a_mint == usdc_mint && *token_b_mint == sol_mint) || //SOL-USDC
            (*token_a_mint == sol_mint && *token_b_mint == usdt_mint) ||
            (*token_a_mint == usdt_mint && *token_b_mint == sol_mint) || //SOL-USDT
            (*token_a_mint == eth_mint && *token_b_mint == usdc_mint) ||
            (*token_a_mint == usdc_mint && *token_b_mint == eth_mint) || //ETH-USDC
            (*token_a_mint == eth_mint && *token_b_mint == usdt_mint) ||
            (*token_a_mint == usdt_mint && *token_b_mint == eth_mint) || //ETH-USDT
            (*token_a_mint == usdc_mint && *token_b_mint == crp_mint) ||
            (*token_a_mint == crp_mint && *token_b_mint == usdc_mint) || //CRP-USDC
            (*token_a_mint == usdt_mint && *token_b_mint == crp_mint) ||
            (*token_a_mint == crp_mint && *token_b_mint == usdt_mint) || //CRP-USDT
            (*token_a_mint == sol_mint && *token_b_mint == crp_mint) ||
            (*token_a_mint == crp_mint && *token_b_mint == sol_mint) ||  //SOL-CRP
            (*token_a_mint == eth_mint && *token_b_mint == crp_mint) ||
            (*token_a_mint == crp_mint && *token_b_mint == eth_mint) ||  //ETH-CRP
            (*token_a_mint == eth_mint && *token_b_mint == sol_mint) || 
            (*token_a_mint == sol_mint && *token_b_mint == eth_mint)     //SOL-ETH
        {
            result = true
        }
        Ok(result)
    }

    /// issue a spl_token `Transfer` instruction.
    pub fn token_transfer<'a>(
        pool: &Pubkey,
        token_program: AccountInfo<'a>,
        source: AccountInfo<'a>,
        destination: AccountInfo<'a>,
        authority: AccountInfo<'a>,
        nonce: u8,
        amount: u64,
    ) -> Result<(), ProgramError> {
        let pool_bytes = pool.to_bytes();
        let authority_signature_seeds = [&pool_bytes[..32], &[nonce]];
        let signers = &[&authority_signature_seeds[..]];
        let ix = spl_token::instruction::transfer(
            token_program.key,
            source.key,
            destination.key,
            authority.key,
            &[],
            amount,
        )?;
        invoke_signed(
            &ix,
            &[source, destination, authority, token_program],
            signers,
        )
    } 
    pub fn assert_program_account(program_id:&Pubkey, key: &Pubkey)->Result<(), ProgramError>{
        let seeds = [
            PREFIX.as_bytes(),
            program_id.as_ref(),
        ];

        let (program_data_key, _bump) = Pubkey::find_program_address(&seeds, program_id);
        if program_data_key != *key {
            return Err(FarmError::InvalidProgramAddress.into());
        }
        else {
            Ok(())
        }
    }
    
}

/// implement all farm error messages
impl PrintProgramError for FarmError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        match self {
            FarmError::AlreadyInUse => msg!("Error: The account cannot be initialized because it is already being used"),
            FarmError::InvalidProgramAddress => msg!("Error: The program address provided doesn't match the value generated by the program"),
            FarmError::InvalidState => msg!("Error: The stake pool state is invalid"),
            FarmError::CalculationFailure => msg!("Error: The calculation failed"),
            FarmError::FeeTooHigh => msg!("Error: Stake pool fee > 1"),
            FarmError::WrongAccountMint => msg!("Error: Token account is associated with the wrong mint"),
            FarmError::WrongManager => msg!("Error: Wrong pool manager account"),
            FarmError::SignatureMissing => msg!("Error: Required signature is missing"),
            FarmError::InvalidValidatorStakeList => msg!("Error: Invalid validator stake list account"),
            FarmError::InvalidFeeAccount => msg!("Error: Invalid manager fee account"),
            FarmError::WrongPoolMint => msg!("Error: Specified pool mint account is wrong"),
            FarmError::NotStarted => msg!("Error: The farm has not started yet"),
            FarmError::FarmEnded => msg!("Error: The farm ended"),
            FarmError::ZeroDepositBalance => msg!("Error: Zero deposit balance"),
            FarmError::NotAllowed => msg!("Error: This farm is not allowed yet. The farm creator has to pay additional fee"),
            FarmError::InvalidFarmFee => msg!("Error: Wrong Farm Fee."),
            FarmError::WrongAmmId => msg!("Error: Wrong Amm Id"),
            FarmError::WrongFarmPool => msg!("Error: Wrong Farm pool"),
            FarmError::WrongCreator => msg!("Error: Not allowed to create the farm by this creator"),
            FarmError::WrongPeriod => msg!("Error: wrong start time and end time"),
            FarmError::InvalidOwner => msg!("Error: invalid owner"),
            FarmError::InvalidSigner => msg!("Error: invalid signer"),
            FarmError::NotEnoughBalance => msg!("Error: Not enough balance"),
            FarmError::InvalidTokenAccount => msg!("Error: Invalid token account"),
            FarmError::InvalidPubkey => msg!("Error: Invalid pubkey"),
            FarmError::PreciseError => msg!("Error: PreciseNumber error"),
            FarmError::NotInitializedProgramData => msg!("Error: Program data is not initialized yet"),
            FarmError::InvalidDelegate => msg!("Error: Token account has a delegate"),
            FarmError::InvalidCloseAuthority => msg!("Error: Token account has a close authority"),
            FarmError::InvalidFreezeAuthority => {
                msg!("Error: Pool token mint has a freeze authority")
            },
            FarmError::InvalidSupply => msg!("Error: Pool token mint has a non-zero supply"),
            FarmError::NotInitialized => msg!("Error: Not Initialized"),
            FarmError::InvalidSystemProgramId => msg!("Error: Invalid System Program Id"),
            FarmError::InvalidRentSysvarId => msg!("Error: Invalid Rent Sysvar Program Id"),
            FarmError::InvalidClockSysvarId => msg!("Error: Invalid Clock Sysvar Program Id"),
            
        }
    }
} 
