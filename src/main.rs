use actix_web::{get, post, App, HttpResponse, HttpServer, Responder, web::{Json, Data, self}, middleware::Logger};
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{FromRow, Pool, Postgres, postgres::PgPoolOptions};
use walkdir::WalkDir;
use dotenv::dotenv;
use log::{debug};
use std::{error::Error, collections::HashMap};
use html_minifier::minify;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use handlebars::Handlebars;
use serde::ser::SerializeTuple;
#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref FILE_CACHE: HashMap<String, String> = initialize_cache().unwrap_or_default();
}

#[derive(Debug, Serialize, Deserialize)]
struct Remedy {
    statement: String,
    n: u32,
    unit: String
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

#[derive(Debug, Clone)]
pub enum Unit {
    Times(i32),
    Rakah(i32),
    Minutes(i32),
    Hours(i32),
    Days(i32)
}
impl Unit {
    fn name(&self) -> &str {
        match self {
            Unit::Times(_) => "Times",
            Unit::Rakah(_) => "Rakah",
            Unit::Minutes(_) => "Minutes",
            Unit::Hours(_) => "Hours",
            Unit::Days(_) => "Days"
        }
    }
}

impl Serialize for Unit {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer {
        let mut unit_tuple = serializer.serialize_tuple(2)?;
        let _ = unit_tuple.serialize_element(&self.name());
        let _ = match self {
            Unit::Times(n) => unit_tuple.serialize_element(n),
            Unit::Rakah(n) => unit_tuple.serialize_element(n),
            Unit::Minutes(n) => unit_tuple.serialize_element(n),
            Unit::Hours(n) => unit_tuple.serialize_element(n),
            Unit::Days(n) => unit_tuple.serialize_element(n)
        };
        unit_tuple.end()
    }
}

impl<'de> Deserialize<'de> for Unit {
    fn deserialize<D>(deserializer: D) -> Result<Unit, D::Error>
    where
        D: Deserializer<'de>
    {
        let data: Vec<serde_json::Value> = Deserialize::deserialize(deserializer)?;

        // if data.len() != 2 {
        //     Err(serde::de::Error::custom("Expected a tuple with 2 elements"))
        // }

        let variant = data[0].as_str().ok_or_else(|| {
            serde::de::Error::custom("First element of the tuple should be a string representing the variant")
        })?;

        let amount = data[1].as_i64().ok_or_else(|| {
            serde::de::Error::custom("Second element of the tuple should be an integer representing the amount")
        })? as i32;

        match variant {
            "Times" => Ok(Unit::Times(amount)),
            "Rakah" => Ok(Unit::Rakah(amount)),
            "Minutes" => Ok(Unit::Minutes(amount)),
            "Hours" => Ok(Unit::Hours(amount)),
            "Days" => Ok(Unit::Days(amount)),
            _ => Err(serde::de::Error::custom("Unknown variant")),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RemedyModel {
    pub condition: String,
    pub statement: String,
    pub unit: Option<Unit>
}

#[derive(Debug, Serialize, Deserialize)]
struct OathModel {
    target: String,
    remedies: Vec<RemedyModel>,
    penalty: String,
    until: DateTime<Utc>
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub struct CreateUserSchema {
    pub id: i32,
    pub first_name: String,
    pub last_name: Option<String>,
    pub date_of_birth: NaiveDate,
    pub created_at: DateTime<Utc>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemedySchema {
    pub id: i32,
    pub condition: String,
    pub statement: String,
    pub n: Option<i32>,
    pub unit: Option<String>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OathSchema {
    pub id: i32,
    pub target: String,
    pub penalty: String,
    pub until: DateTime<Utc>
}

#[post("/save-target")]
// async fn save_target(req: String) -> impl Responder {
async fn save_target(
    req_body: Json<OathModel>,
    data: Data<AppState>
) -> impl Responder {
    let oath_model = &req_body.0;

    let query_result = sqlx::query_as!(
        OathSchema,
        r#"
        INSERT INTO oath (target, penalty, until, user_id)
        VALUES ($1, $2, $3, 1)
        RETURNING id, target, penalty, until
        "#,
        oath_model.target,
        oath_model.penalty,
        oath_model.until
    )
    .fetch_one(&data.db)
    .await;

    let oath = query_result.unwrap();
    debug!("Oath: {:?}", &oath);

    let conditions: Vec<String> =
        oath_model.remedies
        .iter()
        .map(|r| r.condition.clone())
        .collect();
    let statements: Vec<String> =
        oath_model.remedies
        .iter()
        .map(|r| r.statement.clone())
        .collect();
    let units: Vec<Option<&str>> =
        oath_model.remedies
        .iter()
        .map(|r| r.unit.as_ref().map(|u| u.name()))
        .collect();
    let amounts: Vec<Option<i32>> =
        oath_model.remedies
        .iter()
        .map(|r| r.unit.as_ref().map(|u| {
            match u {
                Unit::Times(n) => n.to_owned(),
                Unit::Rakah(n) => n.to_owned(),
                Unit::Minutes(n) => n.to_owned(),
                Unit::Hours(n) => n.to_owned(),
                Unit::Days(n) => n.to_owned(),
            }
        }))
        .collect();

    let query_result = sqlx::query_as!(
        RemedySchema,
        r#"
        INSERT INTO remedy (condition, statement, n, unit, oath_id)
        SELECT
            condition,
            statement,
            n,
            unit,
            $1
        FROM UNNEST(
            $2::text[],
            $3::text[],
            $4::int[],
            $5::text[]
        ) AS remedy_input(
            condition,
            statement,
            n,
            unit
        )
        RETURNING id, condition, statement, n, unit
        "#,
        oath.id,
        &conditions,
        &statements,
        &amounts as _,
        &units as _
    )
    .fetch_all(&data.db)
    .await;

    match query_result {
        Ok(remedies) => {
            debug!("Remedies: {:?}", &remedies);
            HttpResponse::Created().body("Oath Created")
        }
        Err(e) => {
            HttpResponse::BadRequest().body(e.to_string())
        }
    }
}

#[get("/oath/{id}")]
async fn get_all_oaths_for_user(path: web::Path<Option<i32>>) -> impl Responder {
    let content = path.into_inner();
    println!("{:?}", content);
    HttpResponse::Ok().body("body")
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
            .service(get_all_oaths_for_user)
    })
    .bind(("127.0.0.1", 12345))?
    .run()
    .await
}