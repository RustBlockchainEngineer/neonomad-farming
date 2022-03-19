//! State transition types
//! State stores account data and manage version upgrade

#![allow(clippy::too_many_arguments)]
use {
    crate::{
        error::FarmError,
        constant::*,
    },
    borsh::{BorshDeserialize, BorshSchema, BorshSerialize},
    solana_program::{
        pubkey::{Pubkey},
        program_error::ProgramError,
        msg
    },
    spl_math::{precise_number::PreciseNumber},
    std::convert::TryFrom,
};


/// Farm Pool struct
#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct FarmProgram {
    /// program version
    pub version: u8,
    
    /// super owner of this program. this owner can change program state
    pub super_owner: Pubkey,

    /// fee owner wallet address
    pub fee_owner: Pubkey,

    /// allowed creator - This is allowed wallet address to create specified farms
    /// Specified farms are SOL-USDC, SOL-USDT, ETH-USDC, ETH-USDT, CRP-USDC, CRP-USDT, CRP-SOL, CRP-ETH
    pub allowed_creator: Pubkey,

    /// AMM program id
    pub amm_program_id: Pubkey,
    
    /// farm fee for not CRP token pairing farm
    pub farm_fee: u64,

    /// harvest fee numerator
    pub harvest_fee_numerator: u64,

    /// harvest fee denominator
    pub harvest_fee_denominator: u64,

    /// reward multipler
    pub reward_multipler: u64,
    
}


/// Farm Pool struct
#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct FarmPool {
    /// allowed flag for the additional fee to create farm
    pub is_allowed: u8,
    
    /// nonce is used to authorize this farm pool
    pub nonce: u8,

    /// This account stores lp token
    pub pool_lp_token_account: Pubkey,

    /// This account stores reward token
    pub pool_reward_token_account: Pubkey,

    /// lp token's mint address
    pub pool_mint_address: Pubkey,

    /// reward token's mint address
    pub reward_mint_address: Pubkey,

    /// spl-token program id
    pub token_program_id: Pubkey,
    
    /// owner wallet address of this farm
    pub owner: Pubkey,

    /// This represents the total reward amount what a farmer can receive for unit lp
    pub reward_per_share_net: u128,

    /// latest reward time
    pub last_timestamp: u64,

    /// reward per second
    pub remained_reward_amount: u64,

    /// start time of this farm
    pub start_timestamp: u64,

    /// end time of this farm
    pub end_timestamp: u64,

}
impl FarmPool {
    /// get current pending reward amount for a user
    pub fn pending_rewards(&self, user_info:&mut UserInfo) -> Result<u64, ProgramError> {
        
        msg!("pending_rewards() ...");
        let deposit_balance = PreciseNumber::new(user_info.deposit_balance as u128).ok_or(FarmError::PreciseError)?;
        let reward_per_share_net = PreciseNumber::new(self.reward_per_share_net as u128).ok_or(FarmError::PreciseError)?;
        let reward_multipler = PreciseNumber::new(REWARD_MULTIPLER as u128).ok_or(FarmError::PreciseError)?;
        if user_info.reward_debt < JUMP_DEBT {
            msg!("put JUMP_DEBT");
            user_info.reward_debt = JUMP_DEBT;
        }
        let _reward_debt = user_info.reward_debt - JUMP_DEBT;
        msg!("_reward_debt ...{}",_reward_debt);
        let reward_debt = PreciseNumber::new(_reward_debt as u128).ok_or(FarmError::PreciseError)?;
        msg!("2() ...");
        let mut result = deposit_balance.checked_mul(&reward_per_share_net).ok_or(FarmError::PreciseError)?
                    .checked_div(&reward_multipler).ok_or(FarmError::PreciseError)?;
                    msg!("reward_debt ...{}",reward_debt.to_imprecise().ok_or(FarmError::PreciseError)?);
                    msg!("result ...{}",result.to_imprecise().ok_or(FarmError::PreciseError)?);
        if reward_debt.to_imprecise().ok_or(FarmError::PreciseError)? > 0 {
            result = result.checked_sub(&reward_debt).ok_or(FarmError::PreciseError)?;
        }
       
        msg!("pending_rewards():deposit_balance = {}",deposit_balance.to_imprecise().ok_or(FarmError::PreciseError)?);
        msg!("pending_rewards():reward_per_share_net = {}",reward_per_share_net.to_imprecise().ok_or(FarmError::PreciseError)?);
        msg!("pending_rewards():reward_multipler = {}",reward_multipler.to_imprecise().ok_or(FarmError::PreciseError)?);
        msg!("pending_rewards():reward_debt = {}",reward_debt.to_imprecise().ok_or(FarmError::PreciseError)?);

        Ok(u64::try_from(result.to_imprecise().ok_or(FarmError::PreciseError)?).unwrap_or(0))
    }

    /// get total reward amount for a user so far
    pub fn get_new_reward_debt(&self, user_info:&UserInfo) -> Result<u64, ProgramError>{
        msg!("get_new_reward_debt() ...");
        let deposit_balance = PreciseNumber::new(user_info.deposit_balance as u128).ok_or(FarmError::PreciseError)?;
        let reward_per_share_net = PreciseNumber::new(self.reward_per_share_net as u128).ok_or(FarmError::PreciseError)?;
        let reward_multipler = PreciseNumber::new(REWARD_MULTIPLER as u128).ok_or(FarmError::PreciseError)?;

        let result = deposit_balance.checked_mul(&reward_per_share_net).ok_or(FarmError::PreciseError)?
                    .checked_div(&reward_multipler).ok_or(FarmError::PreciseError)?;
                    
        Ok(JUMP_DEBT + u64::try_from(result.to_imprecise().ok_or(FarmError::PreciseError)?).unwrap_or(0))
    }
    /// get harvest fee
    pub fn get_harvest_fee(&self, pending:u64, program_data:&FarmProgram) -> Result<u64, ProgramError>{
        msg!("get_harvest_fee() ...");
        let harvest_fee_numerator = PreciseNumber::new(program_data.harvest_fee_numerator as u128).ok_or(FarmError::PreciseError)?;
        let harvest_fee_denominator = PreciseNumber::new(program_data.harvest_fee_denominator as u128).ok_or(FarmError::PreciseError)?;
        let pending = PreciseNumber::new(pending as u128).ok_or(FarmError::PreciseError)?;

        let result = pending.checked_mul(&harvest_fee_numerator).ok_or(FarmError::PreciseError)?
                    .checked_div(&harvest_fee_denominator).ok_or(FarmError::PreciseError)?;
                    
        Ok(u64::try_from(result.to_imprecise().ok_or(FarmError::PreciseError)?).unwrap_or(0))
    }
    pub fn get_pool_version(&self)->u8 {
        self.is_allowed / 10     
    }
    pub fn set_pool_version(&mut self, ver: u8) {
        self.is_allowed = self.is_allowed % 10 + ver * 10;
    }
    pub fn is_allowed(&self)->bool{
        self.is_allowed % 10 > 0
    }
    pub fn set_allowed(&mut self, is_allowed: u8){
        self.is_allowed = (self.is_allowed / 10) * 10 + is_allowed;
    }
    pub fn update_share(&mut self, cur_timestamp:u64, _lp_balance:u64, _reward_balance:u64) -> Result<(), ProgramError>{
        msg!("update_share() ...");
        if self.get_pool_version() == 0 {
            msg!("converted pool version ...");
            self.remained_reward_amount = _reward_balance;
            self.reward_per_share_net = 0;
            self.last_timestamp = self.start_timestamp;
            self.set_pool_version(1)
        }

        msg!("cur_timestamp {}", cur_timestamp);
        msg!("_lp_balance {}", _lp_balance);
        msg!("remained_reward_amount {}", self.remained_reward_amount);

        let mut _calc_timestamp = cur_timestamp;
        if cur_timestamp > self.end_timestamp {
            _calc_timestamp = self.end_timestamp;
        }

        let remained_farm_duration = PreciseNumber::new((self.end_timestamp - self.last_timestamp) as u128).ok_or(FarmError::PreciseError)?;
        msg!("remained_farm_duration {}", remained_farm_duration.to_imprecise().ok_or(FarmError::PreciseError)?);
        let reward_balance = PreciseNumber::new(self.remained_reward_amount as u128).ok_or(FarmError::PreciseError)?;
        msg!("reward_balance {}", reward_balance.to_imprecise().ok_or(FarmError::PreciseError)?);
        let reward_per_timestamp = reward_balance
                                    .checked_div(&remained_farm_duration).ok_or(FarmError::PreciseError)?;
        msg!("reward_per_timestamp {}", reward_per_timestamp.to_imprecise().ok_or(FarmError::PreciseError)?);
        let duration = PreciseNumber::new((_calc_timestamp - self.last_timestamp) as u128).ok_or(FarmError::PreciseError)?;
        msg!("duration {}", duration.to_imprecise().ok_or(FarmError::PreciseError)?);
        let reward_multipler = PreciseNumber::new(REWARD_MULTIPLER as u128).ok_or(FarmError::PreciseError)?;
        msg!("reward_multipler {}", reward_multipler.to_imprecise().ok_or(FarmError::PreciseError)?);
        let reward_per_share_net = PreciseNumber::new(self.reward_per_share_net as u128).ok_or(FarmError::PreciseError)?;
        msg!("reward_per_share_net {}", reward_per_share_net.to_imprecise().ok_or(FarmError::PreciseError)?);
        let lp_balance = PreciseNumber::new(_lp_balance as u128).ok_or(FarmError::PreciseError)?;
        msg!("lp_balance {}", lp_balance.to_imprecise().ok_or(FarmError::PreciseError)?);

        let mut reward = duration.checked_mul(&reward_per_timestamp).ok_or(FarmError::PreciseError)?;
        if reward.to_imprecise().ok_or(FarmError::PreciseError)? > self.remained_reward_amount as u128 {
            reward = PreciseNumber::new(self.remained_reward_amount as u128).ok_or(FarmError::PreciseError)?;
        }
        
        self.remained_reward_amount -= u64::try_from(reward.to_imprecise().ok_or(FarmError::PreciseError)?).unwrap_or(0);

        msg!("reward {}", reward.to_imprecise().ok_or(FarmError::PreciseError)?);
        let updated_share = reward_multipler.checked_mul(&reward).ok_or(FarmError::PreciseError)?
                            .checked_div(&lp_balance).ok_or(FarmError::PreciseError)?
                            .checked_add(&reward_per_share_net).ok_or(FarmError::PreciseError)?;
        msg!("updated_share {}", updated_share.to_imprecise().ok_or(FarmError::PreciseError)?);
        self.reward_per_share_net = updated_share.to_imprecise().ok_or(FarmError::PreciseError)?;

        
        Ok(())
    }
}

/// User information struct
#[repr(C)]
#[derive(Clone, Debug, Default, PartialEq, BorshDeserialize, BorshSerialize, BorshSchema)]
pub struct UserInfo {
    
    /// user's wallet address
    pub wallet: Pubkey,

    /// farm account address what this user deposited
    pub farm_id: Pubkey,

    /// current deposited balance
    pub deposit_balance: u64,

    /// reward debt so far
    pub reward_debt: u64,
}
