use clap::ArgMatches;
use journal::{BilogItem, Journal, JournalCall};
use std::fs::File;
use std::io::Error;

pub fn viewer_main(matches: ArgMatches) {
    let viewer_matches = matches.subcommand_matches("logview").unwrap();
    let journal_path = viewer_matches
        .value_of("journal-path")
        .expect("Journal path not specified");

    let f = File::open(journal_path).expect("Failed to open journal file");
    let mut j = Journal::open(f, false).expect("Failed to recover journal");

    let iter = match viewer_matches.value_of("direction").unwrap() {
        "reverse" => Box::new(j.read_reverse()) as Box<Iterator<Item = Result<JournalCall, Error>>>,
        "forward" => Box::new(j.read_forward()) as Box<Iterator<Item = Result<JournalCall, Error>>>,
        _ => unreachable!(),
    };

    let verbose = viewer_matches.is_present("verbose");

    for entry in iter {
        println!(
            "{}",
            entry
                .expect("Failed to read journal entry")
                .describe_bilog(verbose)
        )
    }
}
