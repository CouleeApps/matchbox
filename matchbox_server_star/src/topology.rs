use async_trait::async_trait;
use axum::extract::ws::Message;
use futures::StreamExt;
use matchbox_protocol::{JsonSignalEvent, PeerEvent, PeerRequest};
use matchbox_signaling::{
    common_logic::parse_request, ClientRequestError, NoCallbacks, SignalingTopology, WsStateMeta,
};
use tracing::{error, info, warn};

use crate::state::{Peer, ServerState};

#[derive(Debug, Default)]
pub struct MatchmakingDemoTopology;

#[async_trait]
impl SignalingTopology<NoCallbacks, ServerState> for MatchmakingDemoTopology {
    async fn state_machine(upgrade: WsStateMeta<NoCallbacks, ServerState>) {
        let WsStateMeta {
            peer_id,
            sender,
            mut receiver,
            mut state,
            ..
        } = upgrade;

        let room = state.remove_waiting_peer(peer_id);
        let peer = Peer {
            uuid: peer_id,
            sender: sender.clone(),
            requested_room: room,
            room: None,
        };

        let room_id = state.add_peer(peer);

        let event_text = JsonSignalEvent::Peer(PeerEvent::NewPeer(peer_id)).to_string();
        let event = Message::Text(event_text.clone());

        if state.is_peer_host(&peer_id, &room_id) {
            // New room, but we're the host. Announce to everyone I guess?
            // Could there even be people here?

            for peer_id in state.get_room_peers(&room_id) {
                if let Err(e) = state.try_send(peer_id, event.clone()) {
                    error!("error sending to {peer_id:?}: {e:?}");
                } else {
                    info!("{peer_id} -> {event_text:?}");
                }
            }
        } else {
            // Existing room, tell the host we've joined
            if let Some(host_id) = state.get_room_host_peer(&room_id) {
                if let Err(e) = state.try_send(host_id, event.clone()) {
                    error!("error sending to {host_id:?}: {e:?}");
                } else {
                    info!("{host_id} -> {event_text:?}");
                }
            }
        }

        // The state machine for the data channel established for this websocket.
        while let Some(request) = receiver.next().await {
            let request = match parse_request(request) {
                Ok(request) => request,
                Err(e) => {
                    match e {
                        ClientRequestError::Axum(_) => {
                            // Most likely a ConnectionReset or similar.
                            warn!("Unrecoverable error with {peer_id:?}: {e:?}");
                            break;
                        }
                        ClientRequestError::Close => {
                            info!("Connection closed by {peer_id:?}");
                            break;
                        }
                        ClientRequestError::Json(_) | ClientRequestError::UnsupportedType(_) => {
                            error!("Error with request: {:?}", e);
                            continue; // Recoverable error
                        }
                    };
                }
            };

            match request {
                PeerRequest::Signal { receiver, data } => {
                    let event = Message::Text(
                        JsonSignalEvent::Peer(PeerEvent::Signal {
                            sender: peer_id,
                            data,
                        })
                        .to_string(),
                    );
                    if let Some(peer) = state.get_peer(&receiver) {
                        if let Err(e) = peer.sender.send(Ok(event)) {
                            error!("error sending signal event: {e:?}");
                        }
                    } else {
                        warn!("peer not found ({receiver:?}), ignoring signal");
                    }
                }
                PeerRequest::KeepAlive => {
                    // Do nothing. KeepAlive packets are used to protect against idle websocket
                    // connections getting automatically disconnected, common for reverse proxies.
                }
            }
        }

        // Peer disconnected or otherwise ended communication.
        info!("Removing peer: {:?}", peer_id);
        if let Some(removed_peer) = state.remove_peer(&peer_id) {
            if let Some(room_id) = removed_peer.room {
                if state.is_peer_host(&peer_id, &room_id) {
                    // Tell everyone the host has left
                    let other_peers = state
                        .get_room_peers(&room_id)
                        .into_iter()
                        .filter(|other_id| *other_id != peer_id);
                    let event =
                        Message::Text(JsonSignalEvent::Peer(PeerEvent::PeerLeft(removed_peer.uuid)).to_string());
                    for peer_id in other_peers {
                        match state.try_send(peer_id, event.clone()) {
                            Ok(()) => info!("Sent host peer remove to: {:?}", peer_id),
                            Err(e) => error!("Failure sending host peer remove: {e:?}"),
                        }
                    }
                } else {
                    // Tell just the host that someone has left (host gets to tell everyone else)
                    let event =
                        Message::Text(JsonSignalEvent::Peer(PeerEvent::PeerLeft(removed_peer.uuid)).to_string());
                    if let Some(host_id) = state.get_room_host_peer(&room_id) {
                        match state.try_send(host_id.clone(), event) {
                            Ok(()) => info!("Sent peer remove to host: {:?}", host_id),
                            Err(e) => error!("Failure sending peer remove to host: {e:?}"),
                        }
                    } else {
                        error!("Could not find host for room: {room_id:?}");
                    }
                }
            } else {
                error!("Peer {peer_id:?} is not connected to any room");
            }
        } else {
            error!("Could not remove peer {peer_id:?}");
        }
    }
}
