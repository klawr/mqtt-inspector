use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{pin_mut, StreamExt, TryStreamExt};
use warp::Filter;

use crate::{
    config,
    jsonrpc::{self, JsonRpcNotification},
    mqtt,
};

type PeerMap = Arc<Mutex<HashMap<SocketAddr, UnboundedSender<warp::filters::ws::Message>>>>;

fn send_message_to_peers(peer_map: &PeerMap, source: &str, topic: &str, payload: &bytes::Bytes) {
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

fn loop_forever(mut connection: rumqttc::Connection, peer_map: &PeerMap) {
    let (ip, port) = connection.eventloop.mqtt_options.broker_address();
    let source = format!("{}:{}", ip, port);
    for (_i, notification) in connection.iter().enumerate() {
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
                send_message_to_peers(peer_map, &source, &p.topic, &payload);
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(a))) => {
                println!("Connection event: {:?}", a);
            }
            Ok(_) => {
                // Handle other types of events if necessary
            }
            Err(rumqttc::ConnectionError::MqttState(rumqttc::StateError::Deserialization(
                rumqttc::mqttbytes::Error::PayloadSizeLimitExceeded(p),
            ))) => {
                let payload =
                    bytes::Bytes::from(std::format!("Payload size limit exceeded: {}", p));
                println!("Payload size limit exceeded: {}", p);
                send_message_to_peers(peer_map, &source, "error", &payload);
            }
            Err(err) => {
                println!("Unhandled connection error: {:?}", err);
                // TODO: Handle errors properly
                break;
            }
        }
    }
}

pub fn connect_to_known_brokers(broker_path: String, peer_map: PeerMap, mqtt_map: mqtt::Map) {
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();
    let known_brokers = config::get_known_brokers(&broker_path);

    known_brokers.iter().for_each(|broker| {
        let (ip, port) = broker.split_once(':').unwrap();
        connect_to_broker(ip, port, peer_map_clone.clone(), mqtt_map_clone.clone());
    });
}

fn connect_to_mqtt_client(mqtt_host: &SocketAddr, mqtt_map: mqtt::Map, peer_map: PeerMap) -> () {
    let ip = mqtt_host.ip().to_string();
    let port = mqtt_host.port();
    let mut mqtt_lock = mqtt_map.lock().unwrap();
    let mqtt_client = mqtt_lock
        .iter()
        .find(|client| client.0.ip().to_string() == ip && client.0.port() == port);

    if mqtt_client.is_some() {
        println!("MQTT-Client for {} already exists.", mqtt_host);
    } else {
        println!(
            "MQTT-Client for {} does not exist. Creating new client.",
            mqtt_host
        );
        let (client, connection) = mqtt::connect_to_mqtt_host(&ip, port);
        mqtt_lock.insert(*mqtt_host, client);
        drop(mqtt_lock);

        loop_forever(connection, &peer_map);
    }
}

fn connect_to_broker(mqtt_ip: &str, mqtt_port: &str, peer_map: PeerMap, mqtt_map: mqtt::Map) {
    let mqtt_host: SocketAddr = format!("{}:{}", mqtt_ip, mqtt_port)
        .parse()
        .expect("Failed to parse MQTT host and port");
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();

    std::thread::spawn(move || {
        connect_to_mqtt_client(&mqtt_host, mqtt_map_clone, peer_map_clone);
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
    peer_map.lock().unwrap().iter().for_each(|(_addr, tx)| {
        config::send_commands(tx, &format!("{}/commands.json", config_path))
    });
}

fn broadcast_brokers(peer_map: PeerMap, mqtt_map: mqtt::Map) {
    send_brokers(
        &peer_map.lock().unwrap().values().next().unwrap(),
        &mqtt_map,
    );
}

fn deserialize_json_rpc_and_process(
    json_rpc: &str,
    peer_map: PeerMap,
    mqtt_map: mqtt::Map,
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
            let (ip, port) = jsonrpc::get_ip_and_port(message.params);
            connect_to_broker(&ip, &port, peer_map.clone(), mqtt_map.clone());
            let broker_path = std::format!("{}/brokers.json", config_path);
            config::add_to_brokers(&broker_path, std::format!("{}:{}", ip, port));
            broadcast_brokers(peer_map, mqtt_map)
        }
        "publish" => {
            let (ip, port, topic, payload) = jsonrpc::get_ip_port_topic_and_payload(message.params);
            mqtt::publish_message(&ip, &port, &topic, &payload, mqtt_map);
        }
        "save_publish" => {
            let command_path = std::format!("{}/commands.json", config_path);
            config::add_to_commands(&command_path, message.params);
            broadcast_commands(peer_map, &config_path);
        }
        "save_pipeline" => {
            let pipelines_path = std::format!("{}/pipelines", config_path);
            config::add_to_pipelines(&pipelines_path, message.params);
            broadcast_pipelines(peer_map, &config_path);
        }
        _ => {
            // Implement other methods
        }
    }
    // Implement deserialization and processing of JSON-RPC message
}

fn send_brokers(tx: &UnboundedSender<warp::filters::ws::Message>, mqtt_map: &mqtt::Map) {
    let brokers: Vec<String> = mqtt_map
        .lock()
        .unwrap()
        .iter()
        .map(|(addr, _client)| std::format!("{}:{}", addr.ip().to_string(), addr.port()))
        .collect();
    let message = JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "mqtt_brokers".to_string(),
        params: serde_json::json!(brokers),
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
    let mqtt_map = mqtt::Map::new(Mutex::new(HashMap::new()));
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3030);
    let peer_map = PeerMap::new(Mutex::new(HashMap::new()));

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
