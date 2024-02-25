use futures_channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{pin_mut, StreamExt, TryStreamExt};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
};
use warp::{
    filters::ws::Message,
    Filter,
};

mod jsonrpc;
mod mqtt;

type MqttMap = mqtt::MqttMap;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, UnboundedSender<Message>>>>;

fn send_message_to_peers(peer_map: &PeerMap, source: &str, topic: &str, payload: &bytes::Bytes) {
    peer_map.lock().unwrap().iter().for_each(|(addr, tx)| {
        let message = serde_json::json!({
            "source": source,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "topic": topic,
            "payload": payload.to_vec(), // Convert Bytes to Vec<u8>
        });

        match tx.unbounded_send(Message::text(message.to_string())) {
            Ok(_) => { /* Implement Logging */ }
            Err(err) => println!("Error sending message to {}: {:?}", addr, err),
        }
    });
}

fn loop_forever(mut connection: rumqttc::Connection, peer_map: &PeerMap) {
    let (ip, port) = connection.eventloop.mqtt_options.broker_address();
    let source = format!("{}:{}", ip, port);
    for (_i, notification) in connection.iter().enumerate() {
        match notification {
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(p))) => {
                send_message_to_peers(peer_map, &source, &p.topic, &p.payload);
            }
            Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(a))) => {
                println!("Connection event: {:?}", a);
            }
            Ok(_) => {
                // Handle other types of events if necessary
            }
            Err(err) => {
                println!("Connection error: {:?}", err);
                // TODO implement error message
                let payload = bytes::Bytes::from("Connection failed");
                send_message_to_peers(peer_map, &source, "error", &payload);
                break;
            }
        }
    }
}

fn connect_to_mqtt_client(mqtt_host: &SocketAddr, mqtt_map: MqttMap, peer_map: PeerMap) -> () {
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

fn connect_to_broker(mqtt_ip: &str, mqtt_port: &str, peer_map: PeerMap, mqtt_map: MqttMap) {
    let mqtt_host: SocketAddr = format!("{}:{}", mqtt_ip, mqtt_port)
        .parse()
        .expect("Failed to parse MQTT host and port");
    let mqtt_map_clone = mqtt_map.clone();
    let peer_map_clone = peer_map.clone();

    std::thread::spawn(move || {
        connect_to_mqtt_client(&mqtt_host, mqtt_map_clone, peer_map_clone);
    });
}

fn deserialize_json_rpc_and_process(json_rpc: &str, peer_map: PeerMap, mqtt_map: MqttMap) -> () {
    let result = jsonrpc::deserialize_json_rpc(json_rpc);
    if result.is_err() {
        println!("Error deserializing JSON-RPC: {:?}", result.err());
        return;
    }
    let message = result.unwrap();
    println!(
        "Got method \"{}\" with params {}",
        message.method,
        message.params.clone().unwrap()
    );
    match message.method.as_str() {
        "connect" => {
            let (ip, port) = jsonrpc::get_ip_and_port(message.params);
            connect_to_broker(&ip, &port, peer_map, mqtt_map);
        }
        "publish" => {
            let (ip, port, topic, payload) = jsonrpc::get_ip_port_topic_and_payload(message.params);
            mqtt::publish_message(&ip, &port, &topic, &payload, mqtt_map);
            // Implement publishing to a topic
        }
        _ => {
            // Implement other methods
        }
    }
    // Implement deserialization and processing of JSON-RPC message
}

#[tokio::main]
async fn main() -> () {
    let args: Vec<String> = std::env::args().collect();
    let static_files = if args.len() < 2 {
        "../frontend/wwwroot".to_string()
    } else {
        args[1].clone()
    };

    let mqtt_map = MqttMap::new(Mutex::new(HashMap::new()));

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3030);
    let peer_map = PeerMap::new(Mutex::new(HashMap::new()));

    let ws = warp::path("ws")
        .and(warp::ws())
        .and(warp::addr::remote())
        .map(move |ws: warp::ws::Ws, addr: Option<SocketAddr>| {
            let peer_map = peer_map.clone();
            let mqtt_map = mqtt_map.clone();
            ws.on_upgrade(move |socket| async move {
                let (ws_tx, ws_rx) = socket.split();
                let (tx, rx) = unbounded();

                if let Some(addr) = addr {
                    println!("Received new WebSocket connection from {}", addr);
                    peer_map.lock().unwrap().insert(addr, tx);
                }
                let incoming = rx.map(Ok)
                    .forward(ws_tx);

                let handler = ws_rx.try_for_each(|msg| {
                    if let Ok(text) = msg.to_str() {
                        deserialize_json_rpc_and_process(
                            text,
                            peer_map.clone(),
                            mqtt_map.clone(),
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

    println!("Listening for connections on {} using static files from {}", server_addr, static_files);
    let routes = warp::get().and(ws.or(warp::fs::dir(static_files)));
    let warp_handle = tokio::spawn(async move {
        warp::serve(routes).run(server_addr).await;
    });

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for event");
    println!("\nReceived ctrl-c. Shutting down");
    warp_handle.abort();
}
