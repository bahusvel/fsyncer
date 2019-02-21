extern crate cc;
use std::process::Command;

fn main() {
    let iflags = "C:\\Program Files\\Dokan\\Dokan Library-1.2.1\\include";
        cc::Build::new()
    .include(iflags)
    .warnings(false)
    .flag("-Wall")
    .file("helper.c")
    .compile("helper");
}
