use std::{collections::HashMap, env};

use crate::{BallotInfo, ElectionData};
use maud::{html, Markup, DOCTYPE};

pub fn index(ballot_info: &HashMap<String, Vec<BallotInfo>>) -> Markup {
    let goatcounter_url = env::var("GOATCOUNTER_URL");
    let mut keys_sorted: Vec<String> = ballot_info.keys().cloned().collect();
    keys_sorted.sort_unstable();
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
                        h2 class="text-2xl font-semibold mb-2" { "Contests by Ballot Title" }
                        div class="grid grid-cols-2 gap-4" {
                            @for title in keys_sorted {
                                div {
                                    h3 class="text-lg font-bold mb-2" { (title) }
                                    ul class="grid grid-cols-[repeat(auto-fill,minmax(120px,max-content))] auto-rows-auto gap-x-4 gap-y-2" {
                                        @for contest in ballot_info.get(&title).unwrap() {
                                            li { (contest.district_name ) }
                                        }
                                    }
                                }
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
                        "You have found a Daniel Heppner side project. Don't get it twisted, this isn't official! "
                        a class="underline" href="https://github.com/danielhep/kc-elections-api" {"See the messy source code on GitHub."}
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
