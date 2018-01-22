extern crate cc;
use std::process::Command;
use std::str::from_utf8;

fn main() {
    let iflags = Command::new("pkg-config")
        .arg("fuse3")
        .arg("--cflags")
        .output()
        .expect("failed to execute process")
        .stdout;

    cc::Build::new()
        .flag(&from_utf8(&iflags[..iflags.len() - 1]).expect(
            "Non utf output",
        ))
        .include("../include")
        .define("_FILE_OFFSET_BITS", "64")
        .warnings(false)
        .flag("-Wall")
        .file("main.c")
        .file("read.c")
        .file("write.c")
        .file("decode.c")
        .file("fsops.c")
        .compile("fsyncer");
}
