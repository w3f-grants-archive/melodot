// Copyright 2023 ZeroDAO

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at

//     http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	sp_runtime::traits::CheckedSub,
	traits::{Currency, Get},
};
use frame_system::pallet_prelude::*;
use melo_core_primitives::{config::PRE_CELL_LEADING_ZEROS, traits::CommitmentFromPosition};
use melo_proof_of_space::{Cell, FarmerId, PreCell, Solution};
use sp_std::prelude::*;

pub use pallet::*;

pub mod weights;
pub use weights::*;

mod mock;
mod test;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

type BalanceOf<T> =
<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

#[frame_support::pallet]
pub mod pallet {

	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The method to get commitment from a position.
		type CommitmentFromPosition: CommitmentFromPosition<BlockNumber = Self::BlockNumber>;

		/// The currency trait.
		type Currency: Currency<Self::AccountId>;

		#[pallet::constant]
		type RewardAmount: Get<BalanceOf<Self>>;

		#[pallet::constant]
		type MaxClaimantsPerBlock: Get<u32>;
	}

	#[pallet::storage]
	#[pallet::getter(fn claimants)]
	pub type ClaimantsForBlock<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::BlockNumber,
		BoundedVec<T::AccountId, T::MaxClaimantsPerBlock>,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event emitted when a reward is claimed.
		RewardClaimed(T::AccountId, BalanceOf<T>),
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Error for report of a future block.
		InvalidSolution,
		/// Error when the pre-commitment is not found.
		PreCommitNotFound,
		/// Error when the win-commitment is not found.
		WinCommitNotFound,
		/// Error when the max claimants per block is reached.
		MaxClaimantsReached,
		/// Error when the user has already claimed.
		AlreadyClaimed,
		/// Error when the storage limit is reached.
		StorageLimitReached,
		///
		BlockNumberUnderflow,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Claim a reward.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::claim())]
		#[allow(clippy::large_enum_variant)]
		pub fn claim(
			origin: OriginFor<T>,
			pre_cell: PreCell,
			win_cell_left: Cell<BlockNumberFor<T>>,
			win_cell_right: Cell<BlockNumberFor<T>>,
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			let now = <frame_system::Pallet<T>>::block_number();

			let mut claimants = ClaimantsForBlock::<T>::get(now);
			ensure!(
				claimants.len() < T::MaxClaimantsPerBlock::get() as usize,
				Error::<T>::MaxClaimantsReached
			);
			ensure!(!claimants.contains(&who), Error::<T>::AlreadyClaimed);

			let pre_block_num = CheckedSub::checked_sub(
				&now,
				&T::BlockNumber::from(1u32),
			)
			.ok_or(Error::<T>::BlockNumberUnderflow)?;

			let pre_block_hash = <frame_system::Pallet<T>>::block_hash(pre_block_num);
			let win_block_hash_left =
				<frame_system::Pallet<T>>::block_hash(win_cell_left.metadata.block_number());
			let win_block_hash_right =
				<frame_system::Pallet<T>>::block_hash(win_cell_right.metadata.block_number());

			let left_block_num = win_cell_left.metadata.block_number();
			let right_block_num = win_cell_right.metadata.block_number();

			// Get commitments from positions
			let pre_commit =
				T::CommitmentFromPosition::commitments(pre_block_num, &pre_cell.seg.position)
					.ok_or(Error::<T>::PreCommitNotFound)?;

			let left_commit =
				T::CommitmentFromPosition::commitments(left_block_num, &win_cell_left.seg.position)
					.ok_or(Error::<T>::WinCommitNotFound)?;

			let right_commit = T::CommitmentFromPosition::commitments(
				right_block_num,
				&win_cell_right.seg.position,
			)
			.ok_or(Error::<T>::WinCommitNotFound)?;

			let farmer_id = FarmerId::new::<T::AccountId>(who.clone());

			let solution = Solution::<T::Hash, BlockNumberFor<T>>::new(
				&pre_block_hash,
				&farmer_id,
				&pre_cell,
				&win_cell_left,
				&win_cell_right,
			);

			ensure!(
				solution.verify(
					&pre_commit,
					&left_commit,
					&right_commit,
					&win_block_hash_left,
					&win_block_hash_right,
					PRE_CELL_LEADING_ZEROS,
					1,
				),
				Error::<T>::InvalidSolution
			);

			claimants.try_push(who.clone()).map_err(|_| Error::<T>::StorageLimitReached)?;
			ClaimantsForBlock::<T>::insert(now, claimants);

			let reward = T::RewardAmount::get();
			T::Currency::deposit_creating(&who, reward);

			Self::deposit_event(Event::RewardClaimed(who, reward));

			Ok(().into())
		}
	}
}

impl<T: Config> Pallet<T> {}
