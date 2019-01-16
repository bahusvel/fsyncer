extern crate regex;

use self::regex::Regex;
use clap::ArgMatches;
use client::dispatch;
use journal::{BilogEntry, Journal, JournalEntry};
use std::fs::File;
use std::io::Error;

pub fn viewer_main(matches: ArgMatches) {
    let journal_matches = matches.subcommand_matches("journal").unwrap();
    let journal_path = journal_matches
        .value_of("journal-path")
        .expect("Journal path not specified");

    let f = File::open(journal_path).expect("Failed to open journal file");
    let mut j = Journal::open(f, false).expect("Failed to recover journal");

    let iter = if journal_matches.is_present("reverse") {
        Box::new(j.read_reverse()) as Box<Iterator<Item = Result<BilogEntry, Error>>>
    } else {
        Box::new(j.read_forward()) as Box<Iterator<Item = Result<BilogEntry, Error>>>
    };

    let filter = journal_matches
        .value_of("filter")
        .map(|f| Regex::new(f).expect("Filter is not a valid regex"));

    let inverse = journal_matches.is_present("inverse-filter");

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

    match journal_matches.subcommand_name() {
        Some("view") => {
            let view_matches = journal_matches.subcommand_matches("view").unwrap();
            let verbose = view_matches.is_present("verbose");
            for entry in filtered {
                println!("{}", entry.describe(verbose))
            }
        }
        Some("replay") => {
            let replay_matches = journal_matches.subcommand_matches("replay").unwrap();
            let path = replay_matches
                .value_of("backing-store")
                .expect("backing store is required for replay");
            for entry in filtered {
                let vfscall = entry
                    .apply(&path)
                    .expect("Failed to generate bilog vfscall");
                entry.describe(false);
                //debug!(vfscall);
                let res = unsafe { dispatch(&vfscall, path) };
                if res < 0 {
                    panic!(
                        "Failed to apply bilog entry {:?} error {}({})",
                        entry,
                        Error::from_raw_os_error(-res),
                        res,
                    );
                }
            }
        }
        None => panic!("Subcommand is required"),
        _ => unreachable!(),
    }
}
