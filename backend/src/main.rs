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

mod server;
mod utils {
    pub fn get_arguments_or_default() -> (String, String) {
        let args: Vec<String> = std::env::args().collect();
        let static_files = if args.len() < 2 {
            "../wwwroot".to_string()
        } else {
            args[1].to_string()
        };
        let config_dir = if args.len() < 3 {
            "../test/config".to_string()
        } else {
            args[2].to_string()
        };

        (static_files, config_dir)
    }
}

#[tokio::main]
async fn main() {
    let (static_files, config_dir) = utils::get_arguments_or_default();

    match std::fs::create_dir_all(&config_dir) {
        Ok(_) => {}
        Err(err) => {
            eprintln!(
                "Failed to create config directory: {}. Changes will not persist.",
                err
            );
        }
    }

    let warp_handle = server::run_server(static_files, config_dir);

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for event");
    println!("\nReceived ctrl-c. Shutting down");
    warp_handle.abort();
}
