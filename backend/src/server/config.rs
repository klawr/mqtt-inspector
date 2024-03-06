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

use std::collections::VecDeque;

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
    pipeline: VecDeque<PipelineEntry>,
}

pub fn get_known_brokers(brokers_path: &str) -> VecDeque<String> {
    if let Ok(file_content) = &std::fs::read_to_string(&brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| VecDeque::new())
    } else {
        eprintln!("Failed to read file {}", &brokers_path);
        VecDeque::new()
    }
}

pub fn add_to_brokers(brokers_path: &str, broker: &str) {
    let mut brokers: Vec<String> = if let Ok(file_content) = std::fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        eprintln!("Failed to read file {}", brokers_path);
        Vec::new()
    };

    brokers.push(broker.to_string());
    if let Ok(content) = serde_json::to_string(&brokers) {
        if let Err(_) = std::fs::write(brokers_path, content) {
            eprintln!("Failed to save new brokers file to {}", brokers_path);
        }
    } else {
        eprintln!("Failed to serialize updated saved brokers.");
    }
}

pub fn remove_from_brokers(brokers_path: &str, broker: &str) {
    let mut brokers: Vec<String> = if let Ok(file_content) = std::fs::read_to_string(brokers_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        eprintln!("Failed to read file {}", brokers_path);
        Vec::new()
    };

    if let Some(index) = brokers.iter().position(|b| b == broker) {
        brokers.remove(index);
        if let Ok(content) = serde_json::to_string(&brokers) {
            if let Err(_) = std::fs::write(brokers_path, content) {
                eprintln!("Failed to save new brokers file to {}", brokers_path);
            }
        } else {
            eprintln!("Failed to serialize updated saved brokers.");
        }
    } else {
        eprintln!("Broker {} not found in {}", broker, brokers_path);
    }
}

pub fn add_to_commands(commands_path: &str, params: serde_json::Value) {
    if let Ok(new_command) = serde_json::from_value::<CommandMessage>(params) {
        let new_command_path = std::format!("{}/{}.json", commands_path, new_command.name);
        if let Some(parent_dir) = std::path::Path::new(&new_command_path).parent() {
            std::fs::create_dir_all(parent_dir).expect("Failed to create directory path");
        }
        if let Ok(content) = serde_json::to_string(&new_command) {
            if let Err(_) = std::fs::write(&new_command_path, content) {
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
    if let Err(_) = std::fs::remove_file(&command_path) {
        eprintln!("Failed to remove command file from {}", command_path);
    }
}

pub fn add_to_pipelines(pipelines_path: &String, params: serde_json::Value) {
    if let Ok(new_pipeline) = serde_json::from_value::<PipelineMessage>(params) {
        let new_pipeline_path = std::format!("{}/{}.json", pipelines_path, new_pipeline.name);
        if let Some(parent_dir) = std::path::Path::new(&new_pipeline_path).parent() {
            std::fs::create_dir_all(parent_dir).expect("Failed to create directory path");
        }
        if let Ok(content) = serde_json::to_string(&new_pipeline) {
            if let Err(_) = std::fs::write(&new_pipeline_path, content) {
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
    if let Err(_) = std::fs::remove_file(&pipeline_path) {
        eprintln!("Failed to remove pipeline file from {}", pipeline_path);
    }
}
