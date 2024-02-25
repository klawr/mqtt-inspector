use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug)]
pub enum JsonRpcError {
    DeserializationError(serde_json::Error),
}

pub fn deserialize_json_rpc(json_rpc: &str) -> Result<JsonRpcNotification, JsonRpcError> {
    match serde_json::from_str(json_rpc) {
        Ok(result) => Ok(result),
        Err(error) => Err(JsonRpcError::DeserializationError(error)),
    }
}

pub fn get_ip_and_port(params: serde_json::Value) -> (String, String) {
    let ip = params["ip"].as_str().unwrap();
    let port = params["port"].as_str().unwrap();

    (ip.to_string(), port.to_string())
}

pub fn get_ip_port_topic_and_payload(
    params: serde_json::Value,
) -> (String, String, String, String) {
    let ip = params["ip"].as_str().unwrap();
    let port = params["port"].as_str().unwrap();
    let topic = params["topic"].as_str().unwrap();
    let payload = params["payload"].as_str().unwrap();

    (
        ip.to_string(),
        port.to_string(),
        topic.to_string(),
        payload.to_string(),
    )
}
