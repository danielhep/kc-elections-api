use std::{collections::HashMap, env};

use crate::Contest;
use maud::{html, Markup, DOCTYPE};

pub fn header() -> Markup {
    let goatcounter_url = env::var("GOATCOUNTER_URL");
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { "King County Election Data Dashboard" }
                // script src="https://cdnjs.cloudflare.com/ajax/libs/htmx/1.9.10/htmx.min.js" {}
                script src="https://cdn.tailwindcss.com" {}
                @if goatcounter_url.is_ok() {
                    script data-goatcounter=(goatcounter_url.unwrap()) src="//gc.zgo.at/count.js" async {}
                }
            }
        }
    }
}

pub fn footer() -> Markup {
    html!(
        footer class="container mx-auto my-4" {
            p {
                "You have found a Daniel Heppner side project. Don't get it twisted, this isn't official! "
                a class="underline" href="https://github.com/danielhep/kc-elections-api" {"See the messy source code on GitHub."}
            }
            p class="text-xs" {
                "it's written in rust btw"
            }
        }
    )
}

pub fn layout(children: Markup) -> Markup {
    html!(
        (header())
        body class="bg-gray-100" {
            div class="container mx-auto p-4" {
            h1 class="text-3xl font-bold mb-4" { a href="/" {"King County Election Data Dashboard"} }
                div class="mb-8" {
                    (children)
                }
            }
            (footer())
        }
    )
}

pub fn index(ballot_info: &HashMap<String, Vec<Contest>>) -> Markup {
    let mut keys_sorted: Vec<String> = ballot_info.keys().cloned().collect();
    keys_sorted.sort_unstable();
    html! {
        (layout(html!(
                h2 class="text-2xl font-semibold mb-2" { "Contests by Ballot Title" }
                div class="grid md:grid-cols-2 gap-4" {
                @for title in keys_sorted {
                    div class="bg-white rounded shadow p-2" {
                        h3 class="text-lg font-bold mb-2" { (title) }
                        ul class="grid grid-cols-[repeat(auto-fill,minmax(120px,max-content))] auto-rows-auto gap-x-4 gap-y-2" {
                            @for contest in ballot_info.get(&title).unwrap() {
                                li class="underline hover:text-slate-900" { a href=(format!("/{}", contest.id)) {(contest.district.name ) } }
                            }
                        }
                    }
                }
            }
        )))
    }
}

pub fn contest_details_page(contest: Contest) -> Markup {
    html! {
        (layout(html! (
            h2 class="text-2xl font-semibold mb-2" { (contest.ballot_title) }
            @if contest.district.name.contains("State of Washington") {
                p { strong {"Note: This is King County Election results only, for statewide races the results don't represent the total count."} }
            }
            p { strong { "District: " } (contest.district.name) }
            // p { strong { "Ballots Counted: " } (contest.) }
            // p { strong { "Registered Voters: " } (contest.registered_voters_for_district) }
            // p { strong { "Turnout: " } (format!("{:.2}%", contest.percent_turnout_for_district.0)) }
            div class="bg-white rounded shadow p-2 mt-2" {
                h4 class="text-lg font-semibold mb-2" { "Results:" }
                ul class="inline-grid grid-cols-2 gap-x-1 gap-y-2" {
                    @for candidate in contest.candidates {
                        li class="contents" {
                            div {(candidate.name) " ("
                            (candidate.party_preference)
                            "):"}
                            div { (candidate.votes) " votes (" (format!("{:.2}%", candidate.percentage)) ")"}
                        }
                    }
                }
            }
        )))
    }
}
