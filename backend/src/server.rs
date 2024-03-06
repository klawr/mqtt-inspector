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

mod config;
mod jsonrpc;
mod mqtt;
mod websocket;

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Mutex,
};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{pin_mut, StreamExt, TryStreamExt};
use warp::Filter;

use jsonrpc::JsonRpcNotification;

fn loop_forever(
    mut connection: rumqttc::Connection,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
) {
    let (ip, port) = connection.eventloop.mqtt_options.broker_address();
    let hostname = format!("{}:{}", ip, port);

    for (_i, notification) in connection.iter().enumerate() {
        let mut mqtt_lock = mqtt_map.lock().unwrap();
        // Check if broker is still in map:
        if !mqtt_lock.contains_key(&hostname) {
            println!("Broker {} not found in map. Exiting loop.", hostname);
            break;
        }
        match notification {
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(p))) => {
                let payload = if p.payload.len() > 1000000 {
                    bytes::Bytes::from(std::format!(
                        "Payload size limit exceeded: {}.\nThe message is probably fine, but is is too large to be displayed.",
                        p.payload.len()
                    ))
                } else {
                    bytes::Bytes::from(p.payload)
                };
                mqtt_lock.entry(hostname.clone()).and_modify(|broker| {
                    broker.connected = true;
                    if broker.topics.contains_key(&p.topic) {
                        let topic_vec = broker.topics.get_mut(&p.topic).unwrap();
                        topic_vec.push(mqtt::MqttMessage {
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            payload: payload.to_vec(),
                        });
                        while topic_vec.len() > 100 {
                            topic_vec.pop();
                        }
                    } else {
                        broker.topics.insert(
                            p.topic.clone(),
                            vec![mqtt::MqttMessage {
                                timestamp: chrono::Utc::now().to_rfc3339(),
                                payload: payload.to_vec(),
                            }],
                        );
                    }
                });
                websocket::send_message_to_peers(peer_map, &hostname, &p.topic, &payload);
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(a))) => {
                mqtt_lock.entry(hostname.clone()).and_modify(|broker| {
                    broker.connected = true;
                });
                // Small update for the peers already connected
                websocket::send_broker_status_to_peers(peer_map, &hostname, true);
                println!("Connection event: {:?} for {:?}", a.code, hostname);
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Disconnect)) => {
                println!("Disconnect event for {:?}", hostname);
            }
            Ok(_) => {
                // maybe handle other events
            }
            Err(rumqttc::ConnectionError::MqttState(rumqttc::StateError::Deserialization(
                rumqttc::mqttbytes::Error::PayloadSizeLimitExceeded(p),
            ))) => {
                let payload =
                    bytes::Bytes::from(std::format!("Payload size limit exceeded: {}.", p));
                println!("Payload size limit exceeded: {}", p);
                websocket::send_message_to_peers(peer_map, &hostname, "$ERROR", &payload)
            }
            Err(rumqttc::ConnectionError::MqttState(err)) => {
                println!(
                    "Got ConnectionAborted event for {:?}. Error: {}. Stopping loop",
                    hostname,
                    err.to_string()
                );
                break;
            }
            Err(err) => {
                // Update the connection status of the broker
                mqtt_lock.entry(hostname.clone()).and_modify(|broker| {
                    broker.connected = false;
                });
                // Small update for the peers already connected
                websocket::send_broker_status_to_peers(peer_map, &hostname, false);
                println!(
                    "Connection error: {:?} for {:?}. Try again in 5 seconds.",
                    err.to_string(),
                    hostname
                );
                drop(mqtt_lock);
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
    }
}

fn connect_to_known_brokers(
    broker_path: String,
    peer_map: websocket::PeerMap,
    mqtt_map: mqtt::BrokerMap,
) {
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();
    let known_brokers = config::get_known_brokers(&broker_path);

    known_brokers.iter().for_each(|broker| {
        connect_to_broker(broker, peer_map_clone.clone(), mqtt_map_clone.clone());
    });
}

fn connect_to_mqtt_client(
    mqtt_host: &String,
    mqtt_map: mqtt::BrokerMap,
    peer_map: websocket::PeerMap,
) -> () {
    let mut mqtt_lock = mqtt_map.lock().unwrap();
    let mqtt_client = mqtt_lock.iter().find(|entry| entry.0 == mqtt_host);

    if mqtt_client.is_some() {
        println!("MQTT-Client for {} already exists.", mqtt_host);
    } else {
        println!(
            "MQTT-Client for {} does not exist. Creating new client.",
            mqtt_host
        );
        let (client, connection) = mqtt::connect_to_mqtt_host(&mqtt_host);
        let broker = mqtt::MqttBroker {
            client,
            broker: mqtt_host.to_string(),
            connected: false,
            topics: HashMap::new(),
        };

        mqtt_lock.insert(mqtt_host.clone(), broker);
        drop(mqtt_lock);

        loop_forever(connection, &peer_map, &mqtt_map);
    }
}

fn remove_broker(mqtt_host: &String, peer_map: websocket::PeerMap, mqtt_map: mqtt::BrokerMap) {
    let mut mqtt_lock = mqtt_map.lock().unwrap();
    let mqtt_client = mqtt_lock.iter().find(|entry| entry.0 == mqtt_host);

    if mqtt_client.is_some() {
        println!("Removing MQTT-Client for {}", mqtt_host);

        if let Err(err) = mqtt_client.unwrap().1.client.clone().disconnect() {
            println!("Error disconnecting MQTT client: {:?}", err);
        }

        mqtt_lock.remove(mqtt_host);
        drop(mqtt_lock);
        peer_map.lock().unwrap().iter().for_each(|(_addr, tx)| {
            let message = JsonRpcNotification {
                jsonrpc: "2.0".to_string(),
                method: "broker_removal".to_string(),
                params: serde_json::json!(mqtt_host),
            };

            if let Ok(serialized) = serde_json::to_string(&message) {
                match tx.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                    Ok(_) => { /* Implement Logging */ }
                    Err(err) => println!("Error sending message: {:?}", err),
                }
            } else {
                println!("Failed to serialize brokers.");
            }
        });
    } else {
        println!("No MQTT-Client for {} found.", mqtt_host);
    }
}

fn connect_to_broker(mqtt_host: &String, peer_map: websocket::PeerMap, mqtt_map: mqtt::BrokerMap) {
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();
    let host = mqtt_host.clone();

    std::thread::spawn(move || {
        connect_to_mqtt_client(&host, mqtt_map_clone, peer_map_clone);
    });
}

fn broadcast_brokers(peer_map: websocket::PeerMap, mqtt_map: mqtt::BrokerMap) {
    peer_map.lock().unwrap().iter().for_each(|(_addr, tx)| {
        send_brokers(tx, &mqtt_map);
    });
}

fn deserialize_json_rpc_and_process(
    json_rpc: &str,
    peer_map: websocket::PeerMap,
    mqtt_map: mqtt::BrokerMap,
    config_path: String,
) -> () {
    let result = jsonrpc::deserialize_json_rpc(json_rpc);
    if result.is_err() {
        println!("Error deserializing JSON-RPC: {:?}", result.err());
        return;
    }
    let message = result.unwrap();
    println!(
        "Got method \"{}\" with params {}",
        message.method,
        message.params.clone()
    );
    match message.method.as_str() {
        "connect" => {
            let hostname = message.params["hostname"].as_str().unwrap().to_string();
            connect_to_broker(&hostname, peer_map.clone(), mqtt_map.clone());
            let broker_path = std::format!("{}/brokers.json", config_path);
            config::add_to_brokers(&broker_path, hostname);
            broadcast_brokers(peer_map, mqtt_map)
        }
        "remove" => {
            let hostname = message.params["hostname"].as_str().unwrap().to_string();
            remove_broker(&hostname, peer_map.clone(), mqtt_map.clone());
            let broker_path = std::format!("{}/brokers.json", config_path);
            config::remove_from_brokers(&broker_path, hostname);
            broadcast_brokers(peer_map, mqtt_map)
        }
        "publish" => {
            let (host, topic, payload) = jsonrpc::get_host_topic_and_payload(message.params);
            mqtt::publish_message(&host, &topic, &payload, mqtt_map);
        }
        "save_command" => {
            let command_path: String = std::format!("{}/commands", config_path);
            config::add_to_commands(&command_path, message.params);
            websocket::broadcast_commands(peer_map, &config_path);
        }
        "remove_command" => {
            let command_path: String = std::format!("{}/commands", config_path);
            config::remove_from_commands(&command_path, message.params);
            websocket::broadcast_commands(peer_map, &config_path);
        }
        "save_pipeline" => {
            let pipelines_path = std::format!("{}/pipelines", config_path);
            config::add_to_pipelines(&pipelines_path, message.params);
            websocket::broadcast_pipelines(peer_map, &config_path);
        }
        "remove_pipeline" => {
            let pipelines_path = std::format!("{}/pipelines", config_path);
            config::remove_from_pipelines(&pipelines_path, message.params);
            websocket::broadcast_pipelines(peer_map, &config_path);
        }
        _ => {
            // Implement other methods
        }
    }
    // Implement deserialization and processing of JSON-RPC message
}

fn send_brokers(tx: &UnboundedSender<warp::filters::ws::Message>, mqtt_map: &mqtt::BrokerMap) {
    let binding = mqtt_map.lock().unwrap();
    let brokers: Vec<&mqtt::MqttBroker> = binding.iter().map(|broker| broker.1).collect();
    let message = JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "mqtt_brokers".to_string(),
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

pub fn run_server(static_files: String, config_path: String) -> tokio::task::JoinHandle<()> {
    let mqtt_map = mqtt::BrokerMap::new(Mutex::new(HashMap::new()));
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3030);
    let peer_map = websocket::PeerMap::new(Mutex::new(HashMap::new()));

    connect_to_known_brokers(
        std::format!("{}/brokers.json", config_path),
        peer_map.clone(),
        mqtt_map.clone(),
    );

    let config_path_clone = config_path.clone();

    let ws = warp::path("ws")
        .and(warp::ws())
        .and(warp::addr::remote())
        .map(move |ws: warp::ws::Ws, addr: Option<SocketAddr>| {
            let peer_map = peer_map.clone();
            let mqtt_map = mqtt_map.clone();
            let config_path = config_path_clone.clone();
            ws.on_upgrade(move |socket| async move {
                let (ws_tx, ws_rx) = socket.split();
                let (tx, rx) = unbounded();

                if let Some(addr) = addr {
                    println!("Received new WebSocket connection from {}", addr);
                    send_brokers(&tx, &mqtt_map);
                    config::send_configs(&tx, &config_path);

                    peer_map.lock().unwrap().insert(addr, tx);
                }
                let incoming = rx.map(Ok).forward(ws_tx);

                let handler = ws_rx.try_for_each(|msg| {
                    if let Ok(text) = msg.to_str() {
                        deserialize_json_rpc_and_process(
                            text,
                            peer_map.clone(),
                            mqtt_map.clone(),
                            config_path.clone(),
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

    println!(
        "Listening for connections on {} using static files from {} and config {}",
        server_addr, static_files, config_path
    );
    let routes = warp::get().and(ws.or(warp::fs::dir(static_files)));
    let warp_handle = tokio::spawn(async move {
        warp::serve(routes).run(server_addr).await;
    });

    warp_handle
}
