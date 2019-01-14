extern crate regex;

use self::regex::Regex;
use clap::ArgMatches;
use journal::{BilogEntry, Journal, JournalEntry};
use std::fs::File;
use std::io::Error;

pub fn viewer_main(matches: ArgMatches) {
    let viewer_matches = matches.subcommand_matches("journal").unwrap();
    let journal_path = viewer_matches
        .value_of("journal-path")
        .expect("Journal path not specified");

    let f = File::open(journal_path).expect("Failed to open journal file");
    let mut j = Journal::open(f, false).expect("Failed to recover journal");

    let iter = match viewer_matches.value_of("direction").unwrap() {
        "reverse" => Box::new(j.read_reverse()) as Box<Iterator<Item = Result<BilogEntry, Error>>>,
        "forward" => Box::new(j.read_forward()) as Box<Iterator<Item = Result<BilogEntry, Error>>>,
        _ => unreachable!(),
    };

    let verbose = viewer_matches.is_present("verbose");

    let filter = viewer_matches
        .value_of("filter")
        .map(|f| Regex::new(f).expect("Filter is not a valid regex"));

    let inverse = viewer_matches.is_present("inverse");

    let filtered = iter
        .map(|e| e.expect("Failed to read journal entry"))
        .filter(|e| {
            if filter.is_none() {
                return true;
            }
            let has_match = e
                .affected_paths()
                .iter()
                .filter(|p| filter.as_ref().unwrap().is_match(p.to_str().unwrap()))
                .next()
                .is_some();

            (!has_match && inverse) || (has_match && !inverse)
        });

    match viewer_matches.subcommand_name() {
        Some("view") => {
            for entry in filtered {
                println!("{}", entry.describe(verbose))
            }
        }
        Some("replay") => {}
        None => panic!("Subcommand is required"),
        _ => unreachable!(),
    }
}
