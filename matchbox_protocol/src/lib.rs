use cfg_if::cfg_if;
use derive_more::{Display, From};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The format for a peer signature given by the signaling server
#[derive(
    Debug, Display, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, From, Hash, PartialOrd, Ord,
)]
pub struct PeerId(pub Uuid);

/// Requests go from peer to signaling server
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PeerRequest<S> {
    Signal { receiver: PeerId, data: S },
    KeepAlive,
}

/// Events go from signaling server to peer
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PeerEvent<S> {
    /// Sent by the server to the connecting peer, immediately after connection
    /// before any other events
    IdAssigned(PeerId),
    NewPeer(PeerId),
    PeerLeft(PeerId),
    Signal {
        sender: PeerId,
        data: S,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SignalEvent<S> {
    Peer(PeerEvent<S>),
    /// Id of new room created
    RoomOpened(String),
    /// Host has left
    RoomClosed,
    /// If we are the host
    HostStatus(bool),
    /// Arbitrary data (just in case)
    Data(Vec<u8>),
}

cfg_if! {
    if #[cfg(feature = "json")] {
        pub type JsonPeerRequest = PeerRequest<serde_json::Value>;
        pub type JsonSignalEvent = SignalEvent<serde_json::Value>;

        impl ToString for JsonPeerRequest {
            fn to_string(&self) -> String {
                serde_json::to_string(self).expect("error serializing message")
            }
        }
        impl std::str::FromStr for JsonPeerRequest {
            type Err = serde_json::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                serde_json::from_str(s)
            }
        }

        impl ToString for JsonSignalEvent {
            fn to_string(&self) -> String {
                serde_json::to_string(self).expect("error serializing message")
            }
        }
        impl std::str::FromStr for JsonSignalEvent {
            type Err = serde_json::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                serde_json::from_str(s)
            }
        }
    }
}
