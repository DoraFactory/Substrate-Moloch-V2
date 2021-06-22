#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// https://substrate.dev/docs/en/knowledgebase/runtime/frame
/// debug guide https://substrate.dev/recipes/runtime-printing.html
use frame_support::{
	decl_module, decl_storage, decl_event, decl_error, dispatch, debug, ensure,
	traits::{Currency, EnsureOrigin, ReservableCurrency, OnUnbalanced, Get, BalanceStatus, ExistenceRequirement::{KeepAlive, AllowDeath}},
};
use sp_runtime::{ModuleId, traits::{ AccountIdConversion }};
use frame_support::codec::{Encode, Decode};
use frame_system::{ensure_signed};
use sp_std::{vec::Vec, convert::{TryInto}};
use pallet_timestamp;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

// TODO: Not support enum in storage
#[derive(Encode, Decode, Clone, PartialEq)]
pub enum Vote {
	// default value, counted as abstention
	Null,
	Yes,
	No
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
pub struct Member<AccountId> {
	// the # of shares assigned to this member
	pub shares: u128,
	// the loot amount available to this member (combined with shares on ragequit)
	pub loot: u128,
	// highest proposal index # on which the member voted YES
	pub highest_index_yes_vote: u128,
	// always true once a member has been created
	pub exists: bool,
	// the key responsible for submitting proposals and voting - defaults to member address unless updated
	pub delegate_key: AccountId,
	// set to proposalIndex of a passing guild kick proposal for this member, prevents voting on and sponsoring proposals
	pub jailed_at: u128,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
pub struct Proposal<AccountId> {
    // the account that submitted the proposal (can be non-member)
	pub proposer: AccountId,
	// the applicant who wishes to become a member - this key will be used for withdrawals (doubles as guild kick target for gkick proposals)
	pub applicant: AccountId,
	// the member that sponsored the proposal (moving it into the queue)
	pub sponsor: AccountId,
	// the # of shares the applicant is requesting
	pub shares_requested: u128,
	// the # of loot the applicant is requesting
	pub loot_requested: u128,
	// amount of tokens requested as payment
	pub payment_requested: u128,
	// amount of tokens offered as tribute
	pub tribute_offered: u128,
	// [sponsored, processed, didPass, cancelled, whitelist, guildkick]
	pub flags: [bool; 6],
	// the period in which voting can start for this proposal
	pub starting_period: u128,
	// the total number of YES votes for this proposal
	pub yes_votes: u128,
	// the total number of NO votes for this proposal
	pub no_votes: u128,
	// proposal details - Must be ascii chars, limited length
	pub details: Vec<u8>,
	// the maximum # of total shares encountered at a yes vote on this proposal
	pub max_total_shares_at_yes: u128,
}

type MemberOf<T> = Member<<T as frame_system::Trait>::AccountId>;
type ProposalOf<T> = Proposal<<T as frame_system::Trait>::AccountId>;
type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance;
type NegativeImbalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::NegativeImbalance;

/// Configure the pallet by specifying the parameters and types on which it depends.
pub trait Config: pallet_timestamp::Trait + frame_system::Trait {
	// used to generate sovereign account
	// refer: https://github.com/paritytech/substrate/blob/743accbe3256de2fc615adcaa3ab03ebdbbb4dbd/frame/treasury/src/lib.rs#L92
	type ModuleId: Get<ModuleId>;

	/// Origin from which admin must come.
	type AdminOrigin: EnsureOrigin<Self::Origin>;

    // The runtime must supply this pallet with an Event type that satisfies the pallet's requirements.
    type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;

	/// The currency trait.
	type Currency: ReservableCurrency<Self::AccountId>;

	/// What to do with slashed funds.
	type Slashed: OnUnbalanced<NegativeImbalanceOf<Self>>;

	// maximum length of voting period
	type MaxVotingPeriodLength: Get<u128>;

	// maximum length of grace period
	type MaxGracePeriodLength: Get<u128>;

	// maximum dilution bound
	type MaxDilutionBound: Get<u128>;

	// maximum number of shares
	type MaxShares: Get<u128>;

	
}

// The pallet's runtime storage items.
// https://substrate.dev/docs/en/knowledgebase/runtime/storage
decl_storage! {
	// A unique name is used to ensure that the pallet's storage items are isolated.
	// This name may be updated, but each pallet in the runtime must use a unique name.
	trait Store for Module<T: Config> as MolochV2 {
		// Learn more about declaring storage items:
		// https://substrate.dev/docs/en/knowledgebase/runtime/storage#declaring-storage-items
		// Map, each round start with an id => bool 
		TotalShares get(fn totoal_shares): u128;
		TotalLoot get(fn totoal_loot): u128;
		PeriodDuration get(fn period_duration): u32;
        VotingPeriodLength get(fn voting_period_length): u128;
        GracePeriodLength get(fn grace_period_length): u128;
		ProposalCount get(fn proposal_count): u128;
        ProposalDeposit get(fn proposal_deposit): BalanceOf<T>;
        DilutionBound get(fn dilution_bound): u128;
        ProcessingReward get(fn processing_reward): BalanceOf<T>;
		SummonTime get(fn summon_time): T::Moment;
		Members get(fn members): map hasher(blake2_128_concat) T::AccountId  => MemberOf<T>;
		AddressOfDelegates get(fn address_of_delegate): map hasher(blake2_128_concat) T::AccountId  => T::AccountId;
		ProposalQueue get(fn proposal_queue): Vec<u128>;
		Proposals get(fn proposals): map hasher(blake2_128_concat) u128 => ProposalOf<T>;
		ProsedToKick get(fn proposed_to_kick): map hasher(blake2_128_concat) T::AccountId => bool;
		ProposalVotes get(fn proposal_vote): double_map hasher(blake2_128_concat) u128, hasher(blake2_128_concat) T::AccountId => u8;
	}
	add_extra_genesis {
		build(|_config| {
			// Create pallet's internal account
			let _ = T::Currency::make_free_balance_be(
				&<Module<T>>::account_id(),
				T::Currency::minimum_balance(),
			);
			let _ = T::Currency::make_free_balance_be(
				&<Module<T>>::custody_account(),
				T::Currency::minimum_balance(),
			);
		});
	}
}

// Pallets use events to inform users when important changes are made.
// https://substrate.dev/docs/en/knowledgebase/runtime/events
decl_event!(
	pub enum Event<T> where AccountId = <T as frame_system::Trait>::AccountId, 
	        Balance = <<T as Config>::Currency as Currency<<T as frame_system::Trait>::AccountId>>::Balance {
		/// Event documentation should end with an array that provides descriptive names for event
		/// parameters. [proposalIndex, delegateKey, memberAddress, applicant, tokenTribute, sharesRequested] 
		SubmitProposal(u128, AccountId, AccountId, AccountId, u128, u128),
		/// parameters. [proposalIndex, delegateKey, memberAddress, uintVote]
		SubmitVote(u128, AccountId, AccountId, u8),
		/// parameters. [proposalIndex, applicant, memberAddress, tokenTribute, sharesRequested, didPass]
		ProcessProposal(u128, AccountId, AccountId, u128, u128, bool),
		/// parameters. [memberAddress, sharesToBurn]
		Ragequit(AccountId, u128),
		/// parameters. [proposalIndex, applicantAddress]
		Abort(u128, AccountId),
		/// parameters. [memberAddress, newDelegateKey]
		UpdateDelegateKey(AccountId, AccountId),
		/// parameters. [summoner, shares]
		SummonComplete(AccountId, u128),
		/// parameters. [totalShares, dilutionBond, maxTotalSharesVoteAtYes]
		DilutionBoundExeceeds(u128, u128, u128),
		/// parameters. [currentReserved, requiredReserved]
		CustodyBalanceOutage(Balance, Balance),
		CustodySucceeded(AccountId, Balance),
	}
);

// Errors inform users that something went wrong.
decl_error! {
	pub enum Error for Module<T: Config> {
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
		VotingPeriodLengthTooBig,
		DilutionBoundTooBig,
		GracePeriodLengthTooBig,
		NoEnoughProposalDeposit,
		NoEnoughShares,
		NoEnoughLoot,
		NotMember,
		NotStandardProposal,
		NotKickProposal,
		NotProposalProposer,
		SharesOverFlow,
		ProposalNotExist,
		ProposalNotStart,
		ProposalNotReady,
		ProposalHasSponsored,
		ProposalHasProcessed,
		ProposalHasAborted,
		ProposalNotProcessed,
		PreviousProposalNotProcessed,
		ProposalExpired,
		InvalidVote,
		MemberHasVoted,
		NoOverwriteDelegate,
		NoOverwriteMember,
		NoCustodyFound,
		MemberInJail,
		MemberNotInJail,
	}
}

// Dispatchable functions allows users to interact with the pallet and invoke state changes.
// These functions materialize as "extrinsics", which are often compared to transactions.
// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
decl_module! {
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
		// Errors must be initialized if they are used by the pallet.
		type Error = Error<T>;

		// Events must be initialized if they are used by the pallet.
		fn deposit_event() = default;
		const MaxVotingPeriodLength: u128 = T::MaxVotingPeriodLength::get();
		const MaxGracePeriodLength: u128 = T::MaxGracePeriodLength::get();
		const MaxDilutionBound: u128 = T::MaxDilutionBound::get();
		const MaxShares: u128 = T::MaxShares::get();
		
		/// Summon a group or orgnization
		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn summon(origin, period_duration: u32, voting_period_length: u128,
			          grace_period_length: u128, dilution_bound: u128,
					  #[compact] proposal_deposit: BalanceOf<T>, 
					  #[compact]  processing_reward: BalanceOf<T>) -> dispatch::DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(voting_period_length <= T::MaxVotingPeriodLength::get(), Error::<T>::VotingPeriodLengthTooBig);
			ensure!(grace_period_length <= T::MaxGracePeriodLength::get(), Error::<T>::GracePeriodLengthTooBig);
			ensure!(dilution_bound <= T::MaxDilutionBound::get(), Error::<T>::DilutionBoundTooBig);
			ensure!(proposal_deposit >= processing_reward, Error::<T>::NoEnoughProposalDeposit);

			SummonTime::<T>::put(pallet_timestamp::Module::<T>::now());
			PeriodDuration::put(period_duration);
			VotingPeriodLength::put(voting_period_length);
			GracePeriodLength::put(grace_period_length);
			DilutionBound::put(dilution_bound);

			ProposalDeposit::<T>::put(proposal_deposit);
			ProcessingReward::<T>::put(processing_reward);
			let member = Member {
				shares: 1,
				highest_index_yes_vote: 0,
				loot: 0,
				jailed_at: 0,
				exists: true,
				delegate_key: who.clone(),
			};
			Members::<T>::insert(who.clone(), member);
			AddressOfDelegates::<T>::insert(who.clone(), who.clone());
			TotalShares::put(1);
			Self::deposit_event(RawEvent::SummonComplete(who, 1));
			Ok(())
		}

		/// Anyone can submit proposal, but need to ensure enough tokens
		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn submit_proposal(origin, applicant: T::AccountId, #[compact] tribute_offered: BalanceOf<T>,
			                   shares_requested: u128, loot_requested: u128, #[compact] payment_requested: BalanceOf<T>, 
							   details: Vec<u8>) -> dispatch::DispatchResult {
			let who = ensure_signed(origin)?;
			if Members::<T>::contains_key(who.clone()) {
				ensure!(Members::<T>::get(who.clone()).jailed_at == 0, Error::<T>::MemberInJail);
			}
			let total_requested = loot_requested.checked_add(shares_requested).unwrap();
			let future_shares = TotalShares::get().checked_add(total_requested).unwrap();
			ensure!(future_shares <= T::MaxShares::get(), Error::<T>::SharesOverFlow);

			// collect proposal deposit from proposer and store it in the Moloch until the proposal is processed
			let _ = T::Currency::transfer(&who, &Self::custody_account(), tribute_offered, KeepAlive);

			let tribute_offered_num = Self::balance_to_u128(tribute_offered);
			let payment_requested_num = Self::balance_to_u128(payment_requested);
			let flags = [false; 6];
			Self::create_proposal(who.clone(), applicant.clone(), shares_requested, loot_requested, 
			                      tribute_offered_num, payment_requested_num, details, flags);
			Ok(())
		}

		/// propose a guild kick proposal
		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn submit_guild_kick_proposal(origin, member_to_kick: T::AccountId, details: Vec<u8>) -> dispatch::DispatchResult  {
			let who = ensure_signed(origin)?;
			ensure!(Members::<T>::contains_key(member_to_kick.clone()), Error::<T>::NotMember);
			let member = Members::<T>::get(member_to_kick.clone());
			ensure!(member.shares > 0 || member.loot > 0, Error::<T>::NoEnoughShares);
			ensure!(member.jailed_at == 0, Error::<T>::MemberInJail);

			// [sponsored, processed, didPass, cancelled, whitelist, guildkick]
			let mut flags = [false; 6];
			flags[5] = true;
			Self::create_proposal(who.clone(), member_to_kick.clone(), 0, 0, 0, 0, details, flags);
			Ok(())
		}

		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn sponsor_proposal(origin, proposal_index: u128) -> dispatch::DispatchResult  {
			let who = ensure_signed(origin)?;
			ensure!(Members::<T>::contains_key(who.clone()), Error::<T>::NotMember);
			ensure!(Proposals::<T>::contains_key(proposal_index), Error::<T>::ProposalNotExist);
			let proposal = Proposals::<T>::get(proposal_index);
			// check proposal status
			ensure!(!proposal.flags[0], Error::<T>::ProposalHasSponsored);
			ensure!(!proposal.flags[3], Error::<T>::ProposalHasAborted);
			// reject in jailed memeber to process
			if Members::<T>::contains_key(who.clone()) {
				ensure!(Members::<T>::get(who.clone()).jailed_at == 0, Error::<T>::MemberInJail);
			}

			// collect proposal deposit from proposer and store it in the Moloch until the proposal is processed
			let _ = T::Currency::transfer(&who, &Self::account_id(), ProposalDeposit::<T>::get(), KeepAlive);

			if proposal.flags[5] {
				ensure!(!ProsedToKick::<T>::contains_key(proposal.applicant.clone()), Error::<T>::MemberInJail);
				ProsedToKick::<T>::insert(proposal.applicant, true);
			}
			let proposal_queue = ProposalQueue::get();
			let proposal_period = match proposal_queue.len() {
				0 => 0,
				n => Proposals::<T>::get(proposal_queue[n-1]).starting_period
			};
			let starting_period = proposal_period.max(Self::get_current_period()).checked_add(1).unwrap();
			Proposals::<T>::mutate(proposal_index, |p| {
				p.starting_period = starting_period;
				// sponsored
				p.flags[0] = true;
				p.sponsor = AddressOfDelegates::<T>::get(who.clone());
			});
			ProposalQueue::append(proposal_index);

			Ok(())
		}

		/// One of the members submit a vote
		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn submit_vote(origin, proposal_index: u128, vote_unit: u8) -> dispatch::DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(AddressOfDelegates::<T>::contains_key(who.clone()), Error::<T>::NotMember);
			let delegate = AddressOfDelegates::<T>::get(who.clone());
			let member = Members::<T>::get(delegate.clone());
			ensure!(member.shares > 0, Error::<T>::NoEnoughShares);
			
			let proposal_len = ProposalQueue::get().len();
			ensure!(proposal_index < proposal_len.try_into().unwrap(), Error::<T>::ProposalNotExist);
			let _usize_proposal_index = TryInto::<usize>::try_into(proposal_index).ok().unwrap();
			let proposal_id = ProposalQueue::get()[_usize_proposal_index];
			let proposal = Proposals::<T>::get(proposal_id);
			ensure!(vote_unit < 3 && vote_unit > 0, Error::<T>::InvalidVote);
			ensure!(Self::get_current_period() >= proposal.starting_period, Error::<T>::ProposalNotStart);
			ensure!(
				Self::get_current_period() <  VotingPeriodLength::get() + proposal.starting_period,
				Error::<T>::ProposalExpired
			);
			ensure!(!ProposalVotes::<T>::contains_key(proposal_index, delegate.clone()), Error::<T>::MemberHasVoted);
			ensure!(!proposal.flags[3], Error::<T>::ProposalHasAborted);
			let vote = match vote_unit {
				1 => Vote::Yes,
				2 => Vote::No,
				_ => Vote::Null
			};
			ProposalVotes::<T>::insert(proposal_id, delegate.clone(), vote_unit);

			// update proposal
			Proposals::<T>::mutate(proposal_id, |p| {
				if vote == Vote::Yes {
					p.yes_votes = proposal.yes_votes.checked_add(member.shares).unwrap();
					if proposal_index > member.highest_index_yes_vote {	
						Members::<T>::mutate(delegate.clone(), |mem| {
							mem.highest_index_yes_vote = proposal_index;
						});
					}

					// update max yes
					let all_loot_shares = TotalShares::get().checked_add(TotalLoot::get()).unwrap();
					if all_loot_shares > proposal.max_total_shares_at_yes {
						p.max_total_shares_at_yes = all_loot_shares;
					}
				} else if vote == Vote::No {
					p.no_votes = proposal.no_votes.checked_add(member.shares).unwrap();
				}
			});
			Self::deposit_event(RawEvent::SubmitVote(proposal_index, who, delegate, vote_unit));
			Ok(())
		}

		/// Process a proposal in queue
		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn process_proposal(origin, proposal_index: u128) -> dispatch::DispatchResult {
			let who = ensure_signed(origin)?;
			let proposal_len = ProposalQueue::get().len();
			ensure!(proposal_index < proposal_len.try_into().unwrap(), Error::<T>::ProposalNotExist);
			let _usize_proposal_index = TryInto::<usize>::try_into(proposal_index).ok().unwrap();
			let proposal_id = ProposalQueue::get()[_usize_proposal_index];
			let proposal = &mut Proposals::<T>::get(proposal_id);
			ensure!(!proposal.flags[4] && !proposal.flags[5], Error::<T>::NotStandardProposal);
			ensure!(
				Self::get_current_period() - VotingPeriodLength::get() - GracePeriodLength::get() >= proposal.starting_period,
				Error::<T>::ProposalNotReady
			);
			ensure!(proposal.flags[1] == false, Error::<T>::ProposalHasProcessed);
			ensure!(proposal_index == 0 || Proposals::<T>::get(ProposalQueue::get()[_usize_proposal_index - 1]).flags[1], 
			        Error::<T>::PreviousProposalNotProcessed);

			proposal.flags[1] = true;
			let mut did_pass = Self::should_pass(Proposals::<T>::get(proposal_id));
			let tribute_offered = Self::u128_to_balance(proposal.tribute_offered);
			let free_token_num = Self::balance_to_u128(T::Currency::free_balance(&Self::account_id()));
			// too many tokens requested
			if proposal.payment_requested > free_token_num {
				did_pass = false;
			}
			// shares+loot overflow
			let total_requested = proposal.loot_requested.checked_add(proposal.shares_requested).unwrap();
			let future_shares = TotalShares::get().checked_add(total_requested).unwrap();
			ensure!(future_shares.checked_add(TotalLoot::get()).unwrap() <= T::MaxShares::get(), Error::<T>::SharesOverFlow);

			// TODO: guild is full

			// Proposal passed
			if did_pass {
				// mark did_pass to true
				proposal.flags[2] = true;

				// if the applicant is already a member, add to their existing shares
				if Members::<T>::contains_key(&proposal.applicant) {
					Members::<T>::mutate(&proposal.applicant, |mem| {
						mem.shares = mem.shares.checked_add(proposal.shares_requested).unwrap();
						mem.loot = mem.loot.checked_add(proposal.loot_requested).unwrap();
					});
				} else {
					// if the applicant address is already taken by a member's delegateKey, reset it to their member address
					if AddressOfDelegates::<T>::contains_key(proposal.applicant.clone()) {
						let delegate = AddressOfDelegates::<T>::get(proposal.applicant.clone());
						Members::<T>::mutate(delegate.clone(), |mem| {
							mem.delegate_key = delegate.clone();
						});
						AddressOfDelegates::<T>::insert(delegate.clone(), delegate.clone());
					}
					// add new member
					let member = Member {
						shares: proposal.shares_requested,
						highest_index_yes_vote: 0,
						loot: proposal.loot_requested,
						jailed_at: 0,
						exists: true,
						delegate_key: proposal.applicant.clone(),
					};
					Members::<T>::insert(proposal.applicant.clone(), member);
					AddressOfDelegates::<T>::insert(proposal.applicant.clone(), proposal.applicant.clone());
				}

				// mint new shares
				let totoal_shares = TotalShares::get().checked_add(proposal.shares_requested).unwrap();
				TotalShares::put(totoal_shares);
				// transfer correponding balance from custody account to guild bank's free balance
				let res = T::Currency::transfer(&Self::custody_account(),  &Self::account_id(), tribute_offered, AllowDeath);
				debug::info!("asdsa---{:?}", res);
			} else {
				// Proposal failed
				// return the balance of applicant
				let _ = T::Currency::transfer(&Self::custody_account(),  &proposal.applicant, tribute_offered, AllowDeath);
			}

			// need to mutate for update
			Proposals::<T>::insert(proposal_id, proposal.clone());

			// send reward
			let _ = T::Currency::transfer(&Self::account_id(), &who, ProcessingReward::<T>::get(), KeepAlive);
			// return deposit with reward slashed
			let rest_balance = ProposalDeposit::<T>::get() - ProcessingReward::<T>::get();
			let _ = T::Currency::transfer(&Self::account_id(), &proposal.proposer, rest_balance, KeepAlive);			

			Self::deposit_event(RawEvent::ProcessProposal(
				proposal_index, 
				proposal.applicant.clone(),
				proposal.proposer.clone(),
				proposal.tribute_offered,
				proposal.shares_requested,
				did_pass
			));
			Ok(())
		}

		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn process_guild_kick_proposal(origin, proposal_index: u128) -> dispatch::DispatchResult {
			let who = ensure_signed(origin)?;
			let proposal_len = ProposalQueue::get().len();
			ensure!(proposal_index < proposal_len.try_into().unwrap(), Error::<T>::ProposalNotExist);
			let _usize_proposal_index = TryInto::<usize>::try_into(proposal_index).ok().unwrap();
			let proposal_id = ProposalQueue::get()[_usize_proposal_index];
			let proposal = &mut Proposals::<T>::get(proposal_id);
			// ensure guild kick proposal
			ensure!(proposal.flags[5], Error::<T>::NotKickProposal);
			ensure!(
				Self::get_current_period() - VotingPeriodLength::get() - GracePeriodLength::get() >= proposal.starting_period,
				Error::<T>::ProposalNotReady
			);
			ensure!(proposal.flags[1] == false, Error::<T>::ProposalHasProcessed);
			ensure!(proposal_index == 0 || Proposals::<T>::get(ProposalQueue::get()[_usize_proposal_index - 1]).flags[1],
			        Error::<T>::PreviousProposalNotProcessed);

			proposal.flags[1] = true;
			let did_pass = Self::should_pass(Proposals::<T>::get(proposal_id));
			if did_pass {
				// mark did_pass to true
				proposal.flags[2] = true;
				// update memeber status, i.e. jailed and slash shares
				Members::<T>::mutate(proposal.applicant.clone(), |member| {
					member.jailed_at = proposal_index;
					member.loot = member.loot.checked_add(member.shares).unwrap();
					let total_shares = TotalShares::get().checked_sub(member.shares).unwrap();
					let total_loot = TotalLoot::get().checked_add(member.shares).unwrap();
					TotalLoot::put(total_loot);
					TotalShares::put(total_shares);
					member.shares = 0;
				});
			}

			ProsedToKick::<T>::insert(proposal.applicant.clone(), false);

			// send reward
			let _ = T::Currency::transfer(&Self::account_id(), &who, ProcessingReward::<T>::get(), KeepAlive);
			// return deposit with reward slashed
			let rest_balance = ProposalDeposit::<T>::get() - ProcessingReward::<T>::get();
			let _ = T::Currency::transfer(&Self::account_id(), &proposal.proposer, rest_balance, KeepAlive);			

			Ok(())
		}

		/// proposer abort a proposal
		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn abort(origin, proposal_index: u128) -> dispatch::DispatchResult {
			let who = ensure_signed(origin)?;
			let proposal = &mut Proposals::<T>::get(proposal_index);
			ensure!(who == proposal.proposer, Error::<T>::NotProposalProposer);
			ensure!(!proposal.flags[0], Error::<T>::ProposalHasSponsored);
			ensure!(!proposal.flags[3], Error::<T>::ProposalHasAborted);
			let token_to_abort = proposal.tribute_offered;
			proposal.tribute_offered = 0;
			proposal.flags[3] = true;

			// need to mutate for update
			Proposals::<T>::insert(proposal_index, proposal.clone());
			// return the token to applicant and delete record
			let _ = T::Currency::transfer(&Self::custody_account(),  &proposal.proposer, Self::u128_to_balance(token_to_abort), AllowDeath);

			Self::deposit_event(RawEvent::Abort(proposal_index, who.clone()));
			Ok(())
		}

		/// Member rage quit
		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn rage_quit(origin, shares_to_burn: u128, loot_to_burn: u128) -> dispatch::DispatchResult {
			let who = ensure_signed(origin)?;
			Self::member_quit(who, shares_to_burn, loot_to_burn)
		}

		#[weight = 10_000 + T::DbWeight::get().reads_writes(1,1)]
		pub fn rage_kick(origin, member_to_kick: T::AccountId) -> dispatch::DispatchResult {
			let _ = ensure_signed(origin)?;
			let member = Members::<T>::get(member_to_kick.clone());
			ensure!(member.jailed_at != 0, Error::<T>::MemberNotInJail);
			ensure!(member.loot > 0, Error::<T>::NoEnoughLoot);
			Self::member_quit(member_to_kick, 0, member.loot)
		}
	}
}

impl<T: Config> Module<T> {
	// Add public immutables and private mutables.

	/// refer https://github.com/paritytech/substrate/blob/743accbe3256de2fc615adcaa3ab03ebdbbb4dbd/frame/treasury/src/lib.rs#L351
	///
	/// This actually does computation. If you need to keep using it, then make sure you cache the
	/// value and only call this once.
	pub fn account_id() -> T::AccountId {
		T::ModuleId::get().into_account()
	}

	pub fn custody_account() -> T::AccountId {
		T::ModuleId::get().into_sub_account("custody")
	}

	pub fn u128_to_balance(cost: u128) -> BalanceOf<T> {
		TryInto::<BalanceOf::<T>>::try_into(cost).ok().unwrap()
	}

	pub fn balance_to_u128(balance: BalanceOf<T>) -> u128 {
		TryInto::<u128>::try_into(balance).ok().unwrap()
	}

	pub fn get_current_period() -> u128 {
		let now = TryInto::<u128>::try_into(pallet_timestamp::Module::<T>::now()).ok().unwrap();
		let summon_time = TryInto::<u128>::try_into(SummonTime::<T>::get()).ok().unwrap();
		let diff = now.checked_sub(summon_time).unwrap();
		// the timestamp is in milli seconds
		diff.checked_div(1000).unwrap().checked_div(PeriodDuration::get().into()).unwrap()
	}

	pub fn create_proposal(
		proposer: T::AccountId,
		applicant: T::AccountId,
		shares_requested: u128,
		loot_requested: u128,
		tribute_offered: u128,
		payment_requested: u128,
		details: Vec<u8>,
		flags: [bool; 6]
	) {
			let proposal_index = ProposalCount::get();
			let proposal = Proposal {
				proposer: proposer.clone(),
				applicant: applicant.clone(),
				sponsor: proposer.clone(),
				shares_requested: shares_requested,
				starting_period: 0,
				yes_votes: 0,
				no_votes: 0,
				details: details,
				max_total_shares_at_yes: 0,
				loot_requested: loot_requested,
				tribute_offered: tribute_offered,
				payment_requested: payment_requested,
				flags: flags
			};
			Proposals::<T>::insert(proposal_index, proposal);
			Self::deposit_event(RawEvent::SubmitProposal(proposal_index, proposer.clone(), proposer, applicant, tribute_offered, shares_requested));	
			ProposalCount::put(proposal_index + 1);
	}

	pub fn should_pass(proposal: ProposalOf<T>) -> bool {
		let mut pass = proposal.yes_votes > proposal.no_votes;
		// as anyone can process the proposal and get rewarded, so do not fail here
		if TotalShares::get().checked_mul(DilutionBound::get()).unwrap() < proposal.max_total_shares_at_yes {
			Self::deposit_event(RawEvent::DilutionBoundExeceeds(TotalShares::get(), DilutionBound::get(), proposal.max_total_shares_at_yes));
			pass = false;
		}

		if Members::<T>::get(proposal.applicant.clone()).jailed_at != 0 {
			pass = false;
		}
		pass
	}

	pub fn member_quit(who: T::AccountId, shares_to_burn: u128, loot_to_burn: u128) -> dispatch::DispatchResult {
		ensure!(Members::<T>::contains_key(who.clone()), Error::<T>::NotMember);
		let member = Members::<T>::get(who.clone());
		ensure!(member.shares >= shares_to_burn, Error::<T>::NoEnoughShares);
		// check if can rage quit
		let proposal_index = member.highest_index_yes_vote;
		ensure!(proposal_index < ProposalQueue::get().len().try_into().unwrap(), Error::<T>::ProposalNotExist);
		let _usize_proposal_index = TryInto::<usize>::try_into(proposal_index).ok().unwrap();
		let proposal_id = ProposalQueue::get()[_usize_proposal_index];
		let proposal =  Proposals::<T>::get(proposal_id);
		ensure!(proposal.flags[1], Error::<T>::ProposalNotProcessed);
			
		// burn shares and loot
		Members::<T>::mutate(who.clone(), |mem| {
			mem.shares = member.shares.checked_sub(shares_to_burn).unwrap();
			mem.loot = member.loot.checked_sub(loot_to_burn).unwrap();

		});
		let initial_total = TotalShares::get().checked_add(TotalLoot::get()).unwrap();
		let total_to_burn = shares_to_burn.checked_add(loot_to_burn).unwrap();
		let rest_shares = TotalShares::get().checked_sub(shares_to_burn).unwrap();
		TotalShares::put(rest_shares);
		let rest_loot = TotalLoot::get().checked_sub(loot_to_burn).unwrap();
		TotalLoot::put(rest_loot);

		// withdraw the tokens
		let amount = Self::balance_to_u128(T::Currency::free_balance(&Self::account_id()));
		let balance = amount.checked_mul(total_to_burn).unwrap().checked_div(initial_total).unwrap();
		let _ = T::Currency::transfer(&Self::account_id(), &who, Self::u128_to_balance(balance), KeepAlive);			

		Self::deposit_event(RawEvent::Ragequit(who.clone(), shares_to_burn));
		Ok(())
	}
}