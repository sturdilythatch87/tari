// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use super::broadcast_strategy::BroadcastStrategy;
use crate::envelope::{DhtHeader, DhtMessageFlags, DhtMessageType, NodeDestination};
use std::fmt;
use tari_comms::{message::MessageFlags, peer_manager::PeerNodeIdentity, types::CommsPublicKey};

/// Determines if an outbound message should be Encrypted and, if so, for which public key
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundEncryption {
    /// Message should be encrypted using a shared secret derived from the given public key
    EncryptFor(CommsPublicKey),
    /// Message should be encrypted using a shared secret derived from the destination peer's
    /// public key. Each message sent according to the broadcast strategy will be encrypted for
    /// the destination peer.
    EncryptForDestination,
    /// Message should not be encrypted
    None,
}

impl OutboundEncryption {
    /// Return the correct DHT flags for the encryption setting
    pub fn flags(&self) -> DhtMessageFlags {
        match self {
            OutboundEncryption::EncryptFor(_) | OutboundEncryption::EncryptForDestination => DhtMessageFlags::ENCRYPTED,
            _ => DhtMessageFlags::NONE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SendMessageRequest {
    /// Broadcast strategy to use when sending the message
    pub broadcast_strategy: BroadcastStrategy,
    /// The intended destination for this message
    pub destination: NodeDestination,
    /// Encryption setting for message
    pub encryption: OutboundEncryption,
    /// Comms-level message flags
    pub comms_flags: MessageFlags,
    /// Dht-level message flags
    pub dht_flags: DhtMessageFlags,
    /// Dht-level message type (`DhtMessageType::None` for a non-DHT message)
    pub dht_message_type: DhtMessageType,
    /// Message body
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ForwardRequest {
    /// Broadcast strategy to use when forwarding the message
    pub broadcast_strategy: BroadcastStrategy,
    /// Original header from the origin
    pub dht_header: DhtHeader,
    /// Comms-level message flags
    pub comms_flags: MessageFlags,
    /// Message body
    pub body: Vec<u8>,
}

/// Represents a request to the DHT broadcast middleware
#[derive(Debug)]
pub enum DhtOutboundRequest {
    /// Send a message using the given broadcast strategy
    SendMsg(Box<SendMessageRequest>),
    /// Forward a message envelope
    Forward(Box<ForwardRequest>),
}

impl fmt::Display for DhtOutboundRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            DhtOutboundRequest::SendMsg(request) => write!(f, "SendMsg({})", request.broadcast_strategy),
            DhtOutboundRequest::Forward(request) => write!(f, "Forward({})", request.broadcast_strategy),
        }
    }
}

/// DhtOutboundMessage consists of the DHT and comms information required to
/// send a message
#[derive(Clone, Debug)]
pub struct DhtOutboundMessage {
    pub peer_node_identity: PeerNodeIdentity,
    pub dht_header: DhtHeader,
    pub comms_flags: MessageFlags,
    pub encryption: OutboundEncryption,
    pub body: Vec<u8>,
}

impl DhtOutboundMessage {
    /// Create a new DhtOutboundMessage
    pub fn new(
        peer_node_identity: PeerNodeIdentity,
        dht_header: DhtHeader,
        encryption: OutboundEncryption,
        comms_flags: MessageFlags,
        body: Vec<u8>,
    ) -> Self
    {
        Self {
            peer_node_identity,
            dht_header,
            encryption,
            comms_flags,
            body,
        }
    }
}