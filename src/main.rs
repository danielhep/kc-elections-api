use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use env_logger;
use log::{error, info};
use maud::{html, Markup, Render};
use redis::aio::MultiplexedConnection;
use redis::Client as RedisClient;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::env;
use std::{io::Cursor, str::FromStr, sync::Arc};
use tokio::sync::Mutex;

mod templates;

const CACHE_KEY: &str = "election_data";
const CACHE_EXPIRATION: u64 = 60; // 60 seconds

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum PartyPreference {
    Democrat,
    Republican,
    NotAffiliated,
}

impl FromStr for PartyPreference {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lowercase = s.to_lowercase();
        if lowercase.contains("democrat") {
            Ok(PartyPreference::Democrat)
        } else if lowercase.contains("republican") {
            Ok(PartyPreference::Republican)
        } else {
            Ok(PartyPreference::NotAffiliated)
        }
    }
}

impl Render for PartyPreference {
    fn render(&self) -> Markup {
        match self {
            PartyPreference::Democrat => {
                html! { span class="elative select-none whitespace-nowrap rounded-lg bg-blue-900 py-0.5 px-1 font-sans text-xs font-bold uppercase text-white" { "Democrat" } }
            }
            PartyPreference::Republican => {
                html! { span class="elative select-none whitespace-nowrap rounded-lg bg-red-900 py-0.5 px-1 font-sans text-xs font-bold uppercase text-white" { "Republican" } }
            }
            PartyPreference::NotAffiliated => {
                html! { span.party-not-affiliated { "Not Affiliated" } }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Candidate {
    name: String,
    percentage: f64,
    votes: i32,
    party_preference: PartyPreference,
}

#[derive(Debug, Serialize, Deserialize)]
struct District {
    name: String,
    percent_turnout: f64,
    registered_voters: i32,
    ballots_counted: i32,
    district_type: String,
    district_type_subheading: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Contest {
    ballot_title: String,
    district: District,
    id: u32,
    candidates: Vec<Candidate>,
}

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
    gems_contest_id: u32,
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

async fn fetch_and_parse_csv() -> Result<Vec<Contest>, Box<dyn std::error::Error>> {
    let csv_url: String = env::var("CSV_URL").expect("No CSV URL provided.");
    let response = reqwest::get(csv_url).await?.text().await?;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(Cursor::new(response));
    let mut parsed_data: Vec<ElectionData> = Vec::new();

    for result in reader.deserialize() {
        let record: ElectionData = result?;
        parsed_data.push(record);
    }

    Ok(process_election_data(parsed_data))
}

fn process_election_data(data: Vec<ElectionData>) -> Vec<Contest> {
    let mut contests_map: HashMap<u32, Contest> = HashMap::new();

    for record in data {
        let contest_id = record.gems_contest_id;

        let contest = contests_map.entry(contest_id).or_insert_with(|| Contest {
            ballot_title: record.ballot_title.clone(),
            district: District {
                name: record.district_name.clone(),
                percent_turnout: record.percent_turnout_for_district.0,
                registered_voters: record.registered_voters_for_district,
                ballots_counted: record.ballots_counted_for_district,
                district_type: record.district_type.clone(),
                district_type_subheading: record.district_type_subheading.clone(),
            },
            id: contest_id,
            candidates: Vec::new(),
        });

        let candidate = Candidate {
            name: record.ballot_response,
            percentage: record.percent_of_votes.0,
            votes: record.votes,
            party_preference: PartyPreference::from_str(
                &record.party_preference.unwrap_or_default(),
            )
            .unwrap_or(PartyPreference::NotAffiliated),
        };

        contest.candidates.push(candidate);
    }

    contests_map.into_values().collect()
}

fn contests_by_ballot_title(contests: Vec<Contest>) -> HashMap<String, Vec<Contest>> {
    let mut contests_by_ballot_title: HashMap<String, Vec<Contest>> = HashMap::new();
    for contest in contests {
        contests_by_ballot_title
            .entry(contest.ballot_title.clone())
            .or_insert_with(Vec::new)
            .push(contest);
    }
    contests_by_ballot_title
}

async fn get_all_data(data: web::Data<AppState>) -> Result<Vec<Contest>, actix_web::Error> {
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

async fn index(data: web::Data<AppState>) -> impl Responder {
    match get_all_data(data).await.map(contests_by_ballot_title) {
        Ok(contests) => HttpResponse::Ok()
            .content_type("text/html")
            .body(templates::index(&contests).into_string()),
        Err(e) => {
            error!("Failed to get ballot titles: {}", e);
            HttpResponse::InternalServerError().body("Failed to load page")
        }
    }
}

async fn contest_page(data: web::Data<AppState>, path: web::Path<u32>) -> impl Responder {
    let contest_id = path.into_inner();
    match get_all_data(data).await {
        Ok(all_data) => {
            let mut contest = all_data.into_iter().find(|a| a.id == contest_id);

            if contest.is_none() {
                return HttpResponse::Ok().body("No data available for this contest.");
            }

            contest.as_mut().unwrap().candidates.sort_by(|a, b| {
                b.percentage
                    .partial_cmp(&a.percentage)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let markup = templates::contest_details_page(contest.unwrap());
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
    let redis_url = env::var("REDIS_URL").unwrap_or("127.0.0.1".to_string());
    info!("Redis URL: {}", redis_url);
    let redis_client = RedisClient::open(format!("{}", redis_url))
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
            .route("/{contest_id}", web::get().to(contest_page))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
