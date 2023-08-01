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

pub use frame_support::{
	pallet_prelude::{OptionQuery, *},
	traits::{ibs_support::system_token::SystemTokenInterface, UnixTime},
	PalletId,
};
pub use pallet::*;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AccountIdConversion, StaticLookup},
	types::{ParaId as IbsParaId, PalletId as IbsPalletId, SystemTokenId, VoteAssetId, VoteWeight},
	BoundedVec, RuntimeDebug,
};
use sp_std::prelude::*;

pub type ParaAssetId = VoteAssetId;
pub type RelayAssetId = VoteAssetId;
pub type PalletIndex = u32;

pub type SystemTokenWeight = u64;

/// Data structure for Wrapped system tokens
pub type WrappedSystemTokenId = SystemTokenId;
pub type StringLimitOf<T> = <T as Config>::StringLimit;


#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo)]
pub enum SystemTokenType {
	Original(SystemTokenId),
	Wrapped(WrappedSystemTokenId),
}

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default, TypeInfo)]
pub struct ParaCallMetadata {
	para_id: IbsParaId,
	pallet_id: IbsPalletId,
	pallet_name: Vec<u8>,
	call_name: Vec<u8>
}

const REF_WEIGHT: u64 = 500_000_000;
const PROOF_WEIGHT: u64 = 20_000;
/// The base_system_token_weight. Assume that it SHOULD not be changed.
const BASE_SYSTEM_TOKEN_WEIGHT: u128 = 100_000;

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default, TypeInfo, MaxEncodedLen)]

pub struct SystemTokenMetadata<BoundedString> {
	/// The user friendly name of issuer in real world
	pub(crate) issuer: BoundedString,
	/// The description of the token
	pub(crate) description: BoundedString,
	/// The url of related to the token or issuer
	pub(crate) url: BoundedString,
}

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default, TypeInfo, MaxEncodedLen)]
pub struct AssetMetadata<BoundedString, Balance> {
	/// The user friendly name of this system token.
	pub(crate) name: BoundedString,
	/// The exchange symbol for this system token.
	pub(crate) symbol: BoundedString,
	/// The number of decimals this asset uses to represent one unit.
	pub(crate) decimals: u8,
	/// The minimum balance of this new asset that any single account must
	/// have. If an account's balance is reduced below this, then it collapses to zero.
	pub(crate) min_balance: Balance,
}

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default, TypeInfo, MaxEncodedLen)]
pub struct SystemTokenProperty {
	/// The weight of this system token
	pub(crate) system_token_weight: SystemTokenWeight,
	/// The epoch time of this system token registered
	pub(crate) created_at: u128,
}

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default, TypeInfo, MaxEncodedLen)]
pub struct WrappedSystemTokenProperty {
	/// The epoch time of this system token registered
	pub(crate) created_at: u128,
}

pub enum AssetLinkCall {
	/// Register Asset Call on the AssetLink pallet
	LinkAsset,
	/// Deregister Asset Call on the AssetLink pallet
	UnlinkAsset,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use crate::{configuration, dmp, paras};
	use frame_system::pallet_prelude::*;
	use pallet_asset_link::AssetIdOf;

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ configuration::Config
		+ paras::Config
		+ dmp::Config
		+ pallet_assets::Config
		+ pallet_asset_link::Config
		+ pallet_system_token::Config
	{
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// Time used for computing registration date.
		type UnixTime: UnixTime;
		/// The string limit for name and symbol of system token.
		#[pallet::constant]
		type StringLimit: Get<u32>;
		/// Max number which can be used as system tokens on parachain.
		#[pallet::constant]
		type MaxWrappedSystemToken: Get<u32>;
		/// Max number which can be used as system tokens on parachain.
		#[pallet::constant]
		type MaxSystemTokenOnParachain: Get<u32>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Register a new system token.
		SystemTokenRegistered {
			system_token_id: SystemTokenId,
			system_token_metadata: SystemTokenMetadata<BoundedVec<u8, StringLimitOf<T>>>,
		},
		/// Deregister the system token.
		SystemTokenDeregistered {
			system_token_id: SystemTokenId,
			system_token_metadata: SystemTokenMetadata<BoundedVec<u8, StringLimitOf<T>>>,
		},
		/// Register a wrapped system token id to an original system token id.
		WrappedSystemTokenRegistered {
			wrapped_system_token_id: WrappedSystemTokenId,
			system_token_id: SystemTokenId,
		},
		/// Deregister a wrapped system token id to an original system token id.
		WrappedSystemTokenDeregistered { wsys_token_id: WrappedSystemTokenId },
		/// Convert a wrapped system token id to an original system token id.
		SystemTokenConverted {
			wrapped_system_token_id: WrappedSystemTokenId,
			system_token_id: SystemTokenId,
		},
		/// Update the weight for system token. The weight is calculated with exchange_rate and decimal.
		SetSystemTokenWeight { system_token_id: SystemTokenId, property: SystemTokenProperty },
		/// Update the fee rate of the parachain. The default value is 1_000(1).
		SetParaFeeRate { para_id: u32, para_fee_rate: u32 },
		/// Update the fee rate of the parachain. The default value is 1_000(1).
		SetFeeTable { para_call_metadata: ParaCallMetadata, fee: T::Balance },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Failed to register a system token as it is already registered.
		SystemTokenAlreadyRegistered,
		/// Failed to remove the system token as it is not registered.
		SystemTokenNotRegistered,
		/// Key for SystemTokenId is already existed
		SameSystemTokenAlreadyRegistered,
		/// Key for WrappedSystemTokenId, which is for parachain, is already existed
		WrappedSystemTokenAlreadyRegistered,
		/// WrappedSystemToken has not been set for Relay Chain
		WrappedSystemTokenNotRegistered,
		/// Registered System Tokens are out of limit
		TooManySystemTokensOnParachain,
		/// Reigstered WrappedSystemTokens are out of limit
		TooManyWrappedSystemTokens,
		/// Some TryMutate Error
		Unknown,
		/// String metadata is out of limit
		BadMetadata,
		/// Some of the value are not exist
		NotFound
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn system_token_list)]
	/// List for original system token and metadata.
	pub(super) type SystemTokenList<T: Config> = StorageMap<
		_,
		Twox64Concat,
		SystemTokenId,
		(
			SystemTokenMetadata<BoundedVec<u8, StringLimitOf<T>>>,
			AssetMetadata<BoundedVec<u8, StringLimitOf<T>>, T::Balance>,
		),
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn system_token_properties)]
	/// List for original system token and metadata.
	pub(super) type SystemTokenProperties<T: Config> =
		StorageMap<_, Twox64Concat, SystemTokenId, SystemTokenProperty, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn wrapped_system_token_properties)]
	/// List for wrapped system token and metadata.
	pub(super) type WrappedSystemTokenProperties<T: Config> =
		StorageMap<_, Twox64Concat, WrappedSystemTokenId, WrappedSystemTokenProperty, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn system_token_on_parachain)]
	/// Coverter between WrappedSystemTokenId and original SystemTokenId.
	pub(super) type SystemTokenOnParachain<T: Config> =
		StorageMap<_, Twox64Concat, WrappedSystemTokenId, SystemTokenId, OptionQuery>;

	#[pallet::storage]
	#[pallet::getter(fn system_token_on_parachain_by_para_id)]
	/// System token list for specific parachain by ParaId.
	pub(super) type SystemTokenOnParachainByParaId<T: Config> = StorageMap<
		_,
		Twox64Concat,
		IbsParaId,
		BoundedVec<WrappedSystemTokenId, T::MaxWrappedSystemToken>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn para_ids_by_system_token)]
	/// List of parachain ids using system token
	pub(super) type ParaIdsBySystemToken<T: Config> = StorageMap<
		_,
		Twox64Concat,
		SystemTokenId,
		BoundedVec<IbsParaId, T::MaxSystemTokenOnParachain>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn allowed_system_token)]
	/// Wrapped System token list used in parachains.
	/// Key: (PalletId, ParaAssetId) of Wrapped System token. ParaId is omitted.
	/// Value: (SystemTokenId)
	pub(super) type AllowedSystemToken<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		PalletIndex,
		Twox64Concat,
		ParaAssetId,
		SystemTokenId,
		OptionQuery,
	>;

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		u32: PartialEq<<T as pallet_assets::Config>::AssetId>,
		<T as pallet_assets::Config>::Balance: From<u128>,
		<T as pallet_assets::Config>::AssetIdParameter: From<u32>,
		AssetIdOf<T>: From<u32>,
	{
		/// Register a system token.
		#[pallet::call_index(0)]
		#[pallet::weight(1_000)]
		pub fn register_system_token(
			origin: OriginFor<T>,
			system_token_id: SystemTokenId,
			issuer: Vec<u8>,
			description: Vec<u8>,
			url: Vec<u8>,
			relay_asset_pallet_id: u8,
			relay_wrapped_system_token_asset_id: u32,
			name: Vec<u8>,
			symbol: Vec<u8>,
			decimals: u8,
			min_balance: T::Balance,
			system_token_weight: SystemTokenWeight,
		) -> DispatchResult {
			ensure_root(origin.clone())?;

			let original_para_id = system_token_id.clone().para_id;

			// ensure for the validity check
			{
				ensure!(
					!SystemTokenList::<T>::contains_key(&system_token_id),
					Error::<T>::SystemTokenAlreadyRegistered
				);

				let system_tokens = SystemTokenOnParachainByParaId::<T>::get(&original_para_id)
					.map_or(Default::default(), |system_token| system_token);
				ensure!(
					system_tokens.len() < T::MaxWrappedSystemToken::get() as usize,
					Error::<T>::TooManySystemTokensOnParachain
				);
			}

			let (system_token_metadata, asset_metadata) = {
				let s_m = {
					let issuer: BoundedVec<u8, StringLimitOf<T>> =
						issuer.clone().try_into().map_err(|_| Error::<T>::BadMetadata)?;
					let description: BoundedVec<u8, StringLimitOf<T>> =
						description.clone().try_into().map_err(|_| Error::<T>::BadMetadata)?;
					let url: BoundedVec<u8, StringLimitOf<T>> =
						url.clone().try_into().map_err(|_| Error::<T>::BadMetadata)?;

					SystemTokenMetadata { issuer, description, url }
				};

				let a_m = {
					let name: BoundedVec<u8, StringLimitOf<T>> =
						name.clone().try_into().map_err(|_| Error::<T>::BadMetadata)?;
					let symbol: BoundedVec<u8, StringLimitOf<T>> =
						symbol.clone().try_into().map_err(|_| Error::<T>::BadMetadata)?;

					AssetMetadata { name, symbol, decimals, min_balance }
				};

				(s_m, a_m)
			};

			// Create wrapped token in relay chain
			let relay_wrapped_system_token_id = SystemTokenId {
				para_id: 0,
				pallet_id: relay_asset_pallet_id,
				asset_id: relay_wrapped_system_token_asset_id,
			};

			let root = PalletId(*b"infra/rt");
			let owner: T::AccountId = root.into_account_truncating();

			pallet_assets::pallet::Pallet::<T>::force_create_with_metadata(
				origin.clone(),
				relay_wrapped_system_token_asset_id.into(),
				T::Lookup::unlookup(owner.clone()),
				true,
				min_balance,
				name,
				symbol,
				decimals,
				false,
				system_token_id,
				0, // parents for the multilocation of AssetLink
				system_token_weight,
			)?;

			// Storage insert logics
			{
				let now_as_millis_u128 = T::UnixTime::now().as_millis();
				let property =
					SystemTokenProperty { system_token_weight, created_at: now_as_millis_u128 };

				SystemTokenProperties::<T>::insert(&system_token_id, &property);

				SystemTokenOnParachain::<T>::insert(&system_token_id, &system_token_id);
				SystemTokenOnParachain::<T>::insert(
					&relay_wrapped_system_token_id,
					&system_token_id,
				);

				SystemTokenList::<T>::insert(
					&system_token_id,
					(&system_token_metadata, &asset_metadata),
				);

				Self::insert_system_token_on_parachain_by_para_id(
					original_para_id,
					system_token_id.clone(),
				);
				Self::insert_para_ids_by_system_token(system_token_id.clone(), original_para_id);

				// relay chain wrapped system token state
				let wrapped_system_token_property =
					WrappedSystemTokenProperty { created_at: now_as_millis_u128 };

				Self::insert_system_token_on_parachain_by_para_id(
					relay_wrapped_system_token_id.clone().para_id,
					relay_wrapped_system_token_id.clone(),
				);
				Self::insert_para_ids_by_system_token(
					system_token_id.clone(),
					relay_wrapped_system_token_id.clone().para_id,
				);
				WrappedSystemTokenProperties::<T>::insert(
					&relay_wrapped_system_token_id,
					&wrapped_system_token_property,
				);
			}

			// DMP call to the parachain
			Self::dmp_set_sufficient_and_system_token_weight(
				system_token_id.clone(),
				true,
				Some(system_token_weight),
			);

			Self::deposit_event(Event::<T>::SystemTokenRegistered {
				system_token_id,
				system_token_metadata,
			});

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(1_000)]
		/// Register the wrapped system token to other parachain.
		pub fn register_wrapped_system_token(
			origin: OriginFor<T>,
			system_token_id: SystemTokenId,
			wrapped_system_token_id: WrappedSystemTokenId,
		) -> DispatchResult {
			ensure_root(origin.clone())?;

			let original_para_id = system_token_id.clone().para_id;
			let wrapped_para_id = wrapped_system_token_id.clone().para_id;

			// ensure for the validity check
			{
				ensure!(
					SystemTokenList::<T>::contains_key(&system_token_id),
					Error::<T>::SystemTokenNotRegistered
				);
				ensure!(
					!SystemTokenOnParachain::<T>::contains_key(&wrapped_system_token_id),
					Error::<T>::WrappedSystemTokenAlreadyRegistered
				);

				let para_ids = ParaIdsBySystemToken::<T>::get(&system_token_id)
					.map_or(Default::default(), |para_id| para_id);

				ensure!(
					!para_ids.contains(&wrapped_para_id),
					Error::<T>::SameSystemTokenAlreadyRegistered
				);
				ensure!(
					para_ids.len() < T::MaxSystemTokenOnParachain::get() as usize,
					Error::<T>::TooManySystemTokensOnParachain
				);

				let system_tokens =
					match SystemTokenOnParachainByParaId::<T>::get(&original_para_id) {
						Some(system_tokens) => system_tokens,
						None => Default::default(),
					};
				ensure!(
					system_tokens.len() < T::MaxWrappedSystemToken::get() as usize,
					Error::<T>::TooManyWrappedSystemTokens
				);
			}

			SystemTokenOnParachain::<T>::insert(&wrapped_system_token_id, &system_token_id);
			{
				let now_as_millis_u128 = T::UnixTime::now().as_millis();
				let property = WrappedSystemTokenProperty { created_at: now_as_millis_u128 };
				WrappedSystemTokenProperties::<T>::insert(&wrapped_system_token_id, &property);
			}

			Self::insert_system_token_on_parachain_by_para_id(
				wrapped_para_id,
				wrapped_system_token_id.clone(),
			);
			Self::insert_para_ids_by_system_token(system_token_id.clone(), wrapped_para_id);

			let wrapped_asset_id = wrapped_system_token_id.clone().asset_id;

			let property = SystemTokenProperties::<T>::get(&system_token_id)
				.ok_or("property should be obtained")?;
			let system_token_weight = property.system_token_weight;

			// Register wrapped system token in relay chain
			if wrapped_system_token_id.clone().para_id == 0.into() {
				pallet_assets::pallet::Pallet::<T>::set_sufficient_and_system_token_weight(
					origin.clone(),
					wrapped_asset_id.into(),
					true,
					Some(system_token_weight),
				)?;

				pallet_assets::pallet::Pallet::<T>::update_system_token_weight(
					origin.clone(),
					wrapped_asset_id.into(),
					system_token_weight,
				)?;

				pallet_asset_link::pallet::Pallet::<T>::link_system_token(
					origin,
					0,
					wrapped_asset_id.into(),
					system_token_id,
				)?;
			} else {
				Self::dmp_create_wsys_token(
					system_token_id.clone(),
					wrapped_system_token_id.clone(),
					system_token_weight,
				)?;
			}

			Self::deposit_event(Event::<T>::WrappedSystemTokenRegistered {
				system_token_id,
				wrapped_system_token_id,
			});

			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(1_000)]
		/// Deregister the system token.
		pub fn deregister_system_token(
			origin: OriginFor<T>,
			system_token_id: SystemTokenId,
		) -> DispatchResult {
			ensure_root(origin.clone())?;

			ensure!(
				SystemTokenList::<T>::contains_key(&system_token_id),
				Error::<T>::SystemTokenNotRegistered
			);

			let (system_token_metadata, _asset_metadata) =
				SystemTokenList::<T>::get(&system_token_id).ok_or("metadata should be obtained")?;
			{
				let wrapped_system_tokens = SystemTokenOnParachain::<T>::iter_keys()
					.filter(|wrapped_system_token_id| {
						SystemTokenOnParachain::<T>::get(wrapped_system_token_id) ==
							Some(system_token_id.clone())
					})
					.collect::<Vec<SystemTokenId>>();

				for wrapped_system_token_id in wrapped_system_tokens {
					Self::set_sufficient_and_unlink(
						origin.clone(),
						wrapped_system_token_id.clone(),
						false,
					);
					SystemTokenOnParachain::<T>::remove(&wrapped_system_token_id);
					WrappedSystemTokenProperties::<T>::remove(&wrapped_system_token_id);
					Self::remove_system_token_on_parachain_by_para_id(
						wrapped_system_token_id,
					);
				}
			}

			SystemTokenList::<T>::remove(&system_token_id);
			SystemTokenProperties::<T>::remove(&system_token_id);
			SystemTokenOnParachain::<T>::remove(&system_token_id);
			ParaIdsBySystemToken::<T>::remove(&system_token_id);
			Self::remove_system_token_on_parachain_by_para_id(system_token_id.clone());

			// Change `is_sufficient` of the chain which originally issued the system token to false
			Self::dmp_set_sufficient_and_system_token_weight(system_token_id.clone(), false, None);

			Self::deposit_event(Event::<T>::SystemTokenDeregistered {
				system_token_id,
				system_token_metadata,
			});

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(1_000)]
		/// Deregister wrapped system token to other parachain.
		pub fn deregister_wrapped_system_token(
			origin: OriginFor<T>,
			wsys_token_id: SystemTokenId,
		) -> DispatchResult {
			ensure_root(origin.clone())?;
			Self::do_deregister(SystemTokenType::Wrapped(wsys_token_id))?;
			Self::set_sufficient_and_unlink(origin, wsys_token_id.clone(), false);

			Self::deposit_event(
				Event::<T>::WrappedSystemTokenDeregistered { wsys_token_id }
			);

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(1_000)]
		// Description
		//
		// ** Root(Authorized) privileged call ** 
		//
		// Params
		pub fn update_system_token_weight(
			origin: OriginFor<T>,
			system_token_id: SystemTokenId,
			system_token_weight: SystemTokenWeight,
		) -> DispatchResult {
			ensure_root(origin)?;

			ensure!(
				SystemTokenProperties::<T>::contains_key(&system_token_id),
				Error::<T>::SystemTokenNotRegistered
			);
			let wsys_tokens = Self::get_wsys_token_list(&system_token_id);
			for wsys_token_id in wsys_tokens.iter() {
				Self::update_sys_token_weight(wsys_token_id, system_token_weight)
			}

			SystemTokenProperties::<T>::try_mutate_exists(&system_token_id, |p| -> DispatchResult {
				let mut property = p.take().ok_or(Error::<T>::NotFound)?;
				property.system_token_weight = system_token_weight;
				*p = Some(property.clone());
				Self::deposit_event(Event::<T>::SetSystemTokenWeight { system_token_id, property });
				Ok(())
			})?;

			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(1_000)]
		// Description
		//
		// ** Root(Authorized) privileged call ** 
		//
		// Params
		pub fn update_fee_para_rate(
			origin: OriginFor<T>,
			para_id: IbsParaId,
			pallet_id: IbsPalletId,
			para_fee_rate: u32,
		) -> DispatchResult {
			ensure_root(origin)?;

			let encoded_call: Vec<u8> = pallet_assets::Call::<T>::update_para_fee_rate { para_fee_rate }.encode();
			system_token_xcm_handler::queue_dmp::<T>(para_id, pallet_id, encoded_call);
			Self::deposit_event(Event::<T>::SetParaFeeRate { para_id, para_fee_rate });

			Ok(())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(1_000)]
		// Description
		// Setting fee for parachain-specific calls(extrinsics).
		// When fee is charged on the parachain, extract the metadata of the call, which is 'pallet_name' and 'call_name'
		// Lookup the fee table with `key = (pallet_name, call_name)`. 
		// If value exists, we charged with that fee. Otherwise, default fee will be charged
		//
		// ** Root(Authorized) privileged call ** 
 		//
		// Params
		// - para_id: Id of the parachain of which fee to be set
		// - pallet_id: Pallet index of `System Token`
		// - pallet_name: Name of the pallet of which extrinsic is defined
		// - call_name: Name of the call(extrinsic) of which fee to be set
		// - fee: Amount of fee to be set
		pub fn set_fee_table(
			origin: OriginFor<T>,
			para_call_metadata: ParaCallMetadata,
			fee: T::Balance,
		) -> DispatchResult {
			ensure_root(origin)?;

			let ParaCallMetadata {
				para_id,
				pallet_id,
				pallet_name,
				call_name
			} = para_call_metadata.clone();
			let encoded_call: Vec<u8> = pallet_system_token::Call::<T>::set_fee_table { pallet_name, call_name, fee }.encode();
			system_token_xcm_handler::queue_dmp::<T>(para_id, pallet_id, encoded_call);

			Self::deposit_event(Event::<T>::SetFeeTable { para_call_metadata, fee });

			Ok(())
		}
	}
}

// System token related interal methods
impl<T: Config> Pallet<T> {
	
	fn get_wsys_token_list(system_token_id: &SystemTokenId) -> Vec<WrappedSystemTokenId> {
		SystemTokenOnParachain::<T>::iter_keys()
			.filter(|wrapped_system_token_id| {
				SystemTokenOnParachain::<T>::get(wrapped_system_token_id).as_ref() ==
					Some(system_token_id)
			})
			.collect::<Vec<WrappedSystemTokenId>>()
	}

	fn do_deregister(system_token_type: SystemTokenType) -> DispatchResult {
		match system_token_type {
			SystemTokenType::Original(sys_token_id) => Self::try_deregister_sys_token(sys_token_id),
			SystemTokenType::Wrapped(wsys_token_id) => Self::try_deregister_wsys_token(&wsys_token_id)
		}
	}

	fn try_deregister_sys_token(sys_token_id: SystemTokenId) -> DispatchResult {

		Ok(())
	}

	fn try_deregister_wsys_token(wsys_token_id: &WrappedSystemTokenId) -> DispatchResult {
		
		let system_token_id = SystemTokenOnParachain::<T>::get(&wsys_token_id)
			.ok_or(Error::<T>::WrappedSystemTokenNotRegistered)?;

		let SystemTokenId {
			para_id,
			..
		} = wsys_token_id.clone();

		SystemTokenOnParachainByParaId::<T>::try_mutate_exists(
			para_id,
			|maybe_system_tokens| -> Result<(), DispatchError> {
				let system_tokens = maybe_system_tokens.as_mut().ok_or(Error::<T>::NotFound)?;
				system_tokens.retain(|x| x != &wsys_token_id);
				Ok(())
			},
		)?;

		ParaIdsBySystemToken::<T>::try_mutate_exists(
			system_token_id,
			|maybe_para_ids| -> Result<(), DispatchError> {
				let para_ids = maybe_para_ids.as_mut().ok_or(Error::<T>::NotFound)?;
				para_ids.retain(|x| *x != para_id);
				Ok(())
			},
		)?;

		SystemTokenOnParachain::<T>::remove(&wsys_token_id);
		WrappedSystemTokenProperties::<T>::remove(&wsys_token_id);

		Ok(())
	}
}

// XCM-related internal methods
impl<T: Config> Pallet<T>
where
	<T as pallet_assets::Config>::Balance: From<u128>,
	<T as pallet_assets::Config>::AssetIdParameter: From<u32>,
	AssetIdOf<T>: From<u32>,
{

	fn set_sufficient_and_unlink(origin: OriginFor<T>, system_token_id: SystemTokenId, is_sufficient: bool) {
		let SystemTokenId {
			para_id,
			pallet_id,
			asset_id,
		} = system_token_id;
		if para_id == 0u32 {
			// ToDo: Change to internal method
			// Relay Chain
			let _ = pallet_assets::pallet::Pallet::<T>::set_sufficient_with_unlink_system_token(
				origin.clone(),
				asset_id.into(),
				false,
			);
		} else {
			// Parachain
			let encoded_call = pallet_assets::Call::<T>::set_sufficient_with_unlink_system_token {
				id: asset_id.into(),
				is_sufficient,
			}.encode();
			system_token_xcm_handler::queue_dmp::<T>(para_id, pallet_id, encoded_call);
		}
	}

	fn dmp_set_sufficient_and_system_token_weight(
		system_token_id: SystemTokenId,
		is_sufficient: bool,
		system_token_weight: Option<SystemTokenWeight>,
	) {
		let config = <configuration::Pallet<T>>::config();
		let xcm = {
			use parity_scale_codec::Encode as _;
			use xcm::opaque::{latest::prelude::*, VersionedXcm};

			let mut encoded: Vec<u8> = [system_token_id.clone().pallet_id as u8].into(); // asset pallet number
			let mut call_encode =
				pallet_assets::Call::<T>::set_sufficient_and_system_token_weight {
					id: system_token_id.clone().asset_id.into(),
					is_sufficient,
					system_token_weight,
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
					require_weight_at_most: Weight::from_parts(REF_WEIGHT, PROOF_WEIGHT),
					call: encoded.into(),
				},
			]))
			.encode()
		};
		if let Err(dmp::QueueDownwardMessageError::ExceedsMaxMessageSize) =
			<dmp::Pallet<T>>::queue_downward_message(
				&config,
				IbsParaId::from(system_token_id.clone().para_id).into(),
				xcm,
			) {
			log::error!(
				target: "runtime::system_token_manager",
				"sending 'dmp' failed."
			);
		};
	}

	fn update_sys_token_weight(
		system_token_id: &SystemTokenId,
		system_token_weight: SystemTokenWeight,
	) {
		let SystemTokenId {
			para_id,
			pallet_id,
			asset_id,
		} = system_token_id.clone();
		
		if para_id == 0u32 {
			// Relay Chain
			pallet_assets::Call::<T>::update_system_token_weight {
				id: asset_id.into(),
				system_token_weight,
			};
			return
		} else {
			// Parachain
			let encoded_call = pallet_assets::Call::<T>::update_system_token_weight {
				id: asset_id.into(),
				system_token_weight,
			}.encode();
			system_token_xcm_handler::queue_dmp::<T>(para_id, pallet_id, encoded_call);
		}
	}

	fn dmp_create_wsys_token(
		system_token_id: SystemTokenId,
		wrapped_system_token_id: WrappedSystemTokenId,
		system_token_weight: SystemTokenWeight,
	) -> DispatchResult {
		let (_, asset_metadata) =
			SystemTokenList::<T>::get(&system_token_id).ok_or("metadata should be obtained")?;
		let root: T::AccountId = PalletId(*b"infra/rt").into_account_truncating();
		let encoded_call: Vec<u8> = pallet_assets::Call::<T>::force_create_with_metadata { 
			id: wrapped_system_token_id.clone().asset_id.into(),
			owner: T::Lookup::unlookup(root),
			min_balance: asset_metadata.min_balance,
			is_sufficient: true,
			name: asset_metadata.name.to_vec(),
			symbol: asset_metadata.symbol.to_vec(),
			decimals: asset_metadata.decimals,
			is_frozen: false,
			system_token_id,
			asset_link_parents: 1,
			system_token_weight,
		}.encode();

		system_token_xcm_handler::queue_dmp::<T>(
			wrapped_system_token_id.para_id, 
			wrapped_system_token_id.pallet_id, 
			encoded_call
		);

		Ok(())
	}

	fn insert_system_token_on_parachain_by_para_id(
		para_id: IbsParaId,
		system_token_id: SystemTokenId,
	) {
		if SystemTokenOnParachainByParaId::<T>::contains_key(para_id.clone()) {
			let _ = SystemTokenOnParachainByParaId::<T>::try_mutate_exists(
				para_id.clone(),
				|maybe_system_tokens| -> Result<(), DispatchError> {
					let system_tokens =
						maybe_system_tokens.as_mut().ok_or(Error::<T>::Unknown)?;
					system_tokens.try_push(system_token_id.clone()).unwrap_or(());
					Ok(())
				},
			);
		} else {
			SystemTokenOnParachainByParaId::<T>::insert(
				&para_id,
				BoundedVec::try_from(vec![system_token_id.clone()]).unwrap(),
			);
		}
	}
	fn insert_para_ids_by_system_token(system_token_id: SystemTokenId, para_id: IbsParaId) {
		if ParaIdsBySystemToken::<T>::contains_key(system_token_id.clone()) {
			let _ = ParaIdsBySystemToken::<T>::try_mutate_exists(
				system_token_id.clone(),
				|maybe_para_ids| -> Result<(), DispatchError> {
					let para_ids = maybe_para_ids.as_mut().ok_or(Error::<T>::Unknown)?;
					para_ids.try_push(para_id.clone()).unwrap_or(());
					Ok(())
				},
			);
		} else {
			ParaIdsBySystemToken::<T>::insert(
				&system_token_id,
				BoundedVec::try_from(vec![para_id.clone()]).unwrap(),
			);
		}
	}
	fn remove_system_token_on_parachain_by_para_id(system_token_id: SystemTokenId) {
		if SystemTokenOnParachainByParaId::<T>::contains_key(system_token_id.clone().para_id) &&
			SystemTokenOnParachainByParaId::<T>::get(system_token_id.clone().para_id)
				.unwrap()
				.len() > 1
		{
			let _ = SystemTokenOnParachainByParaId::<T>::try_mutate_exists(
				system_token_id.clone().para_id,
				|maybe_system_tokens| -> Result<(), DispatchError> {
					let system_tokens =
						maybe_system_tokens.as_mut().ok_or(Error::<T>::Unknown)?;
					system_tokens.retain(|x| *x != system_token_id.clone());
					Ok(())
				},
			);
		} else {
			SystemTokenOnParachainByParaId::<T>::remove(&system_token_id.clone().para_id);
		}
	}
}


impl<T: Config> SystemTokenInterface for Pallet<T> {
	fn is_system_token(system_token_id: SystemTokenId) -> bool {
		if let Some(_) = <SystemTokenList<T>>::get(system_token_id) {
			return true
		}
		false
	}
	fn convert_to_original_system_token(
		wrapped_system_token_id: WrappedSystemTokenId,
	) -> Option<SystemTokenId> {
		if let Some(s) = <SystemTokenOnParachain<T>>::get(&wrapped_system_token_id) {
			Self::deposit_event(Event::<T>::SystemTokenConverted {
				wrapped_system_token_id,
				system_token_id: s.clone(),
			});
			return Some(s)
		}
		None
	}
	fn adjusted_weight(system_token_id: SystemTokenId, vote_weight: VoteWeight) -> VoteWeight {
		// updated_vote_weight = vote_weight * system_token_weight / base_system_token_weight
		match <SystemTokenProperties<T>>::get(system_token_id) {
			Some(property) => {
				let system_token_weight: u128 = property.system_token_weight.into();
				return vote_weight.saturating_mul(system_token_weight) / BASE_SYSTEM_TOKEN_WEIGHT
			},
			None => return vote_weight,
		}
	}
}

pub mod system_token_xcm_handler {
	use super::{Weight, Vec, Config, IbsPalletId, IbsParaId};
	use crate::{configuration, dmp};
	use parity_scale_codec::Encode;
	use xcm::{
		v2::OriginKind,
		v3::MultiAsset,
		opaque::{latest::prelude::*, VersionedXcm},
	};
	use sp_std::vec;

	const REF_WEIGHT: u64 = 500_000_000;
	const PROOF_WEIGHT: u64 = 20_000;

	fn encode_pallet_call(
		pallet_id: IbsPalletId,
		mut encoded_call: Vec<u8>
	) -> Vec<u8> {
		let mut encoded: Vec<u8> = [pallet_id].into();
		encoded.append(&mut encoded_call);
		encoded
	}

	fn transact_xcm(
		fees: MultiAsset,
		origin_kind: Option<OriginKind>,
		require_weight_at_most: Option<Weight>,
		encoded_call: Vec<u8>,
	) -> Vec<u8> {
		VersionedXcm::from(Xcm(vec![
			BuyExecution {
				fees: fees.into(),
				weight_limit: WeightLimit::Unlimited,
			},
			Transact {
				origin_kind: origin_kind.map_or(xcm::v2::OriginKind::Superuser, |o| o),
				require_weight_at_most: require_weight_at_most.map_or(Weight::from_parts(REF_WEIGHT, PROOF_WEIGHT), |w| w),
				call: encoded_call.into(),
			},
		])).encode()
	}

	fn build_xcm(
		pallet_id: IbsPalletId,
		call: Vec<u8>,
	) -> Vec<u8> {
		let encoded_call = encode_pallet_call(pallet_id, call);
		let fees = MultiAsset { id: Concrete(Here.into()), fun: Fungible(10000) };
		let xcm = transact_xcm(fees, None, None, encoded_call);

		xcm
	}

	pub fn queue_dmp<T: Config>(
		para_id: IbsParaId,
		pallet_id: IbsPalletId,
		encoded_call: Vec<u8>,
	) {
		let config = <configuration::Pallet<T>>::config();
		let xcm = build_xcm(pallet_id, encoded_call);
		if let Err(dmp::QueueDownwardMessageError::ExceedsMaxMessageSize) =
			<dmp::Pallet<T>>::queue_downward_message(
				&config, 
				IbsParaId::from(para_id).into(), 
				xcm
			) {
				log::error!(
					target: "runtime::system_token_manager",
					"sending 'dmp' failed."
				);
			};
	}
}
