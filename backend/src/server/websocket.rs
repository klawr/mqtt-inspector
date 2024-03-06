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

use super::config;
use super::jsonrpc::JsonRpcNotification;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::UnboundedSender;

pub type PeerMap = Arc<Mutex<HashMap<SocketAddr, UnboundedSender<warp::filters::ws::Message>>>>;

pub fn send_message_to_peers(
    peer_map: &PeerMap,
    source: &str,
    topic: &str,
    payload: &bytes::Bytes,
) {
    peer_map.lock().unwrap().iter().for_each(|(addr, tx)| {
        let message = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "mqtt_message".to_string(),
            params: serde_json::json!({
                "source": source,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "topic": topic,
                "payload": payload.to_vec(), // Convert Bytes to Vec<u8>
            }),
        };

        if let Ok(serialized) = serde_json::to_string(&message) {
            match tx.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => {
                    if tx.is_closed() {
                        println!("Peer {} is closed. Removing from peer map.", addr);
                        peer_map.lock().unwrap().remove(addr);
                    }
                    println!("Error sending message to {}: {:?}", addr, err);
                }
            }
        }
    });
}

pub fn send_broker_status_to_peers(peer_map: &PeerMap, source: &String, status: bool) {
    peer_map.lock().unwrap().iter().for_each(|(addr, tx)| {
        let message = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "mqtt_connection_status".to_string(),
            params: serde_json::json!({
                "source": source,
                "connected": status,
            }),
        };

        if let Ok(serialized) = serde_json::to_string(&message) {
            match tx.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => {
                    if tx.is_closed() {
                        println!("Peer {} is closed. Removing from peer map.", addr);
                        peer_map.lock().unwrap().remove(addr);
                    }
                    println!("Error sending message to {}: {:?}", addr, err);
                }
            }
        }
    });
}

pub fn broadcast_pipelines(peer_map: PeerMap, config_path: &String) {
    peer_map
        .lock()
        .unwrap()
        .iter()
        .for_each(|(_addr, tx)| config::send_pipelines(tx, &format!("{}/pipelines", config_path)));
}

pub fn broadcast_commands(peer_map: PeerMap, config_path: &String) {
    peer_map
        .lock()
        .unwrap()
        .iter()
        .for_each(|(_addr, tx)| config::send_commands(tx, &format!("{}/commands", config_path)));
}
