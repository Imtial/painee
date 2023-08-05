use actix_web::{get, post, App, HttpResponse, HttpServer, Responder, web::{Json, Data}, middleware::Logger};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, Pool, Postgres, postgres::PgPoolOptions};
use walkdir::WalkDir;
use dotenv::dotenv;
use std::{error::Error, collections::HashMap};
use html_minifier::minify;
use serde::{Serialize, Deserialize};
// use handlebars::Handlebars;
// use serde_json::json;

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref FILE_CACHE: HashMap<String, String> = initialize_cache().unwrap_or_default();
}

fn initialize_cache() -> Result<HashMap<String, String>, Box<dyn Error>> {
    let filenames = get_filenames("pages")?;

    let mut file_contents = HashMap::new();

    for filename in filenames {
        let content = get_file_content(&filename)?;
        file_contents.insert(filename, content);
    }

    Ok(file_contents)
}

fn get_file_content(filename: &str) -> Result<String, Box<dyn Error>> {
    Ok(minify(std::fs::read_to_string(filename)?)?)
}

fn get_filenames(directory_path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(
        WalkDir::new(directory_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|dir| dir.path().is_file())
        .map(|dir|
            dir.path()
                .display()
                .to_string()
        )
        .collect()
    )
}

fn get_from_cache_or_file(filename: &str) ->Result<String, Box<dyn Error>> {
    let env =
        std::env::var("ENVIRONMENT")
        .unwrap_or(String::from("development"));
    
    let fullpath = format!("{}{}", "pages/", filename);

    if env == "production" {
        Ok(FILE_CACHE.get(&fullpath).unwrap().to_owned())
    }
    else {
        get_file_content(&fullpath)
    }
}

pub struct AppState {
    db: Pool<Postgres>
}

#[get("/")]
async fn index() -> impl Responder {
    let content = get_from_cache_or_file("index.html").unwrap();
    HttpResponse::Ok().body(content)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConditionalRemedy {
    pub condition: String,
    pub remedy: String
}

#[derive(Debug, Serialize, Deserialize)]
struct TargetData {
    target: String,
    remedies: Vec<ConditionalRemedy>,
    penalty: String,
    until: DateTime<Utc>
}

// impl TargetData {
//     fn to_entity(&self) -> OathEntity {
//         OathEntity {
//             id: None,
//             target: self.target.clone(),
//             remedies: types::Json(self.remedies.clone()),
//             penalty: self.penalty.clone(),
//             until: self.until
//         }
//     }
// }

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct CreateOathSchema {
    pub id: Option<i32>,
    pub target: String,
    pub conditional_remedies: sqlx::types::JsonValue,
    pub penalty: String,
    pub until: DateTime<Utc>
}

#[post("/save-target")]
async fn save_target(
    req_body: Json<TargetData>,
    data: Data<AppState>
) -> impl Responder {
    let target_data = &req_body.0;
    let query_result = sqlx::query_as!(
        CreateOathSchema,
        "INSERT INTO oath (target, conditional_remedies, penalty, \"until\")
        VALUES ($1, $2, $3, $4)
        RETURNING
            id,
            target,
            conditional_remedies,
            penalty,
            \"until\";",
        target_data.target,
        serde_json::json!(target_data.remedies),
        target_data.penalty,
        target_data.until
    )
    .fetch_one(&data.db)
    .await;

    match query_result {
        Ok(oath) => {
            HttpResponse::Ok().body(serde_json::to_string(&oath).unwrap())
        }
        Err(e) => {
            HttpResponse::BadRequest().body(e.to_string())
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    env_logger::init_from_env(env_logger::Env::default().default_filter_or("debug"));

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = match PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
    {
        Ok(pool) => {
            println!("✅Connection to the database is successful!");
            pool
        }
        Err(err) => {
            println!("🔥 Failed to connect to the database: {:?}", err);
            std::process::exit(1);
        }
    };

    HttpServer::new(move || {
        App::new()
            .app_data(actix_web::web::Data::new(AppState { db: pool.clone() }))
            .wrap(Logger::default())
            .service(index)
            .service(save_target)
    })
    .bind(("127.0.0.1", 12345))?
    .run()
    .await
}