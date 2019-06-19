mod forward;

pub use self::forward::Snapshot;
use journal::{EntryContent, Journal, JournalConfig, JournalType};
use std::fs::File;
use std::path::{Path, PathBuf};

use clap::ArgMatches;

pub fn snapshot_main(matches: ArgMatches) {
    let snapshot_matches = matches.subcommand_matches("snapshot").unwrap();
    let snapshot_path = snapshot_matches.value_of("snapshot-path").unwrap();
    let mut snapshot = if Path::new(snapshot_path).exists() {
        let file =
            File::create(snapshot_path).expect("Failed to open snapshot file");
        Snapshot::open(file).expect("Failed to parse snapshot")
    } else {
        let file =
            File::create(snapshot_path).expect("Failed to open snapshot file");
        Snapshot::new(file)
    };
    match snapshot_matches.subcommand_name() {
        Some("merge") => {
            let merge_matches =
                snapshot_matches.subcommand_matches("merge").unwrap();
            let journal_path = merge_matches.value_of("journal-path").unwrap();
            let f =
                File::open(journal_path).expect("Failed to open journal file");
            let c = JournalConfig {
                sync: false,
                journal_size: 0,
                filestore_size: 0,
                journal_type: JournalType::Invalid,
                vfsroot: PathBuf::new(),
            };
            let mut j = Journal::open(f, c).expect("Failed to recover journal");
            assert!(j.journal_type() == JournalType::Forward);
            let iter = j.read_forward().filter_map(|e| {
                if let EntryContent::Payload(p) =
                    e.expect("Failed to read journal").take_content()
                {
                    Some(p)
                } else {
                    None
                }
            });
            snapshot
                .merge_from(iter)
                .expect("Failed to process journal entries");
            snapshot.finalize().expect("Failed to finalize snapshot");
        }
        Some("apply") => {
            use client::dispatch;
            use std::io::Error;
            let apply_matches =
                snapshot_matches.subcommand_matches("apply").unwrap();
            let fs_path = apply_matches.value_of("backing-path").unwrap();
            for call in snapshot.apply() {
                let res = unsafe { dispatch(&call, Path::new(fs_path)) };
                if res < 0 {
                    panic!("Failed to apply snapshot {:?} error {}({})", call, Error::from_raw_os_error(-res), res)
                }
            }
        }
        _ => unreachable!(),
    }
}
