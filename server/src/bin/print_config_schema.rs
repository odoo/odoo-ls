use std::fs::File;
use std::io::Write;
use serde_json::to_string_pretty;

use odoo_ls_server::core::config::config_json_schema;

fn main() {
    let schema = config_json_schema();
    let json = to_string_pretty(&schema).expect("Failed to serialize schema");
    let mut file = File::create("config_schema.json").expect("Failed to create file");
    file.write_all(json.as_bytes()).expect("Failed to write schema");
    println!("Schema written to config_schema.json");
}
