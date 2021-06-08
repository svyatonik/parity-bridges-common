// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives of messages module.

#![cfg_attr(not(feature = "std"), no_std)]
// RuntimeApi generated functions
#![allow(clippy::too_many_arguments)]
// Generated by `DecodeLimit::decode_with_depth_limit`
#![allow(clippy::unnecessary_mut_passed)]

use codec::{Decode, Encode};
use frame_support::RuntimeDebug;
use sp_std::{collections::vec_deque::VecDeque, prelude::*};

pub mod source_chain;
pub mod target_chain;

// Weight is reexported to avoid additional frame-support dependencies in related crates.
pub use frame_support::weights::Weight;

/// Messages pallet operating mode.
#[derive(Encode, Decode, Clone, Copy, PartialEq, Eq, RuntimeDebug)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum OperatingMode {
	/// Normal mode, when all operations are allowed.
	Normal,
	/// The pallet is not accepting outbound messages. Inbound messages and receival proofs
	/// are still accepted.
	///
	/// This mode may be used e.g. when bridged chain expects upgrade. Then to avoid dispatch
	/// failures, the pallet owner may stop accepting new messages, while continuing to deliver
	/// queued messages to the bridged chain. Once upgrade is completed, the mode may be switched
	/// back to `Normal`.
	RejectingOutboundMessages,
	/// The pallet is halted. All operations (except operating mode change) are prohibited.
	Halted,
}

impl Default for OperatingMode {
	fn default() -> Self {
		OperatingMode::Normal
	}
}

/// Messages pallet parameter.
pub trait Parameter: frame_support::Parameter {
	/// Save parameter value in the runtime storage.
	fn save(&self);
}

/// Lane identifier.
pub type LaneId = [u8; 4];

/// Message nonce. Valid messages will never have 0 nonce.
pub type MessageNonce = u64;

/// Message id as a tuple.
pub type MessageId = (LaneId, MessageNonce);

/// Opaque message payload. We only decode this payload when it is dispatched.
pub type MessagePayload = Vec<u8>;

/// Message key (unique message identifier) as it is stored in the storage.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct MessageKey {
	/// ID of the message lane.
	pub lane_id: LaneId,
	/// Message nonce.
	pub nonce: MessageNonce,
}

/// Message data as it is stored in the storage.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct MessageData<Fee> {
	/// Message payload.
	pub payload: MessagePayload,
	/// Message delivery and dispatch fee, paid by the submitter.
	pub fee: Fee,
}

/// Message as it is stored in the storage.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug)]
pub struct Message<Fee> {
	/// Message key.
	pub key: MessageKey,
	/// Message data.
	pub data: MessageData<Fee>,
}

/// Inbound lane data.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct InboundLaneData<RelayerId> {
	/// Identifiers of relayers and messages that they have delivered to this lane (ordered by message nonce).
	///
	/// This serves as a helper storage item, to allow the source chain to easily pay rewards
	/// to the relayers who succesfuly delivered messages to the target chain (inbound lane).
	///
	/// It is guaranteed to have at most N entries, where N is configured at the module level.
	/// If there are N entries in this vec, then:
	/// 1) all incoming messages are rejected if they're missing corresponding `proof-of(outbound-lane.state)`;
	/// 2) all incoming messages are rejected if `proof-of(outbound-lane.state).last_delivered_nonce` is
	///    equal to `self.last_confirmed_nonce`.
	/// Given what is said above, all nonces in this queue are in range:
	/// `(self.last_confirmed_nonce; self.last_delivered_nonce()]`.
	///
	/// When a relayer sends a single message, both of MessageNonces are the same.
	/// When relayer sends messages in a batch, the first arg is the lowest nonce, second arg the highest nonce.
	/// Multiple dispatches from the same relayer are allowed.
	pub relayers: VecDeque<(MessageNonce, MessageNonce, RelayerId)>,

	/// Nonce of the last message that
	/// a) has been delivered to the target (this) chain and
	/// b) the delivery has been confirmed on the source chain
	///
	/// that the target chain knows of.
	///
	/// This value is updated indirectly when an `OutboundLane` state of the source
	/// chain is received alongside with new messages delivery.
	pub last_confirmed_nonce: MessageNonce,
}

impl<RelayerId> Default for InboundLaneData<RelayerId> {
	fn default() -> Self {
		InboundLaneData {
			relayers: VecDeque::new(),
			last_confirmed_nonce: 0,
		}
	}
}

impl<RelayerId> InboundLaneData<RelayerId> {
	/// Returns approximate size of the struct, given number of entries in the `relayers` set and
	/// size of each entry.
	///
	/// Returns `None` if size overflows `u32` limits.
	pub fn encoded_size_hint(relayer_id_encoded_size: u32, relayers_entries: u32) -> Option<u32> {
		let message_nonce_size = 8;
		let relayers_entry_size = relayer_id_encoded_size.checked_add(2 * message_nonce_size)?;
		let relayers_size = relayers_entries.checked_mul(relayers_entry_size)?;
		relayers_size.checked_add(message_nonce_size)
	}

	/// Nonce of the last message that has been delivered to this (target) chain.
	pub fn last_delivered_nonce(&self) -> MessageNonce {
		self.relayers
			.back()
			.map(|(_, last_nonce, _)| *last_nonce)
			.unwrap_or(self.last_confirmed_nonce)
	}
}

/// Message details, returned by runtime APIs.
#[derive(Clone, Default, Encode, Decode, RuntimeDebug, PartialEq, Eq)]
pub struct MessageDetails<OutboundMessageFee> {
	/// Nonce assigned to the message.
	pub nonce: MessageNonce,
	/// Message dispatch weight, declared by the submitter.
	pub dispatch_weight: Weight,
	/// Size of the encoded message.
	pub size: u32,
	/// Delivery+dispatch fee paid by the message submitter at the source chain.
	pub delivery_and_dispatch_fee: OutboundMessageFee,
	/// TODO: replace me with `DispatchFeePayment` from #911.
	pub dispatch_fee_payment: bool,
}

/// Gist of `InboundLaneData::relayers` field used by runtime APIs.
#[derive(Clone, Default, Encode, Decode, RuntimeDebug, PartialEq, Eq)]
pub struct UnrewardedRelayersState {
	/// Number of entries in the `InboundLaneData::relayers` set.
	pub unrewarded_relayer_entries: MessageNonce,
	/// Number of messages in the oldest entry of `InboundLaneData::relayers`. This is the
	/// minimal number of reward proofs required to push out this entry from the set.
	pub messages_in_oldest_entry: MessageNonce,
	/// Total number of messages in the relayers vector.
	pub total_messages: MessageNonce,
}

/// Outbound lane data.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct OutboundLaneData {
	/// Nonce of oldest message that we haven't yet pruned. May point to not-yet-generated message if
	/// all sent messages are already pruned.
	pub oldest_unpruned_nonce: MessageNonce,
	/// Nonce of latest message, received by bridged chain.
	pub latest_received_nonce: MessageNonce,
	/// Nonce of latest message, generated by us.
	pub latest_generated_nonce: MessageNonce,
}

impl Default for OutboundLaneData {
	fn default() -> Self {
		OutboundLaneData {
			// it is 1 because we're pruning everything in [oldest_unpruned_nonce; latest_received_nonce]
			oldest_unpruned_nonce: 1,
			latest_received_nonce: 0,
			latest_generated_nonce: 0,
		}
	}
}

/// Returns total number of messages in the `InboundLaneData::relayers` vector.
///
/// Returns `None` if there are more messages that `MessageNonce` may fit (i.e. `MessageNonce + 1`).
pub fn total_unrewarded_messages<RelayerId>(
	relayers: &VecDeque<(MessageNonce, MessageNonce, RelayerId)>,
) -> Option<MessageNonce> {
	match (relayers.front(), relayers.back()) {
		(Some((begin, _, _)), Some((_, end, _))) => {
			if let Some(difference) = end.checked_sub(*begin) {
				difference.checked_add(1)
			} else {
				Some(0)
			}
		}
		_ => Some(0),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn total_unrewarded_messages_does_not_overflow() {
		assert_eq!(
			total_unrewarded_messages(
				&vec![(0, 0, 1), (MessageNonce::MAX, MessageNonce::MAX, 2)]
					.into_iter()
					.collect()
			),
			None,
		);
	}

	#[test]
	fn inbound_lane_data_returns_correct_hint() {
		let expected_size = InboundLaneData::<u8>::encoded_size_hint(1, 13);
		let actual_size = InboundLaneData {
			relayers: (1u8..=13u8).map(|i| (i as _, i as _, i)).collect(),
			last_confirmed_nonce: 13,
		}
		.encode()
		.len();
		let difference = (expected_size.unwrap() as f64 - actual_size as f64).abs();
		assert!(
			difference / (std::cmp::min(actual_size, expected_size.unwrap() as usize) as f64) < 0.1,
			"Too large difference between actual ({}) and expected ({:?}) inbound lane data size",
			actual_size,
			expected_size,
		);
	}
}
