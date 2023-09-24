use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};

use axum::{extract::ws::Message, Error};
use matchbox_protocol::{PeerId, RoomId};
use matchbox_signaling::{
    common_logic::{self, StateObj},
    SignalingError, SignalingState,
};
use serde::Deserialize;
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RequestedRoom {
    pub id: Option<RoomId>,
}

#[derive(Debug, Clone)]
pub(crate) struct Peer {
    pub uuid: PeerId,
    pub requested_room: RequestedRoom,
    pub room: Option<RoomId>,
    pub sender: UnboundedSender<Result<Message, Error>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Room {
    pub id: RoomId,
    pub peers: HashSet<PeerId>,
    pub host: PeerId,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct ActiveState {
    clients: HashMap<PeerId, Peer>,
    rooms: HashMap<RoomId, Room>,
}

#[derive(Default, Debug, Clone)]
pub(crate) struct ServerState {
    clients_waiting: StateObj<HashMap<SocketAddr, RequestedRoom>>,
    clients_in_queue: StateObj<HashMap<PeerId, RequestedRoom>>,
    active_state: StateObj<ActiveState>,
}

impl SignalingState for ServerState {}

impl ServerState {
    /// Add a waiting client to matchmaking
    pub fn add_waiting_client(&mut self, origin: SocketAddr, room: RequestedRoom) {
        self.clients_waiting.lock().unwrap().insert(origin, room);
    }

    /// Assign a peer id to a waiting client
    pub fn assign_id_to_waiting_client(&mut self, origin: SocketAddr, peer_id: PeerId) {
        let room = {
            let mut lock = self.clients_waiting.lock().unwrap();
            lock.remove(&origin).expect("waiting client")
        };
        {
            let mut lock = self.clients_in_queue.lock().unwrap();
            lock.insert(peer_id, room);
        }
    }

    /// Remove the waiting peer, returning the peer's requested room
    pub fn remove_waiting_peer(&mut self, peer_id: PeerId) -> RequestedRoom {
        let room = {
            let mut lock = self.clients_in_queue.lock().unwrap();
            lock.remove(&peer_id).expect("waiting peer")
        };
        room
    }

    /// Add a peer, returning the room the peer was added to
    pub fn add_peer(&mut self, mut peer: Peer) -> RoomId {
        let mut state = self.active_state.lock().unwrap();
        let peer_id = peer.uuid;
        let requested_room = peer.requested_room.clone();

        let room_id = requested_room
            .id
            .unwrap_or_else(|| RoomId(Uuid::new_v4().to_string()));
        {
            let room = state.rooms.entry(room_id.clone()).or_insert_with(|| {
                debug!("Room added: {room_id:?}");
                Room {
                    id: room_id.clone(),
                    peers: Default::default(),
                    host: peer_id,
                }
            });
            let room_id = &room.id;
            debug!("Peer added to room: {peer_id:?} / {room_id:?}");
            room.peers.insert(peer_id);
        }

        peer.room = Some(room_id.clone());
        state.clients.insert(peer.uuid, peer);
        debug!("Peer added: {peer_id:?}");

        room_id
    }

    /// Get a peer
    pub fn get_peer(&self, peer_id: &PeerId) -> Option<Peer> {
        self.active_state
            .lock()
            .unwrap()
            .clients
            .get(peer_id)
            .cloned()
    }

    /// Get the peers in a room currently
    pub fn get_room_peers(&self, room_id: &RoomId) -> Vec<PeerId> {
        self.active_state
            .lock()
            .unwrap()
            .rooms
            .get(room_id)
            .map(|room| room.peers.iter().copied().collect::<Vec<PeerId>>())
            .unwrap_or_default()
    }

    ///
    pub fn get_room_host_peer(&self, room_id: &RoomId) -> Option<PeerId> {
        self.active_state
            .lock()
            .unwrap()
            .rooms
            .get(room_id)
            .map(|room| room.host)
    }

    ///
    pub fn is_peer_host(&self, peer: &PeerId, room_id: &RoomId) -> bool {
        self.active_state
            .lock()
            .unwrap()
            .rooms
            .get(room_id)
            .map(|room| room.host == *peer)
            .unwrap_or(false)
    }

    /// Remove a peer from the state if it existed, returning the peer removed.
    #[must_use]
    pub fn remove_peer(&mut self, peer_id: &PeerId) -> Option<Peer> {
        let mut state = { self.active_state.lock().unwrap() };
        let peer = { state.clients.remove(peer_id) };

        if let Some(peer) = peer.as_ref() {
            debug!("Peer removed: {peer_id:?}");
            if let Some(room_id) = &peer.room {
                debug!("Peer removed from room: {peer_id:?} / {room_id:?}");
                // Best effort to remove peer from their room
                _ = state
                    .rooms
                    .get_mut(room_id)
                    .map(|room| room.peers.remove(peer_id));
            }
        }
        peer
    }

    pub fn remove_room(&mut self, room_id: &RoomId) -> Option<Room> {
        let mut state = self.active_state.lock().unwrap();

        let room = state.rooms.remove(room_id);

        if let Some(room) = room.as_ref() {
            debug!("Room removed: {room_id:?}");
            for peer in &room.peers {
                if let Some(peer) = state.clients.get_mut(peer) {
                    peer.room = None;
                }
            }
        } else {
            debug!("Room not removed, doesn't exist: {room_id:?}");
        }

        room
    }

    /// Send a message to a peer without blocking.
    pub fn try_send(&self, id: &PeerId, message: Message) -> Result<(), SignalingError> {
        let clients = &self.active_state.lock().unwrap().clients;
        match clients.get(id) {
            Some(peer) => Ok(common_logic::try_send(&peer.sender, message)?),
            None => Err(SignalingError::UnknownPeer),
        }
    }
}
