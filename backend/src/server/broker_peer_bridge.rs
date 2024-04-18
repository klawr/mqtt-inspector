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
use super::jsonrpc;
use super::mqtt;
use super::websocket;

use std::collections::HashMap;

fn loop_forever(
    mut connection: rumqttc::Connection,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
) {
    let (ip, port) = connection.eventloop.mqtt_options.broker_address();
    let hostname = format!("{}:{}", ip, port);

    for notification in connection.iter() {
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
                    p.payload
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
                    hostname, err
                );
                break;
            }
            Err(_err) => {
                // Update the connection status of the broker
                mqtt_lock.entry(hostname.clone()).and_modify(|broker| {
                    broker.connected = false;
                });
                // Small update for the peers already connected
                websocket::send_broker_status_to_peers(peer_map, &hostname, false);
                drop(mqtt_lock);
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        }
    }
}

pub fn connect_to_known_brokers(
    broker_path: &str,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
) {
    config::get_known_brokers(broker_path)
        .iter()
        .for_each(|broker| {
            connect_to_broker(broker, peer_map, mqtt_map);
        });
}

pub fn deserialize_json_rpc_and_process(
    json_rpc: &str,
    peer_map: &websocket::PeerMap,
    mqtt_map: &mqtt::BrokerMap,
    config_path: &str,
) {
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
    match message.method {
        "connect" => {
            let hostname = message.params["hostname"]
                .to_string()
                .trim_matches('"')
                .to_string();
            connect_to_broker(&hostname, peer_map, mqtt_map);
            let broker_path = std::format!("{}/brokers.json", &config_path);
            config::add_to_brokers(&broker_path, &hostname);
            websocket::broadcast_brokers(peer_map, mqtt_map)
        }
        "remove" => {
            let hostname = message.params["hostname"]
                .to_string()
                .trim_matches('"')
                .to_string();
            remove_broker(&hostname, peer_map, mqtt_map);
            let broker_path = std::format!("{}/brokers.json", &config_path);
            config::remove_from_brokers(&broker_path, &hostname);
            websocket::broadcast_brokers(peer_map, mqtt_map)
        }
        "publish" => {
            let host = message.params["host"]
                .to_string()
                .trim_matches('"')
                .to_string();
            let topic = message.params["topic"].as_str().unwrap();
            let payload = message.params["payload"].as_str().unwrap();
            mqtt::publish_message(&host, topic, payload, mqtt_map);
        }
        "save_command" => {
            let command_path: String = std::format!("{}/commands", config_path);
            config::add_to_commands(&command_path, message.params);
            websocket::broadcast_commands(peer_map, config_path);
        }
        "remove_command" => {
            let command_path: String = std::format!("{}/commands", config_path);
            config::remove_from_commands(&command_path, message.params);
            websocket::broadcast_commands(peer_map, config_path);
        }
        "save_pipeline" => {
            let pipelines_path = std::format!("{}/pipelines", config_path);
            config::add_to_pipelines(&pipelines_path, message.params);
            websocket::broadcast_pipelines(peer_map, config_path);
        }
        "remove_pipeline" => {
            let pipelines_path = std::format!("{}/pipelines", config_path);
            config::remove_from_pipelines(&pipelines_path, message.params);
            websocket::broadcast_pipelines(peer_map, config_path);
        }
        _ => {
            // Implement other methods
        }
    }
    // Implement deserialization and processing of JSON-RPC message
}

fn connect_to_broker(mqtt_host: &str, peer_map: &websocket::PeerMap, mqtt_map: &mqtt::BrokerMap) {
    let mqtt_host_clone = mqtt_host.trim_matches('"').to_string();
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();

    std::thread::spawn(move || {
        connect_to_mqtt_client_and_loop_forever(&mqtt_host_clone, &mqtt_map_clone, &peer_map_clone);
    });
}

fn connect_to_mqtt_client_and_loop_forever(
    mqtt_host: &String,
    mqtt_map: &mqtt::BrokerMap,
    peer_map: &websocket::PeerMap,
) {
    let mut mqtt_lock = mqtt_map.lock().unwrap();
    let mqtt_client = mqtt_lock.iter().find(|entry| entry.0 == mqtt_host);

    if mqtt_client.is_some() {
        println!("MQTT-Client for {} already exists.", mqtt_host);
    } else {
        println!(
            "MQTT-Client for {} does not exist. Creating new client.",
            mqtt_host
        );
        let (client, connection) = mqtt::connect_to_mqtt_host(mqtt_host);
        let broker = mqtt::MqttBroker {
            client,
            broker: mqtt_host.to_string(),
            connected: false,
            topics: HashMap::new(),
        };

        mqtt_lock.insert(mqtt_host.to_string(), broker);
        drop(mqtt_lock);

        loop_forever(connection, peer_map, mqtt_map);
    }
}

fn remove_broker(mqtt_host: &str, peer_map: &websocket::PeerMap, mqtt_map: &mqtt::BrokerMap) {
    let mut mqtt_lock = mqtt_map.lock().unwrap();
    let mqtt_client = mqtt_lock.iter().find(|entry| entry.0 == mqtt_host);

    if mqtt_client.is_some() {
        println!("Removing MQTT-Client for {}", mqtt_host);

        if let Err(err) = mqtt_client.unwrap().1.client.clone().disconnect() {
            println!("Error disconnecting MQTT client: {:?}", err);
        }

        mqtt_lock.remove(mqtt_host);
        drop(mqtt_lock);
        // TODO put this into websocket
        peer_map.lock().unwrap().iter().for_each(|(_addr, tx)| {
            let message = jsonrpc::JsonRpcNotification {
                jsonrpc: "2.0",
                method: "broker_removal",
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
