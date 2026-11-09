use axum_boilerplate::openapi::ApiDoc;
use std::env;
use std::fs;
use utoipa::OpenApi;

fn main() {
    let spec = ApiDoc::openapi();
    let json_str = serde_json::to_string_pretty(&spec).expect("serialize openapi");
    if let Some(target) = env::args().nth(1) {
        fs::write(&target, json_str).expect("write openapi to file");
        println!("OpenAPI spec written to {target}");
    } else {
        println!("{json_str}");
    }
}
