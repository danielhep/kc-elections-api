use std::str::FromStr;

use crate::{Candidate, Contest, District, PartyPreference};
use chrono::{DateTime, Utc};
use tokio::sync::Mutex;
use tokio_postgres::{Client, NoTls};

pub struct DbClient {
    client: Mutex<Client>,
}

impl DbClient {
    pub async fn new(connection_string: &str) -> Result<Self, tokio_postgres::Error> {
        let (client, connection) = tokio_postgres::connect(connection_string, NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("Connection error: {}", e);
            }
        });

        Ok(DbClient {
            client: Mutex::new(client),
        })
    }

    pub async fn create_tables(&self) -> Result<(), tokio_postgres::Error> {
        let client = self.client.lock().await;
        client
            .batch_execute(
                "
                CREATE TABLE IF NOT EXISTS updates (
                    id SERIAL PRIMARY KEY,
                    timestamp TIMESTAMP NOT NULL,
                    total_votes BIGINT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS districts (
                    id SERIAL PRIMARY KEY,
                    name TEXT NOT NULL,
                    percent_turnout FLOAT NOT NULL,
                    registered_voters INTEGER NOT NULL,
                    ballots_counted INTEGER NOT NULL,
                    district_type TEXT NOT NULL,
                    district_type_subheading TEXT NOT NULL,
                    update_id INTEGER NOT NULL REFERENCES updates(id)
                );

                CREATE TABLE IF NOT EXISTS contests (
                    id INTEGER PRIMARY KEY,
                    ballot_title TEXT NOT NULL,
                    district_id INTEGER NOT NULL REFERENCES districts(id),
                    update_id INTEGER NOT NULL REFERENCES updates(id)
                );

                CREATE TABLE IF NOT EXISTS candidates (
                    id SERIAL PRIMARY KEY,
                    name TEXT NOT NULL,
                    percentage FLOAT NOT NULL,
                    votes INTEGER NOT NULL,
                    party_preference TEXT NOT NULL,
                    contest_id INTEGER NOT NULL REFERENCES contests(id),
                    update_id INTEGER NOT NULL REFERENCES updates(id)
                );
                ",
            )
            .await
    }

    pub async fn log_update(
        &self,
        contests: &[Contest],
        total_votes: i64,
    ) -> Result<(), tokio_postgres::Error> {
        let mut client = self.client.lock().await;
        let transaction = client.transaction().await?;

        // Insert update
        let update_row = transaction
            .query_one(
                "INSERT INTO updates (timestamp, total_votes) VALUES (NOW(), $1) RETURNING id",
                &[&total_votes],
            )
            .await?;
        let update_id: i32 = update_row.get(0);

        // Insert districts, contests, and candidates
        for contest in contests {
            let district_row = transaction.query_one(
                "INSERT INTO districts (name, percent_turnout, registered_voters, ballots_counted, district_type, district_type_subheading, update_id) 
                VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
                &[&contest.district.name, &contest.district.percent_turnout, &contest.district.registered_voters,
                  &contest.district.ballots_counted, &contest.district.district_type, &contest.district.district_type_subheading, &update_id],
            ).await?;
            let district_id: i32 = district_row.get(0);

            transaction.execute(
                "INSERT INTO contests (id, ballot_title, district_id, update_id) VALUES ($1, $2, $3, $4)",
                &[&(contest.id as i32), &contest.ballot_title, &district_id, &update_id],
            ).await?;

            for candidate in &contest.candidates {
                transaction.execute(
                    "INSERT INTO candidates (name, percentage, votes, party_preference, contest_id, update_id) 
                    VALUES ($1, $2, $3, $4, $5, $6)",
                    &[&candidate.name, &candidate.percentage, &candidate.votes,
                      &format!("{:?}", candidate.party_preference), &(contest.id as i32), &update_id],
                ).await?;
            }
        }

        transaction.commit().await?;
        Ok(())
    }

    pub async fn get_latest_total_votes(&self) -> Result<Option<i64>, tokio_postgres::Error> {
        let client = self.client.lock().await;
        let row = client
            .query_opt(
                "SELECT total_votes FROM updates ORDER BY timestamp DESC LIMIT 1",
                &[],
            )
            .await?;

        Ok(row.map(|r| r.get(0)))
    }

    pub async fn get_latest_data(&self) -> Result<Vec<Contest>, tokio_postgres::Error> {
        let client = self.client.lock().await;
        let latest_update = client
            .query_one(
                "SELECT id FROM updates ORDER BY timestamp DESC LIMIT 1",
                &[],
            )
            .await?;
        let update_id: i32 = latest_update.get(0);

        let contests = client
            .query(
                "SELECT c.id, c.ballot_title, 
                        d.name, d.percent_turnout, d.registered_voters, d.ballots_counted, d.district_type, d.district_type_subheading
                 FROM contests c
                 JOIN districts d ON c.district_id = d.id
                 WHERE c.update_id = $1",
                &[&update_id],
            )
            .await?;

        let mut result = Vec::new();

        for contest_row in contests {
            let contest_id: i32 = contest_row.get(0);
            let candidates = client
                .query(
                    "SELECT name, percentage, votes, party_preference 
                     FROM candidates 
                     WHERE contest_id = $1 AND update_id = $2",
                    &[&contest_id, &update_id],
                )
                .await?;

            let contest = Contest {
                id: contest_id as u32,
                ballot_title: contest_row.get(1),
                district: District {
                    name: contest_row.get(2),
                    percent_turnout: contest_row.get(3),
                    registered_voters: contest_row.get(4),
                    ballots_counted: contest_row.get(5),
                    district_type: contest_row.get(6),
                    district_type_subheading: contest_row.get(7),
                },
                candidates: candidates
                    .into_iter()
                    .map(|c| Candidate {
                        name: c.get(0),
                        percentage: c.get(1),
                        votes: c.get(2),
                        party_preference: PartyPreference::from_str(&c.get::<_, String>(3))
                            .unwrap_or(PartyPreference::NotAffiliated),
                    })
                    .collect(),
            };

            result.push(contest);
        }

        Ok(result)
    }

    // pub async fn get_update_timestamps(&self) -> Result<Vec<NaiveDateTime>, tokio_postgres::Error> {
    //     let rows = self.client
    //         .query("SELECT timestamp FROM updates ORDER BY timestamp DESC", &[])
    //         .await?;

    //     Ok(rows.iter().map(|row| row.get(0)).collect())
    // }

    pub async fn get_data_at_timestamp(
        &self,
        timestamp: DateTime<Utc>,
    ) -> Result<Vec<Contest>, tokio_postgres::Error> {
        let client = self.client.lock().await;
        let update_row = client
            .query_one("SELECT id FROM updates WHERE timestamp = $1", &[&timestamp])
            .await?;
        let update_id: i32 = update_row.get(0);

        // Use the same query logic as get_latest_data, but with the specific update_id
        // This code is similar to get_latest_data, consider refactoring to avoid duplication
        let contests = client
            .query(
                "SELECT c.id, c.ballot_title, 
                        d.name, d.percent_turnout, d.registered_voters, d.ballots_counted, d.district_type, d.district_type_subheading
                 FROM contests c
                 JOIN districts d ON c.district_id = d.id
                 WHERE c.update_id = $1",
                &[&update_id],
            )
            .await?;

        let mut result = Vec::new();

        for contest_row in contests {
            let contest_id: i32 = contest_row.get(0);
            let candidates = client
                .query(
                    "SELECT name, percentage, votes, party_preference 
                     FROM candidates 
                     WHERE contest_id = $1 AND update_id = $2",
                    &[&contest_id, &update_id],
                )
                .await?;

            let contest = Contest {
                id: contest_id as u32,
                ballot_title: contest_row.get(1),
                district: District {
                    name: contest_row.get(2),
                    percent_turnout: contest_row.get(3),
                    registered_voters: contest_row.get(4),
                    ballots_counted: contest_row.get(5),
                    district_type: contest_row.get(6),
                    district_type_subheading: contest_row.get(7),
                },
                candidates: candidates
                    .into_iter()
                    .map(|c| Candidate {
                        name: c.get(0),
                        percentage: c.get(1),
                        votes: c.get(2),
                        party_preference: PartyPreference::from_str(&c.get::<_, String>(3))
                            .unwrap_or(PartyPreference::NotAffiliated),
                    })
                    .collect(),
            };

            result.push(contest);
        }

        Ok(result)
    }
}
