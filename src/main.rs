use actix_web::{get, post, App, HttpResponse, HttpServer, Responder, web::Json, middleware::Logger};
use chrono::{DateTime, Utc};
use walkdir::WalkDir;
use std::{error::Error, collections::HashMap};
use html_minifier::minify;
use serde::{Serialize, Deserialize};
use handlebars::Handlebars;
use serde_json::json;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref FILE_CACHE: HashMap<String, String> = initialize_cache().unwrap_or_default();
}

fn initialize_cache() -> Result<HashMap<String, String>, Box<dyn Error>> {
    let filenames = get_filenames("pages")?;

    let mut file_contents = HashMap::new();

    for filename in filenames {
        let content = minify(std::fs::read_to_string(filename.as_str())?)?;
        file_contents.insert(filename, content);
    }

    Ok(file_contents)
}

fn get_filenames(directory_path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(
        WalkDir::new(directory_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|dir| dir.path().is_file())
        .map(|dir| dir.path().display().to_string())
        .collect()
    )
}

#[get("/")]
async fn hello() -> impl Responder {
    let content = FILE_CACHE.get("pages/index.html").unwrap().to_owned();
    HttpResponse::Ok().body(content)
}

#[derive(Debug, Serialize, Deserialize)]
struct TargetData {
    target: String,
    until: DateTime<Utc>
}

#[post("/saveTarget")]
async fn save_target(req_body: Json<TargetData>) -> impl Responder {
    HttpResponse::Ok().body(serde_json::to_string(&req_body).unwrap())
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .service(hello)
            .service(save_target)
    })
    .bind(("127.0.0.1", 12345))?
    .run()
    .await
}