// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Pot Reward Pallet
//!
//! - [`Config`]
//! - [`Call`]
//! - [`Pallet`]
//!
//! ## Overview
//!
//! The Pot Reward Pallet is a pallet that rewards validators
//! who are selected due to pot consensus.

use frame_support::{
	dispatch::DispatchResult,
	pallet_prelude::*,
	traits::{IsType, ValidatorSet},
	PalletId,
};
use frame_system::pallet_prelude::*;
pub use pallet::*;
use pallet_validator_election::{RewardInterface, SessionIndex};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AccountIdConversion, Convert, StaticLookup},
	types::{ParaId, SystemTokenId, VoteWeight},
};
use sp_std::prelude::*;

type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

/// A type for representing the validator id in a session.
pub type ValidatorId<T> = <<T as Config>::ValidatorSet as ValidatorSet<
	<T as frame_system::Config>::AccountId,
>>::ValidatorId;

#[derive(Encode, Decode, Clone, PartialEq, Eq, sp_core::RuntimeDebug, TypeInfo)]
pub struct ValidatorReward {
	pub system_token_id: SystemTokenId,
	pub amount: u128,
}

impl ValidatorReward {
	pub fn new(system_token_id: SystemTokenId, amount: u128) -> Self {
		Self { system_token_id, amount }
	}
}

#[frame_support::pallet]
pub mod pallet {
	use crate::{configuration, dmp, paras};

	use super::*;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	#[pallet::pallet]
	#[pallet::without_storage_info]
	#[pallet::generate_store(pub(crate) trait Store)]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ configuration::Config
		+ paras::Config
		+ dmp::Config
		+ pallet_assets::Config
	{
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// A type for retrieving the validators supposed to be online in a session.
		type ValidatorSet: ValidatorSet<Self::AccountId>;
	}

	#[pallet::storage]
	#[pallet::getter(fn validator_rewards)]
	#[pallet::unbounded]
	pub type ValidatorRewards<T: Config> =
		StorageMap<_, Twox64Concat, ValidatorId<T>, Vec<ValidatorReward>>;

	#[pallet::storage]
	#[pallet::getter(fn session_rewards)]
	#[pallet::unbounded]
	pub type TotalSessionRewards<T: Config> =
		StorageMap<_, Twox64Concat, SessionIndex, Vec<ValidatorReward>>;

	#[pallet::storage]
	#[pallet::getter(fn rewards_by_parachain)]
	#[pallet::unbounded]
	pub type RewardsByParaId<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		SessionIndex,
		Twox64Concat,
		ParaId,
		Vec<ValidatorReward>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The validator has been rewarded.
		ValidatorRewarded { stash: ValidatorId<T>, system_token_id: SystemTokenId, amount: u128 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not a controller account.
		NotController,
		/// Rewards already been claimed for this validator.
		AlreadyClaimed,
		EmptyAggregatedRewards,
		NothingToClaim,
		NeedOriginSignature,
		NoAssociatedValidatorId,
		ExceedsMaxMessageSize,
		Unknown,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		u32: PartialEq<<T as pallet_assets::Config>::AssetId>,
		<T as pallet_assets::Config>::Balance: From<u128>,
		<T as pallet_assets::Config>::AssetIdParameter: From<u32>,
	{
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn claim(
			origin: OriginFor<T>,
			validator: AccountIdLookupOf<T>,
			system_token_id: SystemTokenId,
		) -> DispatchResult {
			let origin = ensure_signed(origin)?;
			let validator = T::Lookup::lookup(validator.clone())?;

			ensure!(origin == validator, Error::<T>::NeedOriginSignature);

			let who = <T::ValidatorSet as ValidatorSet<T::AccountId>>::ValidatorIdOf::convert(
				validator.clone(),
			)
			.ok_or(Error::<T>::NoAssociatedValidatorId)?;
			ensure!(ValidatorRewards::<T>::contains_key(who.clone()), Error::<T>::NothingToClaim);
			let mut rewards: Vec<ValidatorReward> =
				ValidatorRewards::<T>::get(who.clone()).unwrap_or_default();
			ensure!(rewards.len() != 0, Error::<T>::NothingToClaim);

			let sovereign = Self::account_id();
			if let Some(reward) =
				rewards.iter_mut().find(|ar| ar.system_token_id == system_token_id)
			{
				let config = <configuration::Pallet<T>>::config();
				let xcm = {
					use parity_scale_codec::Encode as _;
					use xcm::opaque::{latest::prelude::*, VersionedXcm};

					let mut encoded: Vec<u8> = [system_token_id.clone().pallet_id as u8].into(); // asset pallet number
					let mut call_encode: Vec<u8> = pallet_assets::Call::<T>::force_transfer2 {
						id: system_token_id.clone().asset_id.into(),
						source: T::Lookup::unlookup(sovereign.clone()),
						dest: T::Lookup::unlookup(validator.clone()),
						amount: <T as pallet_assets::Config>::Balance::from(reward.amount),
					}
					.encode();

					encoded.append(&mut call_encode);

					let fee_multilocation =
						MultiAsset { id: Concrete(Here.into()), fun: Fungible(10000) };

					VersionedXcm::from(Xcm(vec![
						BuyExecution {
							fees: fee_multilocation.clone().into(),
							weight_limit: WeightLimit::Unlimited,
						},
						Transact {
							origin_kind: OriginKind::Superuser,
							require_weight_at_most: Weight::from_parts(10_000_000_000, 1_100_000),
							call: encoded.into(),
						},
					]))
					.encode()
				};
				if let Err(dmp::QueueDownwardMessageError::ExceedsMaxMessageSize) =
					<dmp::Pallet<T>>::queue_downward_message(
						&config,
						ParaId::from(system_token_id.clone().para_id).into(),
						xcm,
					) {
					log::error!(
						target: "runtime::infra_reward",
						"sending 'dmp' failed."
					);
				};
				Self::deposit_event(Event::ValidatorRewarded {
					stash: who.into(),
					system_token_id,
					amount: reward.amount,
				});
				reward.amount = 0;
			}

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	pub fn account_id() -> T::AccountId {
		let pallet_id = PalletId(*b"infrafee");
		pallet_id.into_account_truncating()
	}

	fn aggregate_reward(
		session_index: SessionIndex,
		para_id: ParaId,
		system_token_id: SystemTokenId,
		amount: VoteWeight,
	) {
		let amount: u128 = amount.into();

		if let Some(mut rewards) = RewardsByParaId::<T>::get(session_index, para_id.clone()) {
			for reward in rewards.iter_mut() {
				if reward.system_token_id == system_token_id {
					reward.amount += amount;
				}
			}
			RewardsByParaId::<T>::insert(session_index, para_id.clone(), rewards.clone());
		} else {
			let rewards = vec![ValidatorReward::new(system_token_id, amount)];
			RewardsByParaId::<T>::insert(session_index, para_id.clone(), rewards);
		}

		if let Some(mut rewards) = TotalSessionRewards::<T>::get(session_index) {
			for reward in rewards.iter_mut() {
				if reward.system_token_id == system_token_id {
					reward.amount += amount;
				}
			}
			TotalSessionRewards::<T>::insert(session_index, rewards.clone());
		} else {
			let rewards = vec![ValidatorReward::new(system_token_id, amount)];
			TotalSessionRewards::<T>::insert(session_index, rewards);
		}
	}

	fn distribute_reward(session_index: SessionIndex) {
		let current_validators = T::ValidatorSet::validators();
		let aggregated_rewards = TotalSessionRewards::<T>::get(session_index).unwrap_or_default();

		if aggregated_rewards.len() == 0 {
			return
		}

		for validator in current_validators.iter() {
			if ValidatorRewards::<T>::contains_key(validator) {
				let _ = ValidatorRewards::<T>::try_mutate_exists(
					validator,
					|maybe_rewards| -> Result<(), DispatchError> {
						let rewards = maybe_rewards.as_mut().ok_or(Error::<T>::Unknown)?;
						for reward in rewards.iter_mut() {
							if let Some(aggregated_reward) = aggregated_rewards
								.iter()
								.find(|ar| ar.system_token_id == reward.system_token_id)
							{
								let amount = (aggregated_reward.amount /
									current_validators.len() as u128) as u128;
								reward.amount += amount;
							}
						}
						Ok(())
					},
				);
			} else {
				let mut rewards: Vec<ValidatorReward> = vec![];
				for aggregated_reward in aggregated_rewards.iter() {
					let reward = ValidatorReward::new(
						aggregated_reward.clone().system_token_id,
						(aggregated_reward.clone().amount / current_validators.len() as u128)
							as u128,
					);
					rewards.push(reward);
				}
				ValidatorRewards::<T>::insert(validator, rewards.clone());
			}
		}
	}
}

impl<T: Config> RewardInterface for Pallet<T> {
	fn aggregate_reward(
		session_index: SessionIndex,
		para_id: ParaId,
		system_token_id: SystemTokenId,
		amount: VoteWeight,
	) {
		Self::aggregate_reward(session_index, para_id, system_token_id, amount);
	}

	fn distribute_reward(session_index: SessionIndex) {
		Self::distribute_reward(session_index);
	}
}
