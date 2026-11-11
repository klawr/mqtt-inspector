/*
 * Copyright (c) 2024 Kai Lawrence
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

mod broker_peer_bridge;
mod config;
mod jsonrpc;
mod mqtt;
mod oidc;
#[cfg(test)]
mod test_certs;
mod websocket;

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Mutex,
};

use futures_channel::mpsc::channel;
use futures_util::{pin_mut, StreamExt, TryStreamExt};
use warp::Filter;

fn max_ws_publish_message_size() -> usize {
    // Frontend publish payload is sent as JSON text over WS. Allow extra room
    // for JSON overhead and escaped characters beyond raw payload bytes.
    let mqtt_limit = mqtt::max_incoming_packet_size();
    mqtt_limit.saturating_mul(2)
}

pub fn run_server(static_files: String, config_path: String) -> tokio::task::JoinHandle<()> {
    let mqtt_map = mqtt::BrokerMap::new(Mutex::new(HashMap::new()));
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3030);
    let peer_map = websocket::PeerMap::new(Mutex::new(HashMap::new()));
    let notification_buf =
        websocket::NotificationBuf::new(Mutex::new(websocket::NotificationBuffer::default()));

    // Spawn a dedicated thread that flushes batched notifications every 100 ms
    {
        let buf = notification_buf.clone();
        let pm = peer_map.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            websocket::flush_notification_buffer(&buf, &pm);
        });
    }

    let oidc = oidc::OidcConfig::from_env().map(|config| {
        println!(
            "Claims-based access enabled (issuer {}, roles from {:?}, user role {:?}, admin role {:?})",
            config.issuer, config.roles_claims, config.user_role, config.admin_role
        );
        match oidc::OidcVerifier::new(config) {
            Ok(verifier) => std::sync::Arc::new(verifier),
            // Never fall back to open access on a broken auth configuration.
            Err(err) => panic!("Invalid OIDC configuration: {err}"),
        }
    });

    let broker_path = &std::format!("{config_path}/brokers.json");
    if oidc.is_some() {
        let configs = config::get_known_brokers(broker_path);
        for host in config::brokers_with_ignored_access_password(&configs) {
            eprintln!(
                "Warning: broker {host} has an access_password, which is ignored in claims mode; \
                 use allowed_roles to restrict access."
            );
        }
    }
    broker_peer_bridge::connect_to_known_brokers(
        broker_path,
        &peer_map,
        &mqtt_map,
        &notification_buf,
    );
    println!("Listening for connections on {server_addr} using static files from {static_files} and config {config_path}");

    let ws = {
        let ws_max = max_ws_publish_message_size();
        warp::path("ws")
            .and(
                warp::ws().map(move |ws: warp::ws::Ws| {
                    ws.max_message_size(ws_max).max_frame_size(ws_max)
                }),
            )
            .and(warp::addr::remote())
            .and(warp::header::headers_cloned())
            .and_then(
                move |ws: warp::ws::Ws,
                      addr: Option<SocketAddr>,
                      headers: warp::http::HeaderMap| {
                    let peer_map = std::sync::Arc::clone(&peer_map);
                    let mqtt_map = std::sync::Arc::clone(&mqtt_map);
                    let notification_buf = std::sync::Arc::clone(&notification_buf);
                    let config_path = config_path.clone();
                    let oidc = oidc.clone();
                    async move {
                        let identity = match &oidc {
                            None => None,
                            Some(verifier) => match authenticate(verifier, &headers).await {
                                Ok(identity) => Some(identity),
                                Err(err) => {
                                    println!("Rejected WebSocket connection from {addr:?}: {err}");
                                    return Ok::<_, warp::Rejection>(auth_error_reply(&err));
                                }
                            },
                        };
                        let reply = ws.on_upgrade(move |socket| {
                            handle_socket(
                                socket,
                                addr,
                                identity,
                                peer_map,
                                mqtt_map,
                                notification_buf,
                                config_path,
                            )
                        });
                        Ok(Box::new(reply) as Box<dyn warp::Reply>)
                    }
                },
            )
    };

    let routes = warp::get()
        .and(ws)
        .or(warp::get().and(warp::fs::dir(static_files)));

    tokio::spawn(async move {
        warp::serve(routes).run(server_addr).await;
    })
}

async fn authenticate(
    verifier: &oidc::OidcVerifier,
    headers: &warp::http::HeaderMap,
) -> Result<oidc::Identity, oidc::AuthError> {
    let token = oidc::token_from_headers(headers).ok_or(oidc::AuthError::MissingToken)?;
    verifier.verify(&token).await
}

fn auth_error_reply(err: &oidc::AuthError) -> Box<dyn warp::Reply> {
    let status = match err {
        oidc::AuthError::MissingToken | oidc::AuthError::InvalidToken(_) => {
            warp::http::StatusCode::UNAUTHORIZED
        }
        oidc::AuthError::Forbidden(_) => warp::http::StatusCode::FORBIDDEN,
        oidc::AuthError::Unavailable(_) => warp::http::StatusCode::SERVICE_UNAVAILABLE,
    };
    // Details are logged server-side only.
    Box::new(warp::reply::with_status(
        status.canonical_reason().unwrap_or_default(),
        status,
    ))
}

/// Time until the token expires; the socket is closed then so the client
/// reconnects through the proxy with a fresh token.
fn time_until_expiry(identity: &oidc::Identity) -> std::time::Duration {
    let remaining = identity.expires_at - chrono::Utc::now().timestamp();
    std::time::Duration::from_secs(remaining.max(0) as u64)
}

async fn handle_socket(
    socket: warp::ws::WebSocket,
    addr: Option<SocketAddr>,
    identity: Option<oidc::Identity>,
    peer_map: websocket::PeerMap,
    mqtt_map: mqtt::BrokerMap,
    notification_buf: websocket::NotificationBuf,
    config_path: String,
) {
    if identity.is_some() && addr.is_none() {
        // Permissions are looked up per peer address; without one we can't enforce them.
        println!("Closing claims-mode connection without a remote address");
        return;
    }
    let (ws_tx, ws_rx) = socket.split();
    let (tx, rx) = channel(websocket::PEER_CHANNEL_CAPACITY);
    // Fixed for the lifetime of this connection and passed to every request,
    // independent of whether the peer is still in `peer_map`.
    let access = identity
        .clone()
        .map(websocket::PeerAccess::Claims)
        .unwrap_or_default();

    if let Some(addr) = addr {
        match &identity {
            Some(id) => println!(
                "Received new WebSocket connection from {addr} (user {}, admin={})",
                id.display_name(),
                id.is_admin
            ),
            None => println!("Received new WebSocket connection from {addr}"),
        }
        let mut peer = websocket::PeerConnection::new(tx);
        peer.access = access.clone();
        websocket::auto_authenticate_peer(&mut peer, &mqtt_map);
        websocket::send_session(&mut peer);
        websocket::send_brokers(&mut peer, &mqtt_map);
        websocket::send_configs(&mut peer.tx, &config_path);
        peer_map.lock().unwrap().insert(addr, peer);
    }
    let incoming = rx.map(Ok).forward(ws_tx);

    let handler = ws_rx.try_for_each(|msg| {
        if let Ok(text) = msg.to_str() {
            broker_peer_bridge::deserialize_json_rpc_and_process(
                text,
                &peer_map,
                &mqtt_map,
                &config_path,
                addr,
                &notification_buf,
                &access,
            );
        }

        futures_util::future::ok(())
    });

    let expiry = async {
        match &identity {
            Some(id) => tokio::time::sleep(time_until_expiry(id)).await,
            None => futures_util::future::pending::<()>().await,
        }
    };

    pin_mut!(incoming, handler, expiry);
    tokio::select! {
        _ = &mut incoming => {}
        _ = &mut handler => {}
        _ = &mut expiry => {
            println!("Access token of {addr:?} expired. Closing connection.");
        }
    }

    if let Some(addr) = addr {
        println!("{addr} disconnected");
        peer_map.lock().unwrap().remove(&addr);
    }
}
