use crate::{Error, mock::*};
use frame_support::{assert_ok, assert_noop};
use super::RawEvent;
use sp_std::convert::{TryInto};


fn last_event() -> RawEvent<u64, u64> {
	System::events().into_iter().map(|r| r.event)
		.filter_map(|e| {
			if let Event::moloch_v2(inner) = e { Some(inner) } else { None }
		})
		.last()
		.unwrap()
}

/// A helper function to summon moloch for each test case
fn summon_with(initial_member: u64) {
	// in seconds
	let period_duration = 10;
	let voting_period_length = 2;
	let grace_period_length = 2;
	let dilution_bound = 1;
	let proposal_deposit = 100;
	let processing_reward = 50;

	let _ = MolochV2::summon(
		Origin::signed(initial_member),
		period_duration,
		voting_period_length,
		grace_period_length,
		dilution_bound,
		proposal_deposit,
		processing_reward
	);
}

/// Simulate a scenario that a member is proposed in jail
fn put_in_jail(initial_member: u64, jailed_member: u64) {
	// initial a DAO first
	summon_with(initial_member);

	// submit a proposal
	let tribute_offered = 50;
	let shares_requested = 5;
	let loot_requested = 0;
	let payment_requested = 0;
	let detail = b"test_proposal".to_vec();
	let mut proposal_idx = 0;

	let _ = MolochV2::submit_proposal(
		Origin::signed(jailed_member), 
		jailed_member, 
		tribute_offered,
		shares_requested,
		loot_requested,
		payment_requested,
		detail.clone()
	);
	
	// sponsor it
	assert_ok!(MolochV2::sponsor_proposal(Origin::signed(initial_member), proposal_idx));
	// set the timestamp to make voting period effect
	let now = Timestamp::now();
	let period_duration = TryInto::<u64>::try_into(MolochV2::period_duration() * 1000 * 2).ok().unwrap();
	Timestamp::set_timestamp(now + period_duration);
	// vote yes
	assert_ok!(MolochV2::submit_vote(Origin::signed(initial_member), proposal_idx, 1));
	 
	// pass grace period
	Timestamp::set_timestamp(now + period_duration * 4);
	let processor = 0;
	assert_ok!(MolochV2::process_proposal(Origin::signed(processor), proposal_idx));
	
	// propose himself to kick
	assert_ok!(MolochV2::submit_guild_kick_proposal(Origin::signed(jailed_member), jailed_member, detail.clone()));
	proposal_idx = proposal_idx + 1;
	// sponsor it
	assert_ok!(MolochV2::sponsor_proposal(Origin::signed(initial_member), proposal_idx));
	// set the timestamp to make voting period effect
	let now = Timestamp::now();
	let period_duration = TryInto::<u64>::try_into(MolochV2::period_duration() * 1000 * 2).ok().unwrap();
	Timestamp::set_timestamp(now + period_duration);
	// vote yes
	assert_ok!(MolochV2::submit_vote(Origin::signed(initial_member), proposal_idx, 1));
	Timestamp::set_timestamp(now + period_duration * 4);
	let processor = 0;
	assert_ok!(MolochV2::process_guild_kick_proposal(Origin::signed(processor), proposal_idx));
	
}

#[test]
fn summon_works() {
	new_test_ext().execute_with(|| {
		// in seconds
		let period_duration = 10;
		let voting_period_length = 2;
		let grace_period_length = 2;
		let dilution_bound = 1;
		let proposal_deposit = 100;
		let processing_reward = 50;

		assert_ok!(MolochV2::summon(Origin::signed(1), period_duration, voting_period_length, grace_period_length, dilution_bound, proposal_deposit, processing_reward));
		// check the constants
		assert_eq!(MolochV2::period_duration(), period_duration);
		assert_eq!(MolochV2::voting_period_length(), voting_period_length);
		assert_eq!(MolochV2::grace_period_length(), grace_period_length);
		assert_eq!(MolochV2::dilution_bound(), dilution_bound);
		assert_eq!(MolochV2::proposal_deposit(), proposal_deposit);
		assert_eq!(MolochV2::processing_reward(), processing_reward);

		// check the shares and member
		assert_eq!(MolochV2::totoal_shares(), 1);
		assert_eq!(MolochV2::members(1).exists, true);
	});
}

#[test]
fn summon_failed_validation() {
	new_test_ext().execute_with(|| {
		// in seconds
		let period_duration = 10;
		let voting_period_length = 2;
		let grace_period_length = 2;
		let dilution_bound = 1;
		let proposal_deposit = 100;
		let processing_reward = 150;

		// When reward is greater than deposit, should fail
		assert_noop!(
			MolochV2::summon(
				Origin::signed(1),
				period_duration,
				voting_period_length,
				grace_period_length,
				dilution_bound,
				proposal_deposit,
				processing_reward
			),
			Error::<Test>::NoEnoughProposalDeposit
		);
	});
}

#[test]
fn submit_proposal_works() {
	new_test_ext().execute_with(|| {
		// IMPORTANT, event won't emit in block 0
		System::set_block_number(1);
		let initial_member = 1;
		summon_with(initial_member);

		// failed when member propose for applicant who did not deposit in custody account
		let tribute_offered = 50;
		let shares_requested = 5;
		let loot_requested = 0;
		let payment_requested = 0;
		let applicant = 2;
		let detail = b"test_proposal".to_vec();

		assert_ok!(
			MolochV2::submit_proposal(
				Origin::signed(1), 
				applicant, 
				tribute_offered,
				shares_requested,
				loot_requested,
				payment_requested,
				detail.clone()
			)
		);
		assert_eq!(last_event(), RawEvent::SubmitProposal(0, 1, 1, applicant, tribute_offered.into(), shares_requested));
	});
}

#[test]
fn add_member_works() {
	new_test_ext().execute_with(|| {
		// IMPORTANT, event won't emit in block 0
		System::set_block_number(1);
		let initial_member = 1;
		summon_with(initial_member);

		// submit a proposal
		let tribute_offered = 50;
		let shares_requested = 5;
		let loot_requested = 0;
		let payment_requested = 0;
		let applicant = 2;
		let detail = b"test_proposal".to_vec();

		// a non-member can submit
		assert_ok!(
			MolochV2::submit_proposal(
				Origin::signed(applicant), 
				applicant, 
				tribute_offered, 
				shares_requested, 
				loot_requested,
				payment_requested, 
				detail
			)
		);
		// need to be sponsored 
		assert_ok!(MolochV2::sponsor_proposal(Origin::signed(1), 0));

		// set the timestamp to make voting period effect
		let now = Timestamp::now();
		let period_duration = TryInto::<u64>::try_into(MolochV2::period_duration() * 1000 * 2).ok().unwrap();
		Timestamp::set_timestamp(now + period_duration);

		// vote yes
		assert_ok!(MolochV2::submit_vote(Origin::signed(1), 0, 1));
		 
		// pass grace period
		Timestamp::set_timestamp(now + period_duration * 4);
		let processor = 3;
		let balance_before = Balances::free_balance(processor);
		let processing_reward = MolochV2::processing_reward();
		assert_ok!(MolochV2::process_proposal(Origin::signed(processor), 0));
		// make sure the processor get rewarded
		assert_eq!(Balances::free_balance(processor), processing_reward + balance_before);

		// check the applicant has become a member
		assert_eq!(MolochV2::members(applicant).exists, true);
		
	});
}

#[test]
fn vote_failed_validation() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let initial_member = 1;
		summon_with(initial_member);

		// submit a proposal
		let tribute_offered = 50;
		let shares_requested = 5;
		let loot_requested = 0;
		let payment_requested = 0;
		let applicant = 2;
		let detail = b"test_proposal".to_vec();

		// a non-member can submit
		assert_ok!(
			MolochV2::submit_proposal(
				Origin::signed(applicant), 
				applicant, 
				tribute_offered, 
				shares_requested, 
				loot_requested,
				payment_requested, 
				detail
			)
		);

		// only member can vote
		assert_noop!(
			MolochV2::submit_vote(Origin::signed(2), 0, 1),
			Error::<Test>::NotMember
		);

		// can not vote a proposal until it's sponsored
		assert_noop!(
			MolochV2::submit_vote(Origin::signed(initial_member), 0, 1),
			Error::<Test>::ProposalNotExist
		);

		// sponsor it and continue to vote
		assert_ok!(MolochV2::sponsor_proposal(Origin::signed(initial_member), 0));

		// can not vote a proposal until it's in voting period
		assert_noop!(
			MolochV2::submit_vote(Origin::signed(initial_member), 0, 1),
			Error::<Test>::ProposalNotStart
		);
	});
}

#[test]
fn guild_kick_works() {
	new_test_ext().execute_with(|| {
		let initial_member = 1;
		let jailed_member = 2;
		put_in_jail(initial_member, jailed_member);
		// make sure the member is in jail
		assert_eq!(MolochV2::members(jailed_member).exists, true);
		assert_eq!(MolochV2::members(jailed_member).jailed_at > 0, true);
	});
}

#[test]
fn guild_member_failed() {
	new_test_ext().execute_with(|| {
		let initial_member = 1;
		let jailed_member = 2;
		put_in_jail(initial_member, jailed_member);
		// submit a proposal
		let tribute_offered = 50;
		let shares_requested = 5;
		let loot_requested = 0;
		let payment_requested = 0;
		let applicant = 3;
		let detail = b"test_proposal".to_vec();

		// a non-member can submit
		assert_ok!(
			MolochV2::submit_proposal(
				Origin::signed(applicant), 
				applicant, 
				tribute_offered, 
				shares_requested, 
				loot_requested,
				payment_requested, 
				detail
			)
		);

		// member in jailed can not sponsor
		// NOTE: as our helper functions have submitted 2 proposal, thus ours index is 2
		assert_noop!(
			MolochV2::sponsor_proposal(Origin::signed(jailed_member), 2),
			Error::<Test>::MemberInJail
		);
	});
}

#[test]
fn abort_works() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let initial_member = 1;
		summon_with(initial_member);

		// submit a proposal
		let tribute_offered = 50;
		let shares_requested = 5;
		let loot_requested = 0;
		let payment_requested = 0;
		let applicant = 2;
		let detail = b"test_proposal".to_vec();

		// a non-member can submit
		assert_ok!(
			MolochV2::submit_proposal(
				Origin::signed(applicant), 
				applicant, 
				tribute_offered, 
				shares_requested, 
				loot_requested,
				payment_requested, 
				detail
			)
		);

		assert_ok!(MolochV2::abort(Origin::signed(applicant), 0));
	});
}

#[test]
fn abort_failed() {
	new_test_ext().execute_with(|| {
		System::set_block_number(1);
		let initial_member = 1;
		let naughty_boy = 0;
		summon_with(initial_member);

		// submit a proposal
		let tribute_offered = 50;
		let shares_requested = 5;
		let loot_requested = 0;
		let payment_requested = 0;
		let applicant = 2;
		let detail = b"test_proposal".to_vec();

		// a non-member can submit
		assert_ok!(
			MolochV2::submit_proposal(
				Origin::signed(applicant), 
				applicant, 
				tribute_offered, 
				shares_requested, 
				loot_requested,
				payment_requested, 
				detail
			)
		);

		// only proposer can abort
		assert_noop!(
			MolochV2::abort(Origin::signed(naughty_boy), 0),
			Error::<Test>::NotProposalProposer
		);

	});
}

#[test]
fn rage_kick_works() {
	new_test_ext().execute_with(|| {
		let initial_member = 1;
		let jailed_member = 2;
		put_in_jail(initial_member, jailed_member);
		assert_ok!(MolochV2::rage_kick(Origin::signed(0), jailed_member));
	});
}

#[test]
fn rage_kick_failed() {
	new_test_ext().execute_with(|| {
		// add a member and not put in jail, 1=initial_member, 2=new_member
		add_member_works();
		// anyone can call rage_kick
		assert_noop!(
			MolochV2::rage_kick(Origin::signed(0), 2),
			Error::<Test>::MemberNotInJail
		);
		
	});
}

#[test]
fn update_delegate_validation() {
	new_test_ext().execute_with(|| {
		let initial_member = 1;
		let delegate = 3;
		summon_with(initial_member);
		// anyone can call rage_kick
		assert_noop!(
			MolochV2::rage_kick(Origin::signed(0), 2),
			Error::<Test>::MemberNotInJail
		);
		// submit a proposal
		let tribute_offered = 50;
		let shares_requested = 5;
		let loot_requested = 0;
		let payment_requested = 0;
		let applicant = 2;
		let detail = b"test_proposal".to_vec();
		let proposal_idx = 0;

		// a non-member can submit
		assert_ok!(
			MolochV2::submit_proposal(
				Origin::signed(applicant), 
				applicant, 
				tribute_offered, 
				shares_requested, 
				loot_requested,
				payment_requested, 
				detail
			)
		);
		// sponsor it
		assert_ok!(MolochV2::sponsor_proposal(Origin::signed(initial_member), proposal_idx));
		// set the timestamp to make voting period effect
		let now = Timestamp::now();
		let period_duration = TryInto::<u64>::try_into(MolochV2::period_duration() * 1000 * 2).ok().unwrap();
		Timestamp::set_timestamp(now + period_duration);
		
		// change delegate
		assert_ok!(MolochV2::update_delegate(Origin::signed(initial_member), delegate));
		// negative case as the voting rights have been delegated
		assert_noop!(
			MolochV2::submit_vote(Origin::signed(initial_member), proposal_idx, 1),
			Error::<Test>::NotMember
		);
		// positive case, use delegate to vote
		assert_ok!(MolochV2::submit_vote(Origin::signed(delegate), proposal_idx, 1));
	});
}