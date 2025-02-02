use axum::{http::StatusCode, response::IntoResponse, routing::get};
use clap::Parser;
use matchbox_signaling::SignalingServerBuilder;
use tracing::info;
use tracing_subscriber::prelude::*;
use matchbox_protocol::RoomId;

use crate::args::Args;
use crate::{
    state::{RequestedRoom, ServerState},
    topology::MatchmakingDemoTopology,
};

mod args;
mod state;
mod topology;

fn setup_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "matchbox_server=info,tower_http=debug".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .compact()
                .with_file(false)
                .with_target(false),
        )
        .init();
}

#[tokio::main]
async fn main() {
    setup_logging();
    let args = Args::parse();

    // Setup router
    info!("Matchbox Signaling Server: {}", args.host);

    let mut state = ServerState::default();
    let server = SignalingServerBuilder::new(args.host, MatchmakingDemoTopology, state.clone())
        .on_connection_request({
            let mut state = state.clone();
            move |connection| {
                let room_id = connection.path.clone().map(|path| RoomId(path));
                let room = RequestedRoom { id: room_id };
                state.add_waiting_client(connection.origin, room);
                Ok(true) // allow all clients
            }
        })
        .on_id_assignment({
            move |(origin, peer_id)| {
                info!("Client connected {origin:?}: {peer_id:?}");
                state.assign_id_to_waiting_client(origin, peer_id);
            }
        })
        .cors()
        .trace()
        .mutate_router(|router| {
            // Apply router transformations
            router.route("/health", get(|| async { StatusCode::OK }))
        })
        .build();
    server
        .serve()
        .await
        .expect("Unable to run signaling server, is it already running?")
}

pub async fn health_handler() -> impl IntoResponse {
    StatusCode::OK
}
