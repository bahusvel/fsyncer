extern crate chrono;
extern crate regex;

use self::chrono::{DateTime, Local};
use self::regex::Regex;
use clap::ArgMatches;
use client::dispatch;
use journal::{BilogEntry, EntryContent, Journal, JournalConfig, JournalEntry, StoreEntry};
use std::fs::File;
use std::io::Error;
use std::path::Path;

pub fn viewer_main(matches: ArgMatches) {
    let journal_matches = matches.subcommand_matches("journal").unwrap();
    let journal_path = journal_matches
        .value_of("journal-path")
        .expect("Journal path not specified");

    let f = File::open(journal_path).expect("Failed to open journal file");
    let c = JournalConfig {
        sync: false,
        journal_size: 0,
        filestore_size: 0,
        vfsroot: Path::new(
            journal_matches
                .subcommand_matches("replay")
                .and_then(|r| r.value_of("backing-store"))
                .unwrap_or(""),
        )
        .to_path_buf(),
    };
    let mut j = Journal::open(f, c).expect("Failed to recover journal");

    let iter = if journal_matches.is_present("reverse") {
        Box::new(j.read_reverse()) as Box<Iterator<Item = Result<StoreEntry<BilogEntry>, Error>>>
    } else {
        Box::new(j.read_forward()) as Box<Iterator<Item = Result<StoreEntry<BilogEntry>, Error>>>
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
            let has_match = match e.contents() {
                EntryContent::Payload(e) => e
                    .affected_paths()
                    .iter()
                    .filter(|p| filter.as_ref().unwrap().is_match(p.to_str().unwrap()))
                    .next()
                    .is_some(),
                EntryContent::Time(_) => return true,
            };
            (!has_match && inverse) || (has_match && !inverse)
        });

    match journal_matches.subcommand_name() {
        Some("view") => {
            let view_matches = journal_matches.subcommand_matches("view").unwrap();
            let verbose = view_matches.is_present("verbose");
            for entry in filtered {
                match entry.contents() {
                    EntryContent::Payload(e) => {
                        println!("{}\t{}", entry.trans_id(), e.describe(verbose))
                    }
                    EntryContent::Time(t) => println!("{}", DateTime::<Local>::from(*t)),
                }
            }
        }
        Some("replay") => {
            let replay_matches = journal_matches.subcommand_matches("replay").unwrap();
            let path = Path::new(
                replay_matches
                    .value_of("backing-store")
                    .expect("backing store is required for replay"),
            );

            for entry in filtered {
                match entry.contents() {
                    EntryContent::Payload(e) => {
                        let vfscall = e.apply(&path).expect("Failed to generate bilog vfscall");
                        e.describe(false);
                        //debug!(vfscall);
                        let res = unsafe { dispatch(&vfscall, path) };
                        if res < 0 {
                            panic!(
                                "Failed to apply bilog entry {:?} error {}({})",
                                e,
                                Error::from_raw_os_error(-res),
                                res,
                            );
                        }
                    }
                    EntryContent::Time(t) => {
                        println!("Replaying events from {}", DateTime::<Local>::from(*t))
                    }
                }
            }
        }
        None => panic!("Subcommand is required"),
        _ => unreachable!(),
    }
}
