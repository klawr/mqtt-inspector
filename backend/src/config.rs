use std::fs;

use futures_channel::mpsc::UnboundedSender;

use crate::jsonrpc::JsonRpcNotification;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct CommandMessage {
    name: String,
    topic: String,
    payload: String
}

pub fn add_to_commands(commands_path: &String, params: serde_json::Value) {
    let mut commands: Vec<CommandMessage> = if let Ok(file_content) = fs::read_to_string(commands_path) {
        serde_json::from_str(&file_content).unwrap_or_else(|_| Vec::new())
    } else {
        eprintln!("Failed to read file {}", commands_path);
        Vec::new()
    };

    if let Ok(new_command_message) = serde_json::from_value::<CommandMessage>(params) {
        commands.push(new_command_message);
        if let Ok(content) = serde_json::to_string(&commands)
        {
            if let Err(_) = fs::write(commands_path, content)
            {
                println!("Failed to save new commands file to {}", commands_path);
            }
        } else {
            println!("Failed to serialize updated saved commands.");
        }
    } else {
        println!("Could not deserialize new command.");
    }
}

fn send_commands(sender: &UnboundedSender<warp::filters::ws::Message>, commands_path: &String) -> () {
    if let Ok(commands) = fs::read_to_string(commands_path) {
        let jsonrpc = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "commands".to_string(),
            params: serde_json::json!(&commands)
        };

        if let Ok(serialized) = serde_json::to_string(&jsonrpc) {
            match sender.unbounded_send(warp::filters::ws::Message::text(serialized)) {
                Ok(_) => { /* Implement Logging */ }
                Err(err) => println!("Error sending message: {:?}", err),
            }
        } else {
            println!("Failed to serialize commands jsonjpc")
        }
    } else {
        println!("Failed to read commands file from {}", commands_path);
    }
}

pub fn send_configs(sender: &UnboundedSender<warp::filters::ws::Message>, config_path: &String) -> () {
    send_commands(sender, &format!("{}/commands.json", config_path));
}
