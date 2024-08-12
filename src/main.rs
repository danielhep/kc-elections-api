use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use env_logger;
use log::{error, info};
use maud::{html, Markup, Render};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::env;
use std::{io::Cursor, str::FromStr, sync::Arc};
use tokio::time::{interval, Duration};

mod database;
mod templates;

use database::DbClient;

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
    db: Arc<DbClient>,
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

async fn fetch_and_parse_csv() -> Result<(Vec<Contest>, i64), Box<dyn std::error::Error>> {
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

    let contests = process_election_data(parsed_data);
    let total_votes: i64 = contests
        .iter()
        .flat_map(|c| &c.candidates)
        .map(|c| c.votes as i64)
        .sum();

    Ok((contests, total_votes))
}

async fn update_data(db_client: &DbClient) -> Result<(), Box<dyn std::error::Error>> {
    let (parsed_data, total_votes) = fetch_and_parse_csv().await?;

    let latest_total_votes = db_client.get_latest_total_votes().await?;

    if latest_total_votes.map_or(true, |votes| votes != total_votes) {
        // Log the update to PostgreSQL
        db_client.log_update(&parsed_data, total_votes).await?;
        info!("Data updated. New total votes: {}", total_votes);
    } else {
        info!("No change in data. Current total votes: {}", total_votes);
    }

    Ok(())
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

async fn get_all_data(db_client: &DbClient) -> Result<Vec<Contest>, actix_web::Error> {
    db_client.get_latest_data().await.map_err(|e| {
        error!("Database error: {}", e);
        actix_web::error::ErrorInternalServerError("Database error")
    })
}

async fn index(data: web::Data<AppState>) -> impl Responder {
    match get_all_data(&data.db).await.map(contests_by_ballot_title) {
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
    match get_all_data(&data.db).await {
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
    let postgres_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    info!("PostgreSQL URL: {}", postgres_url);
    let db_client = DbClient::new(&postgres_url)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Wrap the DbClient in an Arc immediately
    let db_client = Arc::new(db_client);

    db_client
        .clone()
        .create_tables()
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let app_state = web::Data::new(AppState {
        db: db_client.clone(),
    });

    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(3600)); // Check every hour
        loop {
            interval.tick().await;
            if let Err(e) = update_data(&db_client).await {
                error!("Failed to update data: {}", e);
            }
        }
    });

    HttpServer::new(move || {
        // Create a CORS middleware
        let cors = Cors::permissive();

        App::new()
            .wrap(cors) // Add this line to wrap the entire app with CORS middleware
            .app_data(app_state.clone())
            .route("/", web::get().to(index))
            .route("/{contest_id}", web::get().to(contest_page))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
