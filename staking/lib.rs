#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;
pub use staking::{Staking, StakingRef};

#[ink::contract]
#[cfg(not(feature = "ink-as-dependency"))]
mod staking {
    use sushibar::SushibarRef as Sushibar;
    
    use ink_storage::{traits::SpreadAllocate, Mapping};

    #[derive(Debug, PartialEq, Eq, scale::Encode, scale::Decode)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        /// Zero Liquidity
        ZeroLiquidity,
        /// Amount cannot be zero!
        ZeroAmount,
        /// Insufficient amount
        InsufficientAmount,
        /// Equivalent value of tokens not provided
        NonEquivalentValue,
        /// Asset value less than threshold for contribution!
        ThresholdNotReached,
        /// Share should be less than totalShare
        InvalidShare,
        /// Insufficient staking balance
        InsufficientLiquidity,
        /// Slippage tolerance exceeded
        SlippageExceeded,
        /// Returned if not enough balance to fulfill a request is available.
        InsufficientBalance,
        /// Returned if not enough allowance to fulfill a request is available.
        InsufficientAllowance,
    }

    /// Event emitted when a token transfer occurs.
    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        from: Option<AccountId>,
        #[ink(topic)]
        to: Option<AccountId>,
        value: u128,
    }

    /// Event emitted when an approval occurs that `spender` is allowed to withdraw
    /// up to the amount of `value` tokens from `owner`.
    #[ink(event)]
    pub struct Approval {
        #[ink(topic)]
        owner: AccountId,
        #[ink(topic)]
        spender: AccountId,
        value: u128,
    }

    #[ink(storage)]
    #[derive(SpreadAllocate)]
    pub struct Staking {
        sushi: AccountId,

        xsushi: AccountId,
        total_sushi: u128,
        total_xsushi: u128,
        total_shares: u128,
        staked_at: Mapping<AccountId, u64>,
        shares: Mapping<AccountId, u128>,
        allowances: Mapping<(AccountId, AccountId), u128>,
        fees: u16,
    }

    fn sushibar(addr: AccountId) -> Sushibar {
        ink_env::call::FromAccountId::from_account_id(addr)
    }

    type Result<T> = core::result::Result<T, Error>;

    impl Staking {
        #[ink(constructor)]
        pub fn new(sushi: AccountId, xsushi: AccountId, fees: u16) -> Self {
            ink_lang::utils::initialize_contract(|contract| {
                Self::new_init(contract, sushi, xsushi, fees)
            })
        }

        fn new_init(&mut self, sushi: AccountId, xsushi: AccountId, fees: u16) {
            self.sushi = sushi;
            self.xsushi = xsushi;
            self.total_sushi = 0;
            self.total_xsushi = 0;
            self.total_shares = 0;
            self.fees = fees;
        }
    }

    impl Staking {
        /// Returns amount of Sushi required when providing liquidity with _amount_xsushi quantity of Xsushi
        #[ink(message)]
        pub fn get_equivalent_sushi_estimate_given_xsushi(
            &self,
            _amount_xsushi: u128,
        ) -> Result<u128> {
            self.active_pool()?;
            Ok(self.total_sushi * _amount_xsushi / self.total_xsushi)
        }

        /// Returns amount of Xsushi required when providing liquidity with _amount_sushi quantity of Sushi
        #[ink(message)]
        pub fn get_equivalent_xsushi_estimate_given_sushi(
            &self,
            _amount_sushi: u128,
        ) -> Result<u128> {
            self.active_pool()?;
            Ok(self.total_xsushi * _amount_sushi / self.total_sushi)
        }

        /// Adding new liquidity in the staking
        /// Returns the amount of share issued for locking given assets
        #[ink(message)]
        pub fn enter(
            &mut self,
            _amount_sushi: u128,
            _amount_xsushi: u128,
        ) -> Result<u128> {
            let caller = self.env().caller();

            let share: u128;
            if self.total_shares == 0 {
                // Genesis liquidity is issued 100 Shares
                share = 100 * u128::pow(10, self.decimals() as u32);
            } else {
                let share1 = self.total_shares * _amount_sushi / self.total_sushi;
                let share2 = self.total_shares * _amount_xsushi / self.total_xsushi;

                if share1 != share2 {
                    return Err(Error::NonEquivalentValue);
                }
                share = share1;
            }

            if share == 0 {
                return Err(Error::ThresholdNotReached);
            }

            let locked_on = self.env().block_timestamp();
            self.staked_at.insert(&caller, &locked_on);

            let me = self.env().account_id();
            sushibar(self.sushi)
                .transfer_from(caller, me, _amount_sushi)
                .expect("Failed to receive token");

            sushibar(self.xsushi)
                .transfer_from(caller, me, _amount_xsushi)
                .expect("Failed to receive token");

            self.total_sushi += _amount_sushi;
            assert_eq!(sushibar(self.sushi).balance_of(me), self.total_sushi);

            self.total_xsushi += _amount_xsushi;
            assert_eq!(sushibar(self.xsushi).balance_of(me), self.total_xsushi);

            self.total_shares += share;

            let new_share = self.shares.get(caller).map(|v| v + share).unwrap_or(share);
            self.shares.insert(caller, &new_share);

            Ok(share)
        }
        /// Returns the estimate of Sushi & Xsushi that will be released on burning given _share
        #[ink(message)]
        pub fn get_withdraw_estimate(&self, _share: u128) -> Result<(u128, u128)> {
            self.active_pool()?;
            if _share > self.total_shares {
                return Err(Error::InvalidShare);
            }

            let amount_sushi = _share * self.total_sushi / self.total_shares;
            let amount_xsushi = _share * self.total_xsushi / self.total_shares;
            Ok((amount_sushi, amount_xsushi))
        }

        /// Removes liquidity from the staking and releases corresponding Sushi & Xsushi to the withdrawer
        

        
        
        #[ink(message)]
        pub fn leave(&mut self, _share: u128) -> Result<(u128, u128)> {
            let caller = self.env().caller();
            assert!(_share <= self.shares.get(caller).unwrap_or_default());

            let  (mut amount_sushi, mut amount_xsushi) = self.get_withdraw_estimate(_share)?;
            let new_share = self.shares.get(caller).unwrap() - _share;
            self.shares.insert(caller, &new_share);
            self.total_shares -= _share;

            self.total_sushi -= amount_sushi;
            self.total_xsushi -= amount_xsushi;

            let _staked_at = self.staked_at.get(&caller).unwrap();
            let total_staked_time = self.env().block_timestamp() - _staked_at;
            if total_staked_time < 172800 {
                amount_sushi = 0;
                amount_xsushi = 0;
            } else if total_staked_time >= 172800 && total_staked_time < 172800 * 2 {
                amount_sushi = amount_sushi / 4;
                amount_xsushi = amount_xsushi / 4;
            } else if total_staked_time >= 172800 * 2 && total_staked_time < 172800 * 3 {
                amount_sushi = amount_sushi / 2;
                amount_xsushi = amount_xsushi / 2;
            } else if total_staked_time >= 172800 * 3 && total_staked_time < 172800 * 4 {
                amount_sushi = amount_sushi * 3 / 4;
                amount_xsushi = amount_xsushi * 3 / 4;
            } 
            sushibar(self.sushi)
                .transfer(caller, amount_sushi)
                .expect("Failed to withdraw");
            sushibar(self.xsushi)
                .transfer(caller, amount_xsushi)
                .expect("Failed to withdraw");

            Ok((amount_sushi, amount_xsushi))
        }
        
        


        /// Returns the amount of Xsushi that the user should swap to get _amount_sushi in return
        #[ink(message)]
        pub fn get_swap_xsushi_estimate_given_sushi(&self, _amount_sushi: u128) -> Result<u128> {
            self.active_pool()?;
            if _amount_sushi >= self.total_sushi {
                return Err(Error::InsufficientLiquidity);
            }

            let sushi_after = self.total_sushi - _amount_sushi;
            let xsushi_after = self.get_k() / sushi_after;
            let amount_xsushi =
                (xsushi_after - self.total_xsushi) * 1000 / (1000 - self.fees) as u128;
            Ok(amount_xsushi)
        }

       

        /// Swaps given amount of Xsushi to Sushi using algorithmic price determination
        /// Swap fails if amount of Xsushi required to obtain _amount_sushi exceeds _max_xsushi
        #[ink(message)]
        pub fn swap_xsushi_given_sushi(
            &mut self,
            _amount_sushi: u128,
            _max_xsushi: u128,
        ) -> Result<u128> {
            let caller = self.env().caller();

            let amount_xsushi = self.get_swap_xsushi_estimate_given_sushi(_amount_sushi)?;
            if amount_xsushi > _max_xsushi {
                return Err(Error::SlippageExceeded);
            }

            let me = self.env().account_id();
            sushibar(self.xsushi)
                .transfer_from(caller, me, amount_xsushi)
                .expect("Failed to receive token");

            self.total_xsushi += amount_xsushi;
            assert_eq!(sushibar(self.xsushi).balance_of(me), self.total_xsushi);

            self.total_sushi -= _amount_sushi;
            sushibar(self.sushi)
                .transfer(caller, _amount_sushi)
                .expect("Failed to transfer token");
            Ok(amount_xsushi)
        }
    }

    impl Staking {
        #[ink(message)]
        pub fn decimals(&self) -> u8 {
            18
        }

        #[ink(message)]
        pub fn total_supply(&self) -> u128 {
            self.total_shares
        }

        #[ink(message)]
        pub fn balance_of(&self, owner: AccountId) -> u128 {
            self.shares.get(owner).unwrap_or_default()
        }

        #[ink(message)]
        pub fn allowance(&self, owner: AccountId, spender: AccountId) -> u128 {
            self.allowances.get((owner, spender)).unwrap_or_default()
        }

        #[ink(message)]
        pub fn transfer(&mut self, to: AccountId, value: u128) -> Result<()> {
            let from = self.env().caller();
            self.transfer_from_to(&from, &to, value)
        }

        #[ink(message)]
        pub fn approve(&mut self, spender: AccountId, value: u128) -> Result<()> {
            let owner = self.env().caller();
            self.allowances.insert((&owner, &spender), &value);

            // @bug: https://github.com/paritytech/ink/pull/1243
            // self.env().emit_event(Approval {
            //     owner,
            //     spender,
            //     value,
            // });
            Ok(())
        }

        #[ink(message)]
        pub fn transfer_from(&mut self, from: AccountId, to: AccountId, value: u128) -> Result<()> {
            let caller = self.env().caller();
            let allowance = self.allowance(from, caller);
            if allowance < value {
                return Err(Error::InsufficientAllowance);
            }
            self.transfer_from_to(&from, &to, value)?;
            self.allowances
                .insert((&from, &caller), &(allowance - value));
            Ok(())
        }

        fn transfer_from_to(
            &mut self,
            from: &AccountId,
            to: &AccountId,
            value: u128,
        ) -> Result<()> {
            let from_balance = self.balance_of(*from);
            if from_balance < value {
                return Err(Error::InsufficientBalance);
            }

            self.shares.insert(from, &(from_balance - value));
            let to_balance = self.balance_of(*to);
            self.shares.insert(to, &(to_balance + value));

            // @bug: https://github.com/paritytech/ink/pull/1243
            // self.env().emit_event(Transfer {
            //     from: Some(*from),
            //     to: Some(*to),
            //     value,
            // });

            Ok(())
        }
    }

    #[ink(impl)]
    impl Staking {
        // Returns the liquidity constant of the staking
        fn get_k(&self) -> u128 {
            self.total_sushi * self.total_xsushi
        }

        // Used to restrict withdraw & swap feature till liquidity is added to the staking
        fn active_pool(&self) -> Result<()> {
            match self.get_k() {
                0 => Err(Error::ZeroLiquidity),
                _ => Ok(()),
            }
        }
    }
}