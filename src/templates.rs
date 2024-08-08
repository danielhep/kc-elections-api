use std::env;

use crate::{BallotInfo, ElectionData};
use maud::{html, Markup, DOCTYPE};

pub fn index(ballot_info: &[BallotInfo]) -> Markup {
    let goatcounter_url = env::var("GOATCOUNTER_URL");
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { "King County Election Data Dashboard" }
                script src="https://cdnjs.cloudflare.com/ajax/libs/htmx/1.9.10/htmx.min.js" {}
                script src="https://cdn.tailwindcss.com" {}
                @if goatcounter_url.is_ok() {
                    script data-goatcounter=(goatcounter_url.unwrap()) src="//gc.zgo.at/count.js" async {}
                }
            }
            body class="bg-gray-100" {
                div class="container mx-auto p-4" {
                    h1 class="text-3xl font-bold mb-4" { "King County Election Data Dashboard" }

                    div class="mb-8" {
                        h2 class="text-2xl font-semibold mb-2" { "Summary Statistics" }
                        div id="summary-stats" hx-get="/election-data/summary-html" hx-trigger="load" class="bg-white p-4 rounded shadow" {
                            "Loading summary statistics..."
                        }
                    }

                    div class="mb-8" {
                        h2 class="text-2xl font-semibold mb-2" { "Ballot Titles" }
                        select id="contest-select" name="contest" class="mb-4 p-2 border rounded" hx-get="/election-data/contest-html" hx-target="#contest-details" hx-trigger="change" {
                            option value="" { "Select a contest" }
                            @for info in ballot_info {
                                option value=(info.contest_id) { (info.ballot_title) " - " (info.district_name) }
                            }
                        }
                    }

                    div {
                        h2 class="text-2xl font-semibold mb-2" { "Contest Details" }
                        div id="contest-details" class="bg-white p-4 rounded shadow" {
                            "Select a contest to view details."
                        }
                    }
                }
                footer class="container mx-auto my-4" {
                    p {
                        "Not affiliated with King County government. Made by Daniel Heppner. "
                        a class="underline" href="https://github.com/danielhep/kc-elections-api" {"Source available on GitHub"}
                    }
                }
            }
        }
    }
}

pub fn summary_statistics(
    total_votes: i32,
    total_registered_voters: i32,
    average_turnout: f64,
) -> Markup {
    html! {
        p { strong { "Total Votes: " } (total_votes) }
        p { strong { "Total Registered Voters: " } (total_registered_voters) }
        p { strong { "Average Turnout: " } (format!("{:.2}%", average_turnout)) }
    }
}

pub fn contest_details(contest_data: &[ElectionData]) -> Markup {
    html! {
        h3 class="text-xl font-semibold mb-2" { (contest_data[0].ballot_title) }
        p { strong { "District: " } (contest_data[0].district_name) }
        p { strong { "Ballots Counted: " } (contest_data[0].ballots_counted_for_district) }
        p { strong { "Registered Voters: " } (contest_data[0].registered_voters_for_district) }
        p { strong { "Turnout: " } (format!("{:.2}%", contest_data[0].percent_turnout_for_district.0)) }
        h4 class="text-lg font-semibold mt-4 mb-2" { "Results:" }
        ul {
            @for item in contest_data {
                li {
                    (item.ballot_response) " ("
                    @if let Some(party) = &item.party_preference {
                        (party)
                    } @else {
                        "No Party"
                    }
                    "): " (item.votes) " votes (" (format!("{:.2}%", item.percent_of_votes.0)) ")"
                }
            }
        }
    }
}
