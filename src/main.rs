use actix_files::NamedFile;
use actix_web::{
    get,
    post,
    App,
    HttpResponse,
    HttpServer,
    Responder,
    web::{
        Json as WebJson,
        Path as WebPath,
        Data
    },
    middleware::Logger,
    Result as WebResult
};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::json;
use sqlx::{FromRow, Pool, Postgres, postgres::PgPoolOptions, migrate};
use walkdir::WalkDir;
use dotenv::dotenv;
use log::{debug, info};
use std::{error::Error, collections::HashMap, io::Result as IOResult, path::PathBuf};
use html_minifier::minify;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use handlebars::{Handlebars, handlebars_helper};
use serde::ser::SerializeTuple;
#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref FILE_CACHE: HashMap<String, String> = initialize_cache().unwrap_or_default();
}

static UNITS: [&'static str; 5] = [
    "Times",
    "Rakah",
    "Minutes",
    "Hours",
    "Days"
];

handlebars_helper!(readable_date: |dt: DateTime<Utc>| dt.format("%d %h, %Y %l:%M %p").to_string());

#[derive(Debug, Clone)]
pub enum Unit {
    Times(i32),
    Rakah(i32),
    Minutes(i32),
    Hours(i32),
    Days(i32),
    Taka(i32)
}
impl Unit {
    fn name(&self) -> &str {
        match self {
            Unit::Times(_) => "Times",
            Unit::Rakah(_) => "Rakah",
            Unit::Minutes(_) => "Minutes",
            Unit::Hours(_) => "Hours",
            Unit::Days(_) => "Days",
            Unit::Taka(_) => "Taka"
        }
    }
}
impl From<(&String, i32)> for Unit {
    fn from(value: (&String, i32)) -> Self {
        let (name, n) = value;
        match name.as_str() {
            "Times" => Unit::Times(n),
            "Rakah" => Unit::Rakah(n),
            "Minutes" => Unit::Minutes(n),
            "Hours" => Unit::Hours(n),
            "Days" => Unit::Days(n),
            "Taka" => Unit::Taka(n),
            _ => panic!("Cannot convert to `Unit` enum from {:?}", name)
        }
    }
}

fn initialize_cache() -> Result<HashMap<String, String>, Box<dyn Error>> {
    let filenames = get_filenames("pages")?;
    debug!("Cached Files: {:?}", filenames);

    let mut file_contents = HashMap::new();

    for filename in filenames {
        let content = get_file_content(&filename)?;
        file_contents.insert(filename, content);
    }

    Ok(file_contents)
}

fn get_file_content(filename: &str) -> Result<String, Box<dyn Error>> {
    debug!("loading file: {}", filename);
    let hb = Handlebars::new();
    let content = minify(std::fs::read_to_string(filename)?)?;
    let rendered_content =
        if filename == "pages/index.html" {
            hb.render_template(
                &content,
                &json!({
                    "units": UNITS
                })
            )?
        }
        else {
            content
        };
    Ok(rendered_content)
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
        let maybe_content = FILE_CACHE.get(&fullpath);
        match maybe_content {
            Some(content) => Ok(content.to_owned()),
            None => get_file_content(&fullpath)
        }
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

#[derive(Debug, Serialize, Deserialize)]
struct StaticResource {
    directory: String,
    filename: String
}

#[get(r#"/{directory:assets|styles}/{filename:.*[\.css|\.png|\.ico|\.webmanifest]}"#)]
async fn static_resources(resource: WebPath<StaticResource>) -> WebResult<NamedFile> {
    debug!("path: {:?}", &resource);
    let mut path = PathBuf::from(&resource.directory);
    path.push(
        &resource.filename
        .parse::<PathBuf>()
        .unwrap()
    );
    
    Ok(NamedFile::open(path)?)
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
            Unit::Days(n) => unit_tuple.serialize_element(n),
            Unit::Taka(n) => unit_tuple.serialize_element(n)
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
            "Taka" => Ok(Unit::Taka(amount)),
            _ => Err(serde::de::Error::custom("Unknown variant")),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateRemedyModel {
    pub condition: String,
    pub statement: String,
    pub unit: Option<Unit>
}

#[derive(Debug, Serialize, Deserialize)]
struct CreateOathModel {
    target: String,
    remedies: Vec<CreateRemedyModel>,
    penalty: String,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    ends_at_alias: String
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
    pub unit: Option<String>,
    pub created_at: DateTime<Utc>
}

#[automatically_derived]
impl ::sqlx::encode::Encode<'_, ::sqlx::Postgres> for RemedySchema
{
    fn encode_by_ref(
        &self,
        buf: &mut ::sqlx::postgres::PgArgumentBuffer,
    ) -> ::sqlx::encode::IsNull {
        let mut encoder = ::sqlx::postgres::types::PgRecordEncoder::new(buf);
        encoder.encode(&self.id);
        encoder.encode(&self.condition);
        encoder.encode(&self.statement);
        encoder.encode(&self.n);
        encoder.encode(&self.unit);
        encoder.encode(&self.created_at);
        encoder.finish();
        ::sqlx::encode::IsNull::No
    }
    fn size_hint(&self) -> ::std::primitive::usize {
        6usize * (4 + 4)
            + <i32 as ::sqlx::encode::Encode<::sqlx::Postgres>>::size_hint(&self.id)
            + <String as ::sqlx::encode::Encode<
                ::sqlx::Postgres,
            >>::size_hint(&self.condition)
            + <String as ::sqlx::encode::Encode<
                ::sqlx::Postgres,
            >>::size_hint(&self.statement)
            + <Option<
                i32,
            > as ::sqlx::encode::Encode<::sqlx::Postgres>>::size_hint(&self.n)
            + <Option<
                String,
            > as ::sqlx::encode::Encode<::sqlx::Postgres>>::size_hint(&self.unit)
            + <DateTime<
                Utc,
            > as ::sqlx::encode::Encode<::sqlx::Postgres>>::size_hint(&self.created_at)
    }
}
#[automatically_derived]
impl<'r> ::sqlx::decode::Decode<'r, ::sqlx::Postgres> for RemedySchema
{
    fn decode(
        value: ::sqlx::postgres::PgValueRef<'r>,
    ) -> ::std::result::Result<
        Self,
        ::std::boxed::Box<
            dyn ::std::error::Error + 'static + ::std::marker::Send + ::std::marker::Sync,
        >,
    > {
        let mut decoder = ::sqlx::postgres::types::PgRecordDecoder::new(value)?;
        let id = decoder.try_decode::<i32>()?;
        let condition = decoder.try_decode::<String>()?;
        let statement = decoder.try_decode::<String>()?;
        let n = decoder.try_decode::<Option<i32>>()?;
        let unit = decoder.try_decode::<Option<String>>()?;
        let created_at = decoder.try_decode::<DateTime<Utc>>()?;
        ::std::result::Result::Ok(RemedySchema {
            id,
            condition,
            statement,
            n,
            unit,
            created_at,
        })
    }
}
#[automatically_derived]
impl ::sqlx::Type<::sqlx::Postgres> for RemedySchema {
    fn type_info() -> ::sqlx::postgres::PgTypeInfo {
        ::sqlx::postgres::PgTypeInfo::with_name("RemedySchema")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OathSchema {
    pub id:            i32,
    pub target:        String,
    pub penalty:       String,
    pub created_at:    DateTime<Utc>,
    pub starts_at:     DateTime<Utc>,
    pub ends_at:       DateTime<Utc>,
    pub ends_at_alias: String,
    pub remedies:      Option<Vec<RemedySchema>>
}

#[derive(Debug, Serialize, Deserialize)]
struct ViewRemedyModel {
    id:         i32,
    condition:  String,
    statement:  String,
    n:          Option<i32>,
    unit:       Option<String>,
    created_at: DateTime<Utc>
}
impl From<&RemedySchema> for ViewRemedyModel {
    fn from(remedy_schema: &RemedySchema) -> Self {
        ViewRemedyModel {
            id:         remedy_schema.id,
            condition:  remedy_schema.condition.clone(),
            statement:  remedy_schema.statement.clone(),
            unit:       remedy_schema.unit.to_owned(),
            n:          remedy_schema.n,
            created_at: remedy_schema.created_at
        }
    }
}


#[derive(Debug, Serialize, Deserialize)]
struct ViewTimeSpanModel {
    d: i64,
    h: i64,
    m: i64,
    s: i64
}

#[derive(Debug, Serialize, Deserialize)]
struct ViewOathModel {
    id:            i32,
    target:        String,
    penalty:       String,
    created_at:    DateTime<Utc>,
    starts_at:     DateTime<Utc>,
    ends_at:       DateTime<Utc>,
    ends_at_alias: String,
    is_ongoing:    bool,
    is_expired:    bool,
    remaining:     ViewTimeSpanModel,
    remedies:      Option<Vec<ViewRemedyModel>>
}
impl From<&OathSchema> for ViewOathModel {
    fn from(oath_schema: &OathSchema) -> Self {
        let time_diff = Utc::now().signed_duration_since(oath_schema.created_at);
        ViewOathModel {
            id:            oath_schema.id,
            target:        oath_schema.target.clone(),
            penalty:       oath_schema.penalty.clone(),
            starts_at:     oath_schema.starts_at,
            ends_at:       oath_schema.ends_at,
            ends_at_alias: oath_schema.ends_at_alias.clone(),
            created_at:    oath_schema.created_at,
            is_ongoing:    oath_schema.starts_at <= Utc::now() && Utc::now() < oath_schema.ends_at,
            is_expired:    Utc::now() > oath_schema.ends_at,
            remaining: ViewTimeSpanModel {
                d: time_diff.num_days(),
                h: time_diff.num_hours() % 24,
                m: time_diff.num_minutes() % 60,
                s: time_diff.num_seconds() % 60
            },
            remedies: oath_schema.remedies
                .as_ref()
                .map(|remedies|
                    remedies
                    .into_iter()
                    .map(|r| ViewRemedyModel::from(r))
                    .collect::<Vec<ViewRemedyModel>>()
                )
        }
    }
}

#[post("/save-target")]
async fn save_target(
    req_body: WebJson<CreateOathModel>,
    data: Data<AppState>
) -> impl Responder {
    let oath_model = &req_body.0;

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
                Unit::Taka(n) => n.to_owned()
            }
        }))
        .collect();

    let query_result = sqlx::query_as!(
        OathSchema,
        r#"
        WITH inserted_oath AS (
            INSERT INTO oath (target, penalty, starts_at, ends_at, ends_at_alias, user_id)
            VALUES ($1, $2, $3, $4, $5, 1)
            RETURNING id, target, penalty, starts_at, ends_at, ends_at_alias, created_at
        ), inserted_remedy AS (
            INSERT INTO remedy (condition, statement, n, unit, oath_id)
            SELECT
                condition,
                statement,
                n,
                unit,
                (SELECT id FROM inserted_oath)
            FROM UNNEST(
                $6::text[],
                $7::text[],
                $8::int[],
                $9::text[]
            ) AS remedy_input(
                condition,
                statement,
                n,
                unit
            )
            RETURNING id, condition, statement, n, unit, created_at, oath_id
        )
        SELECT
            O.id,
            O.target,
            O.penalty,
            O.starts_at,
            O.ends_at,
            O.ends_at_alias,
            O.created_at,
            ARRAY_AGG((
                R.id,
                R.condition,
                R.statement,
                R.n,
                R.unit,
                R.created_at
            )) AS "remedies: Vec<RemedySchema>"
        FROM inserted_oath O
        JOIN inserted_remedy R ON R.oath_id = O.id
        GROUP BY
            O.id,
            O.target,
            O.penalty,
            O.starts_at,
            O.ends_at,
            O.ends_at_alias,
            O.created_at
        "#,
        oath_model.target,
        oath_model.penalty,
        oath_model.starts_at,
        oath_model.ends_at,
        oath_model.ends_at_alias,
        &conditions,
        &statements,
        &amounts as _,
        &units as _
    )
    .fetch_one(&data.db)
    .await;

    match query_result {
        Ok(oath) => {
            debug!("Oath: {:?}", &oath);
            HttpResponse::Created().body("Oath Created")
        }
        Err(e) => {
            HttpResponse::BadRequest().body(e.to_string())
        }
    }
}

#[get("/oath")]
async fn get_all_oaths_for_user(
    data: Data<AppState>
) -> impl Responder {
    let user_id = 1;// path.into_inner();
    let query_result = sqlx::query_as!(
        OathSchema,
        r#"
        SELECT
            O.id,
            O.target,
            O.penalty,
            O.starts_at,
            O.ends_at,
            O.ends_at_alias,
            O.created_at,
            ARRAY_AGG((
                R.id,
                R.condition,
                R.statement,
                R.n,
                R.unit,
                R.created_at
            )) AS "remedies: Vec<RemedySchema>"
        FROM oath O
        JOIN remedy R ON R.oath_id = O.id
        WHERE O.user_id = $1
        GROUP BY
            O.id,
            O.target,
            O.penalty,
            O.starts_at,
            O.ends_at,
            O.ends_at_alias,
            O.created_at
        ORDER BY O.created_at DESC
        "#,
        user_id
    )
    .fetch_all(&data.db)
    .await;

    let content = get_from_cache_or_file("list.html").unwrap();

    let mut hb = Handlebars::new();
    hb.register_helper("readable_date", Box::new(readable_date));

    match query_result {
        Ok(oaths) => {
            let models: Vec<ViewOathModel> = oaths.iter().map(|o| ViewOathModel::from(o)).collect();
            debug!("ViewModel: {:?}", models);
            let rendered = hb.render_template(&content, &models).unwrap();
            HttpResponse::Ok().body(rendered)
        },
        Err(e) => HttpResponse::BadRequest().body(e.to_string())
    }
}

#[actix_web::main]
async fn main() -> IOResult<()> {
    println!("Program starts here");
    dotenv().ok();

    let log_level = std::env::var("LOG_LEVEL").unwrap_or(String::from("info"));

    env_logger::init_from_env(env_logger::Env::default().default_filter_or(log_level));

    let app_url = std::env::var("APP_URL").expect("APP_URL must be set");
    let app_port =
        std::env::var("APP_PORT")
        .map_err(|_| "APP_PORT must be set")
        .and_then(|p| p.parse::<u16>().map_err(|_| "APP_PORT is not a positive integer"))
        .expect("APP_PORT is not valid");
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = match PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
    {
        Ok(pool) => {
            let migrations = migrate!().run(&pool).await;
            info!("✅Connection to the database is successful!");
            match migrations {
                Ok(()) => info!("✅DB migrations successful!"),
                Err(e) => info!("❌DB migrations failed: {:?}", e.to_string())
            }
            pool
        }
        Err(err) => {
            info!("🔥 Failed to connect to the database: {:?}", err);
            std::process::exit(1);
        }
    };

    HttpServer::new(move || {
        App::new()
            .app_data(actix_web::web::Data::new(AppState { db: pool.clone() }))
            .wrap(Logger::default())
            .service(index)
            .service(static_resources)
            .service(save_target)
            .service(get_all_oaths_for_user)
    })
    .bind((app_url, app_port))?
    .run()
    .await
}