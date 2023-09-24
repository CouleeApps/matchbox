use futures::{select, FutureExt};
use futures_timer::Delay;
use log::Level::Debug;
use log::{debug, info};
use matchbox_socket::{Packet, PeerState, SignalEvent, WebRtcSocket};
#[cfg(target_arch = "wasm32")]
use std::future::pending;
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use tokio::io::{stdin, AsyncBufReadExt, BufReader};

#[cfg(target_arch = "wasm32")]
fn main() {
    // Setup logging
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).unwrap();

    wasm_bindgen_futures::spawn_local(async_main());
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    // Setup logging
    use tracing_subscriber::prelude::*;
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "unreliable_chatroom=info,matchbox_socket=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    async_main().await
}

async fn async_main() {
    info!("Connecting to matchbox");
    let (mut socket, loop_fut) = WebRtcSocket::builder("wss://ebfe.retf.cc:2053/room1")
        .add_unreliable_channel()
        .build();

    let mut unreliable_socket = socket.take_channel(0).unwrap();

    let loop_fut = loop_fut.fuse();
    futures::pin_mut!(loop_fut);

    let timeout = Delay::new(Duration::from_millis(100));
    futures::pin_mut!(timeout);

    #[cfg(not(target_arch = "wasm32"))]
    let mut stdin = BufReader::new(stdin());

    let mut is_host = false;
    'run: loop {
        // Handle any new peers
        for (peer, state) in socket.update_peers() {
            match state {
                PeerState::Connected => {
                    info!("Peer joined: {peer}");
                }
                PeerState::Disconnected => {
                    info!("Peer left: {peer}");
                }
            }
        }

        for msg in socket.update_signals() {
            match &msg {
                SignalEvent::Peer(_) => {}
                _ => debug!("Signal event: {msg:?}"),
            }
            match msg {
                SignalEvent::RoomOpened(room_id) => {
                    info!("Room opened! {room_id:?}");
                }
                SignalEvent::RoomClosed => {
                    info!("Room closed!");
                    break 'run;
                }
                SignalEvent::HostStatus(status) => {
                    info!("Host status: {status}");
                    is_host = status;
                }
                SignalEvent::Data(data) => {
                    info!("Signal data: {data:?}");
                }
                SignalEvent::Peer(_) => {}
            }
        }

        // Accept any messages incoming
        for (peer, packet) in unreliable_socket.receive() {
            let message = String::from_utf8_lossy(&packet);
            info!("Message from {peer}: {message:?}");
        }

        let mut stdin_text: String = Default::default();

        #[cfg(not(target_arch = "wasm32"))]
        let stdin_fut = stdin.read_line(&mut stdin_text);
        #[cfg(target_arch = "wasm32")]
        let stdin_fut = pending::<()>();

        select! {
            // Restart this loop every 100ms
            _ = (&mut timeout).fuse() => {
                timeout.reset(Duration::from_millis(100));
            }

            _ = stdin_fut.fuse() => {
                debug!("Got line: {stdin_text}");
                let packet: Packet = Box::from(stdin_text.as_bytes());
                for peer in socket.connected_peers().into_iter().collect::<Vec<_>>() {
                    unreliable_socket.send(packet.clone(), peer);
                }
            }

            // Or break if the message loop ends (disconnected, closed, etc.)
            _ = &mut loop_fut => {
                break;
            }
        }
    }
    debug!("Done?");
}
