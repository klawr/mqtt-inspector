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

use super::jsonrpc;
use std::fs;

use futures_channel::mpsc::UnboundedSender;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct CommandMessage {
    name: String,
    topic: String,
    payload: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct PipelineEntry {
    topic: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct PipelineMessage {
    name: String,
    pipeline: Vec<PipelineEntry>,
}

pub fn get_known_brokers(brokers_path: &String) -> Vec<String> {
    if let Ok(file_content) = fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        eprintln!("Failed to read file {}", brokers_path);
        Vec::new()
    }
}

pub fn add_to_brokers(brokers_path: &String, broker: String) {
    let mut brokers: Vec<String> = if let Ok(file_content) = fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        eprintln!("Failed to read file {}", brokers_path);
        Vec::new()
    };

    brokers.push(broker);
    if let Ok(content) = serde_json::to_string(&brokers) {
        if let Err(_) = fs::write(brokers_path, content) {
            eprintln!("Failed to save new brokers file to {}", brokers_path);
        }
    } else {
        eprintln!("Failed to serialize updated saved brokers.");
    }
}

pub fn remove_from_brokers(brokers_path: &String, broker: String) {
    let mut brokers: Vec<String> = if let Ok(file_content) = fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        eprintln!("Failed to read file {}", brokers_path);
        Vec::new()
    };

    if let Some(index) = brokers.iter().position(|b| b == &broker) {
        brokers.remove(index);
        if let Ok(content) = serde_json::to_string(&brokers) {
            if let Err(_) = fs::write(brokers_path, content) {
                eprintln!("Failed to save new brokers file to {}", brokers_path);
            }
        } else {
            eprintln!("Failed to serialize updated saved brokers.");
        }
    } else {
        eprintln!("Broker {} not found in {}", broker, brokers_path);
    }
}

pub fn add_to_commands(commands_path: &String, params: serde_json::Value) {
    if let Ok(new_command) = serde_json::from_value::<CommandMessage>(params) {
        let new_command_path = std::format!("{}/{}.json", commands_path, new_command.name);
        if let Some(parent_dir) = std::path::Path::new(&new_command_path).parent() {
            fs::create_dir_all(parent_dir).expect("Failed to create directory path");
        }
        if let Ok(content) = serde_json::to_string(&new_command) {
            if let Err(_) = fs::write(&new_command_path, content) {
                eprintln!("Failed to save new commands file to {}", new_command_path);
            }
        } else {
            eprintln!("Failed to serialize updated saved commands.");
        }
    } else {
        println!("Could not deserialize new command.");
    }
}

pub fn remove_from_commands(commands_path: &String, params: serde_json::Value) {
    let command = params["name"].as_str().unwrap();
    let command_path = std::format!("{}/{}.json", commands_path, command);
    if let Err(_) = fs::remove_file(&command_path) {
        eprintln!("Failed to remove command file from {}", command_path);
    }
}

pub fn add_to_pipelines(pipelines_path: &String, params: serde_json::Value) {
    if let Ok(new_pipeline) = serde_json::from_value::<PipelineMessage>(params) {
        let new_pipeline_path = std::format!("{}/{}.json", pipelines_path, new_pipeline.name);
        if let Some(parent_dir) = std::path::Path::new(&new_pipeline_path).parent() {
            fs::create_dir_all(parent_dir).expect("Failed to create directory path");
        }
        if let Ok(content) = serde_json::to_string(&new_pipeline) {
            if let Err(_) = fs::write(&new_pipeline_path, content) {
                eprintln!("Failed to save new commands file to {}", new_pipeline_path);
            }
        } else {
            eprintln!("Failed to serialize updated saved commands.");
        }
    } else {
        println!("Could not deserialize new pipeline.");
    }
}

pub fn remove_from_pipelines(pipelines_path: &String, params: serde_json::Value) {
    let pipeline = params["name"].as_str().unwrap();
    let pipeline_path = std::format!("{}/{}.json", pipelines_path, pipeline);
    if let Err(_) = fs::remove_file(&pipeline_path) {
        eprintln!("Failed to remove pipeline file from {}", pipeline_path);
    }
}

pub fn send_commands(
    sender: &UnboundedSender<warp::filters::ws::Message>,
    commands_path: &String,
) -> () {
    if let Ok(commands) = fs::read_dir(commands_path) {
        let commands: Vec<CommandMessage> = commands
            .filter_map(|dir_entry| {
                if let Ok(file) = dir_entry {
                    if let Ok(file_content) = fs::read_to_string(file.path()) {
                        return serde_json::from_str(&file_content).ok();
                    }
                }
                None
            })
            .collect();

        let jsonrpc = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "commands".to_string(),
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
) -> () {
    if let Ok(pipelines) = fs::read_dir(pipelines_path) {
        let pipelines: Vec<PipelineMessage> = pipelines
            .filter_map(|dir_entry| {
                if let Ok(file) = dir_entry {
                    if let Ok(file_content) = fs::read_to_string(file.path()) {
                        return serde_json::from_str(&file_content).ok();
                    }
                }
                None
            })
            .collect();

        let jsonrpc = jsonrpc::JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "pipelines".to_string(),
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

pub fn send_configs(
    sender: &UnboundedSender<warp::filters::ws::Message>,
    config_path: &String,
) -> () {
    send_commands(sender, &format!("{}/commands", config_path));
    send_pipelines(sender, &format!("{}/pipelines", config_path));
}
