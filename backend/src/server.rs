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
mod websocket;

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Mutex,
};

use futures_channel::mpsc::unbounded;
use futures_util::{pin_mut, StreamExt, TryStreamExt};
use warp::Filter;

pub fn run_server(static_files: String, config_path: String) -> tokio::task::JoinHandle<()> {
    let mqtt_map = mqtt::BrokerMap::new(Mutex::new(HashMap::new()));
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3030);
    let peer_map = websocket::PeerMap::new(Mutex::new(HashMap::new()));

    let broker_path = &std::format!("{}/brokers.json", config_path);
    broker_peer_bridge::connect_to_known_brokers(broker_path, &peer_map, &mqtt_map);
    println!(
        "Listening for connections on {} using static files from {} and config {}",
        server_addr, static_files, config_path
    );

    let ws = warp::path("ws")
        .and(warp::ws())
        .and(warp::addr::remote())
        .map(move |ws: warp::ws::Ws, addr: Option<SocketAddr>| {
            let peer_map = std::sync::Arc::clone(&peer_map);
            let mqtt_map = std::sync::Arc::clone(&mqtt_map);
            // TODO can I avoid cloning config_path here?
            let config_path = config_path.clone();
            ws.on_upgrade(move |socket| async move {
                let (ws_tx, ws_rx) = socket.split();
                let (tx, rx) = unbounded();

                if let Some(addr) = addr {
                    println!("Received new WebSocket connection from {}", addr);
                    websocket::send_brokers(&tx, &mqtt_map);
                    websocket::send_configs(&tx, &config_path);

                    peer_map.lock().unwrap().insert(addr, tx);
                }
                let incoming = rx.map(Ok).forward(ws_tx);

                let handler = ws_rx.try_for_each(|msg| {
                    if let Ok(text) = msg.to_str() {
                        broker_peer_bridge::deserialize_json_rpc_and_process(
                            text,
                            &peer_map,
                            &mqtt_map,
                            &config_path,
                        );
                    }

                    futures_util::future::ok(())
                });

                pin_mut!(incoming, handler);
                futures_util::future::select(incoming, handler).await;

                if let Some(addr) = addr {
                    println!("{} disconnected", addr);
                    peer_map.lock().unwrap().remove(&addr);
                }
            })
        });

    let routes = warp::get().and(ws.or(warp::fs::dir(static_files)));
    

    tokio::spawn(async move {
        warp::serve(routes).run(server_addr).await;
    })
}
