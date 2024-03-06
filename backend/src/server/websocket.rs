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

use super::config::{CommandMessage, PipelineMessage};
use super::jsonrpc;
use super::mqtt;

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
        let message = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "mqtt_message",
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

pub fn send_broker_status_to_peers(peer_map: &PeerMap, source: &str, status: bool) {
    peer_map.lock().unwrap().iter().for_each(|(addr, tx)| {
        let message = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "mqtt_connection_status",
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

pub fn send_configs(sender: &UnboundedSender<warp::filters::ws::Message>, config_path: &str) {
    send_commands(sender, &format!("{}/commands", config_path));
    send_pipelines(sender, &format!("{}/pipelines", config_path));
}

pub fn send_commands(
    sender: &UnboundedSender<warp::filters::ws::Message>,
    commands_path: &String,
) {
    if let Ok(commands) = std::fs::read_dir(commands_path) {
        let commands: Vec<CommandMessage> = commands
            .filter_map(|dir_entry| {
                if let Ok(file) = dir_entry {
                    if let Ok(file_content) = std::fs::read_to_string(file.path()) {
                        return serde_json::from_str(&file_content).ok();
                    }
                }
                None
            })
            .collect();

        let jsonrpc = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "commands",
            params: serde_json::json!(&commands),
        };

        if let Ok(serialized) = serde_json::to_string(&jsonrpc) {
            match sender.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => println!("Error sending message: {:?}", err),
            }
        } else {
            eprintln!("Failed to serialize commands jsonjpc")
        }
    } else {
        eprintln!("Failed to read commands file from {}", commands_path);
    }
}

pub fn send_pipelines(
    sender: &UnboundedSender<warp::filters::ws::Message>,
    pipelines_path: &String,
) {
    if let Ok(pipelines) = std::fs::read_dir(pipelines_path) {
        let pipelines: Vec<PipelineMessage> = pipelines
            .filter_map(|dir_entry| {
                if let Ok(file) = dir_entry {
                    if let Ok(file_content) = std::fs::read_to_string(file.path()) {
                        return serde_json::from_str(&file_content).ok();
                    }
                }
                None
            })
            .collect();

        let jsonrpc = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0",
            method: "pipelines",
            params: serde_json::json!(&pipelines),
        };

        if let Ok(serialized) = serde_json::to_string(&jsonrpc) {
            match sender.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => println!("Error sending message: {:?}", err),
            }
        } else {
            eprintln!("Failed to serialize commands jsonjpc")
        }
    }
}

pub fn broadcast_brokers(peer_map: &PeerMap, mqtt_map: &mqtt::BrokerMap) {
    peer_map.lock().unwrap().iter().for_each(|(_addr, tx)| {
        send_brokers(tx, mqtt_map);
    });
}

pub fn send_brokers(tx: &UnboundedSender<warp::filters::ws::Message>, mqtt_map: &mqtt::BrokerMap) {
    let binding = mqtt_map.lock().unwrap();
    let brokers: Vec<&mqtt::MqttBroker> = binding.iter().map(|broker| broker.1).collect();
    let message = jsonrpc::JsonRpcNotification {
        jsonrpc: "2.0",
        method: "mqtt_brokers",
        params: serde_json::json!(brokers.clone()),
    };

    if let Ok(serialized) = serde_json::to_string(&message) {
        match tx.unbounded_send(warp::filters::ws::Message::text(serialized)) {
            Ok(_) => { /* Implement Logging */ }
            Err(err) => println!("Error sending message: {:?}", err),
        }
    } else {
        println!("Failed to serialize brokers.");
    }
}

pub fn broadcast_pipelines(peer_map: &PeerMap, config_path: &str) {
    peer_map
        .lock()
        .unwrap()
        .iter()
        .for_each(|(_addr, tx)| send_pipelines(tx, &format!("{}/pipelines", config_path)));
}

pub fn broadcast_commands(peer_map: &PeerMap, config_path: &str) {
    peer_map
        .lock()
        .unwrap()
        .iter()
        .for_each(|(_addr, tx)| send_commands(tx, &format!("{}/commands", config_path)));
}
