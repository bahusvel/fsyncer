extern crate chrono;

use self::chrono::{DateTime, Local};
use clap::ArgMatches;
use client::dispatch;
use common::{canonize_path, VFSCall};
use error::Error;
use journal::{
    BilogEntry, EntryContent, Journal, JournalConfig, JournalEntry,
    JournalType, StoreEntry,
};
use regex::Regex;
use std::fmt::Debug;
use std::fs::File;
use std::io;
use std::path::Path;

fn filter_journal<
    'a,
    T: JournalEntry<'a>,
    I: Iterator<Item = Result<StoreEntry<T>, Error<io::Error>>>,
>(
    iter: I,
    filter: Option<Regex>,
    inverse: bool,
) -> impl Iterator<Item = StoreEntry<T>> {
    iter.map(|e| e.expect("Failed to read journal entry"))
        .filter(move |e| {
            // If there is no filter, and we are flattening filter out all time
            // entries, otherwise return true (permitting all elements).
            if filter.is_none() {
                return true;
            }
            let has_match = match e.contents() {
                EntryContent::Payload(e) => e
                    .affected_paths()
                    .iter()
                    .any(|p| {
                        filter.as_ref().unwrap().is_match(p.to_str().unwrap())
                    }),
                EntryContent::Time(_) => return true,
            };
            (!has_match && inverse) || (has_match && !inverse)
        })
}

fn view<T>(j: &mut Journal, journal_matches: &ArgMatches)
where
    T: for<'de> JournalEntry<'de> + Debug,
{
    let iter: Box<
        dyn Iterator<Item = Result<StoreEntry<T>, Error<io::Error>>>,
    > = if journal_matches.is_present("reverse") {
        Box::new(j.read_reverse())
    } else {
        Box::new(j.read_forward())
    };

    let view_matches = journal_matches.subcommand_matches("view").unwrap();
    let verbose = view_matches.is_present("verbose");
    let filter = view_matches
        .value_of("filter")
        .map(|f| Regex::new(f).expect("Filter is not a valid regex"));
    for entry in
        filter_journal(iter, filter, view_matches.is_present("invert-filter"))
    {
        match entry.contents() {
            EntryContent::Payload(e) => {
                println!("{}\t{}", entry.trans_id(), e.describe(verbose))
            }
            EntryContent::Time(t) => {
                println!("{}", DateTime::<Local>::from(*t))
            }
        }
    }
}

fn replay<T>(j: &mut Journal, journal_matches: &ArgMatches)
where
    T: for<'de> JournalEntry<'de> + Debug,
{
    let iter: Box<
        dyn Iterator<Item = Result<StoreEntry<T>, Error<io::Error>>>,
    > = if journal_matches.is_present("reverse") {
        Box::new(j.read_reverse())
    } else {
        Box::new(j.read_forward())
    };

    let replay_matches = journal_matches.subcommand_matches("replay").unwrap();
    let path = canonize_path(Path::new(
        replay_matches
            .value_of("backing-store")
            .expect("backing store is required for replay"),
    ))
    .expect("Failed to get absolute path");
    let filter = replay_matches
        .value_of("filter")
        .map(|f| Regex::new(f).expect("Filter is not a valid regex"));

    for entry in
        filter_journal(iter, filter, replay_matches.is_present("invert-filter"))
    {
        match entry.contents() {
            EntryContent::Payload(e) => {
                let vfscall =
                    e.apply(&path).expect("Failed to generate bilog vfscall");
                e.describe(false);
                //debug!(vfscall);
                let res = unsafe { dispatch(&vfscall, &path) };
                if res < 0 {
                    panic!(
                        "Failed to apply entry {:?} error {}({})",
                        e,
                        io::Error::from_raw_os_error(-res),
                        res,
                    );
                }
            }
            EntryContent::Time(t) => println!(
                "Replaying events from {}",
                DateTime::<Local>::from(*t)
            ),
        }
    }
}

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
        journal_type: JournalType::Invalid,
        vfsroot: Path::new(
            journal_matches
                .subcommand_matches("replay")
                .and_then(|r| r.value_of("backing-store"))
                .unwrap_or(""),
        )
        .to_path_buf(),
    };
    let mut j = Journal::open(f, c).expect("Failed to recover journal");

    match journal_matches.subcommand_name() {
        Some("view") => match j.journal_type() {
            JournalType::Forward => {
                if journal_matches.is_present("reverse") {
                    eprintln!(
                        "You are viewing a forward-only journal in reverse, \
                         it cannot be replayed in this direction!"
                    )
                }
                view::<VFSCall>(&mut j, journal_matches);
            }
            JournalType::Undo => {
                if !journal_matches.is_present("reverse") {
                    eprintln!(
                        "You are viewing a undo-only journal forward, it \
                         cannot be replayed in this direction!"
                    )
                }
            }
            JournalType::Bilog => view::<BilogEntry>(&mut j, journal_matches),
            JournalType::Invalid => panic!("Invalid journal type"),
        },
        Some("replay") => match j.journal_type() {
            JournalType::Forward => {
                if journal_matches.is_present("reverse") {
                    panic!(
                        "Forward-only journal cannot be replayed in reverse!"
                    )
                }
                replay::<VFSCall>(&mut j, journal_matches);
            }
            JournalType::Undo => {
                if !journal_matches.is_present("reverse") {
                    panic!("Undo-only journal cannot be replayed forward!")
                }
            }
            JournalType::Bilog => replay::<BilogEntry>(&mut j, journal_matches),
            JournalType::Invalid => panic!("Invalid journal type"),
        },
        _ => unreachable!(),
    }
}
