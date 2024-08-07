use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use env_logger;
use log::{error, info};
use redis::aio::MultiplexedConnection;
use redis::Client as RedisClient;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::{io::Cursor, str::FromStr, sync::Arc};
use tokio::sync::Mutex;

mod templates;

const CSV_URL: &str = "https://aqua.kingcounty.gov/elections/2024/aug-primary/webresults.csv";
const CACHE_KEY: &str = "election_data";
const CACHE_EXPIRATION: u64 = 5; // 5 seconds

#[derive(Clone)]
struct AppState {
    redis: Arc<Mutex<MultiplexedConnection>>,
}

#[derive(Debug, Clone, Copy)]
struct QuotedFloat(f64);

impl<'de> Deserialize<'de> for QuotedFloat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // This will accept both string and number representations
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StringOrFloat {
            String(String),
            Float(f64),
        }

        let value = StringOrFloat::deserialize(deserializer)?;
        match value {
            StringOrFloat::String(s) => {
                let trimmed = s.trim_matches(|c| c == '"' || c == ' ');
                f64::from_str(trimmed)
                    .map(QuotedFloat)
                    .map_err(serde::de::Error::custom)
            }
            StringOrFloat::Float(f) => Ok(QuotedFloat(f)),
        }
    }
}

impl Serialize for QuotedFloat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ElectionData {
    #[serde(rename = "GEMS Contest ID")]
    gems_contest_id: i32,
    #[serde(rename = "Contest Sort Seq")]
    contest_sort_seq: i32,
    #[serde(rename = "District Type")]
    district_type: String,
    #[serde(rename = "District Type Subheading")]
    district_type_subheading: String,
    #[serde(rename = "District Name")]
    district_name: String,
    #[serde(rename = "Ballot Title")]
    ballot_title: String,
    #[serde(rename = "Ballots Counted for District")]
    ballots_counted_for_district: i32,
    #[serde(rename = "Registered Voters for District")]
    registered_voters_for_district: i32,
    #[serde(rename = "Percent Turnout for District")]
    percent_turnout_for_district: QuotedFloat,
    #[serde(rename = "Candidate Sort Seq")]
    candidate_sort_seq: i32,
    #[serde(rename = "Ballot Response")]
    ballot_response: String,
    #[serde(rename = "Party Preference")]
    party_preference: Option<String>,
    #[serde(rename = "Votes")]
    votes: i32,
    #[serde(rename = "Percent of Votes")]
    percent_of_votes: QuotedFloat,
}

async fn fetch_and_parse_csv() -> Result<Vec<ElectionData>, Box<dyn std::error::Error>> {
    let response = reqwest::get(CSV_URL).await?.text().await?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(Cursor::new(response));
    let mut parsed_data: Vec<ElectionData> = Vec::new();

    for result in reader.deserialize() {
        let record: ElectionData = result?;
        parsed_data.push(record);
    }

    Ok(parsed_data)
}

async fn get_all_data(data: web::Data<AppState>) -> Result<Vec<ElectionData>, actix_web::Error> {
    let mut redis = data.redis.lock().await;

    // Try to get cached data
    let cached_data: Option<String> = redis::cmd("GET")
        .arg(CACHE_KEY)
        .query_async(&mut *redis)
        .await
        .map_err(|e| {
            error!("Redis error: {}", e);
            actix_web::error::ErrorInternalServerError("Redis error")
        })?;

    match cached_data {
        Some(data) => {
            // If we have cached data, parse and return it
            serde_json::from_str(&data).map_err(|e| {
                error!("JSON deserialization error: {}", e);
                actix_web::error::ErrorInternalServerError("Data parsing error")
            })
        }
        None => {
            // If no cached data, fetch and parse CSV
            let parsed_data = fetch_and_parse_csv().await.map_err(|e| {
                error!("CSV fetch and parse error: {}", e);
                actix_web::error::ErrorInternalServerError("Data fetch error")
            })?;

            // Cache the new data
            let json_data = serde_json::to_string(&parsed_data).map_err(|e| {
                error!("JSON serialization error: {}", e);
                actix_web::error::ErrorInternalServerError("Data serialization error")
            })?;

            let _: () = redis::cmd("SETEX")
                .arg(CACHE_KEY)
                .arg(CACHE_EXPIRATION)
                .arg(&json_data)
                .query_async(&mut *redis)
                .await
                .map_err(|e| {
                    error!("Redis caching error: {}", e);
                    actix_web::error::ErrorInternalServerError("Redis caching error")
                })?;

            Ok(parsed_data)
        }
    }
}

async fn get_all_data_handler(data: web::Data<AppState>) -> impl Responder {
    match get_all_data(data).await {
        Ok(election_data) => HttpResponse::Ok().json(election_data),
        Err(e) => {
            error!("Failed to fetch data: {}", e);
            HttpResponse::InternalServerError().body("Failed to fetch data")
        }
    }
}

async fn get_contest_data(data: web::Data<AppState>, contest_id: web::Path<i32>) -> impl Responder {
    match get_all_data(data).await {
        Ok(all_data) => {
            let contest_data: Vec<ElectionData> = all_data
                .into_iter()
                .filter(|record| record.gems_contest_id == *contest_id)
                .collect();
            HttpResponse::Ok().json(contest_data)
        }
        Err(e) => {
            error!("Failed to get contest data: {}", e);
            HttpResponse::InternalServerError().body("Failed to get data")
        }
    }
}

async fn get_summary_statistics(data: web::Data<AppState>) -> impl Responder {
    match get_all_data(data).await {
        Ok(all_data) => {
            let total_votes: i32 = all_data.iter().map(|record| record.votes).sum();
            let total_registered_voters: i32 = all_data
                .iter()
                .map(|record| record.registered_voters_for_district)
                .sum();
            let average_turnout: f64 = all_data
                .iter()
                .map(|record| record.percent_turnout_for_district.0)
                .sum::<f64>()
                / all_data.len() as f64;

            let summary = serde_json::json!({
                "total_votes": total_votes,
                "total_registered_voters": total_registered_voters,
                "average_turnout_percentage": average_turnout,
            });

            HttpResponse::Ok().json(summary)
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to get data"),
    }
}

#[derive(Debug, Serialize)]
struct BallotInfo {
    contest_id: i32,
    ballot_title: String,
}

async fn get_ballot_titles(data: web::Data<AppState>) -> impl Responder {
    match get_all_data(data).await {
        Ok(all_data) => {
            let mut ballot_map: HashMap<String, i32> = HashMap::new();

            for record in all_data {
                ballot_map
                    .entry(record.ballot_title)
                    .or_insert(record.gems_contest_id);
            }

            let mut ballot_info: Vec<BallotInfo> = ballot_map
                .into_iter()
                .map(|(ballot_title, contest_id)| BallotInfo {
                    contest_id,
                    ballot_title,
                })
                .collect();

            ballot_info.sort_by(|a, b| a.ballot_title.cmp(&b.ballot_title));

            HttpResponse::Ok().json(ballot_info)
        }
        Err(e) => {
            error!("Failed to get ballot titles: {}", e);
            HttpResponse::InternalServerError().body("Failed to get data")
        }
    }
}

// New HTML endpoints

async fn index(data: web::Data<AppState>) -> impl Responder {
    match get_all_data(data).await {
        Ok(all_data) => {
            let mut ballot_map: HashMap<String, i32> = HashMap::new();
            for record in all_data {
                ballot_map
                    .entry(record.ballot_title.clone())
                    .or_insert(record.gems_contest_id);
            }

            let mut ballot_info: Vec<BallotInfo> = ballot_map
                .into_iter()
                .map(|(ballot_title, contest_id)| BallotInfo {
                    contest_id,
                    ballot_title,
                })
                .collect();

            ballot_info.sort_by(|a, b| a.ballot_title.cmp(&b.ballot_title));

            HttpResponse::Ok()
                .content_type("text/html")
                .body(templates::index(&ballot_info).into_string())
        }
        Err(e) => {
            error!("Failed to get ballot titles: {}", e);
            HttpResponse::InternalServerError().body("Failed to load page")
        }
    }
}

async fn get_summary_statistics_html(data: web::Data<AppState>) -> impl Responder {
    match get_all_data(data).await {
        Ok(all_data) => {
            let total_votes: i32 = all_data.iter().map(|record| record.votes).sum();
            let total_registered_voters: i32 = all_data
                .iter()
                .map(|record| record.registered_voters_for_district)
                .sum();
            let average_turnout: f64 = all_data
                .iter()
                .map(|record| record.percent_turnout_for_district.0)
                .sum::<f64>()
                / all_data.len() as f64;

            let markup = templates::summary_statistics(
                total_votes,
                total_registered_voters,
                average_turnout,
            );
            HttpResponse::Ok()
                .content_type("text/html")
                .body(markup.into_string())
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to get data"),
    }
}

async fn get_ballot_titles_html(data: web::Data<AppState>) -> impl Responder {
    match get_all_data(data).await {
        Ok(all_data) => {
            let mut ballot_map: HashMap<String, i32> = HashMap::new();
            for record in all_data {
                ballot_map
                    .entry(record.ballot_title.clone())
                    .or_insert(record.gems_contest_id);
            }

            let mut ballot_info: Vec<BallotInfo> = ballot_map
                .into_iter()
                .map(|(ballot_title, contest_id)| BallotInfo {
                    contest_id,
                    ballot_title,
                })
                .collect();

            ballot_info.sort_by(|a, b| a.ballot_title.cmp(&b.ballot_title));

            let markup = templates::ballot_titles(&ballot_info);
            HttpResponse::Ok()
                .content_type("text/html")
                .body(markup.into_string())
        }
        Err(e) => {
            error!("Failed to get ballot titles: {}", e);
            HttpResponse::InternalServerError().body("Failed to get data")
        }
    }
}

#[derive(Deserialize)]
struct ContestQuery {
    contest: i32,
}
async fn get_contest_data_html(
    data: web::Data<AppState>,
    query: web::Query<ContestQuery>,
) -> impl Responder {
    let contest_id = query.contest;
    match get_all_data(data).await {
        Ok(all_data) => {
            let mut contest_data: Vec<ElectionData> = all_data
                .into_iter()
                .filter(|record| record.gems_contest_id == contest_id)
                .collect();

            if contest_data.is_empty() {
                return HttpResponse::Ok().body("No data available for this contest.");
            }

            contest_data.sort_by(|a, b| {
                b.percent_of_votes
                    .0
                    .partial_cmp(&a.percent_of_votes.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let markup = templates::contest_details(&contest_data);
            HttpResponse::Ok()
                .content_type("text/html")
                .body(markup.into_string())
        }
        Err(_) => HttpResponse::InternalServerError().body("Failed to get data"),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    info!("Hello!");
    let redis_client = RedisClient::open("redis://127.0.0.1/")
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let app_state = AppState {
        redis: Arc::new(Mutex::new(redis_conn)),
    };

    HttpServer::new(move || {
        // Create a CORS middleware
        let cors = Cors::permissive();

        App::new()
            .wrap(cors) // Add this line to wrap the entire app with CORS middleware
            .app_data(web::Data::new(app_state.clone()))
            .route("/", web::get().to(index))
            .route("/election-data", web::get().to(get_all_data_handler))
            .route(
                "/election-data/contest/{contest_id}",
                web::get().to(get_contest_data),
            )
            .route(
                "/election-data/summary",
                web::get().to(get_summary_statistics),
            )
            .route(
                "/election-data/ballot-titles",
                web::get().to(get_ballot_titles),
            )
            // HTML endpoints
            .route(
                "/election-data/summary-html",
                web::get().to(get_summary_statistics_html),
            )
            .route(
                "/election-data/ballot-titles-html",
                web::get().to(get_ballot_titles_html),
            )
            .route(
                "/election-data/contest-html",
                web::get().to(get_contest_data_html),
            )
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
