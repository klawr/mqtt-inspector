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
