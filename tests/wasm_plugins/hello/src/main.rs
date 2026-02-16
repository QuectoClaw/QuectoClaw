use std::io::{self, Read};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Input {
    name: Option<String>,
}

fn main() {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap();
    
    let input: Input = serde_json::from_str(&buffer).unwrap_or(Input { name: None });
    let name = input.name.unwrap_or_else(|| "World".to_string());
    
    println!("Hello, {}! This is a WASM plugin.", name);
}
