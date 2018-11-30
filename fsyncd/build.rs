extern crate cc;
extern crate git_build_version;
use std::process::Command;
use std::str::from_utf8;

const PACKAGE_TOP_DIR: &'static str = ".";

fn main() {
    git_build_version::write_version(PACKAGE_TOP_DIR).expect("Saving git version");
    let fuse_flags_out = Command::new("pkg-config")
        .arg("fuse3")
        .arg("--cflags")
        .output()
        .expect("failed to execute process");

    if !fuse_flags_out.status.success() {
        panic!("Could not find fuse3 using 'pkg-config fuse3 --cflags'");
    }

    let iflags = fuse_flags_out.stdout;

    cc::Build::new()
        .flag(
            &from_utf8(&iflags[..iflags.len() - 1])
                .expect("Non utf output")
                .trim(),
        )
        .include("../include")
        .define("_FILE_OFFSET_BITS", "64")
        .warnings(false)
        .flag("-Wall")
        .file("read.c")
        .compile("fsyncer");
}
