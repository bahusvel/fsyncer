extern crate cc;
//extern crate git_build_version;
extern crate git_version;
use std::process::Command;
use std::str::from_utf8;

//const PACKAGE_TOP_DIR: &'static str = ".";

fn main() {
    //git_build_version::write_version(PACKAGE_TOP_DIR).expect("Saving git version");

    git_version::set_env();

    if cfg!(target_os="windows") {
        let iflags = "C:\\Program Files\\Dokan\\Dokan Library-1.2.1\\include";
         cc::Build::new()
        .include(iflags)
        .warnings(false)
        .flag("-Wall")
        .file("windows.c")
        .compile("fsyncer");
    } else {
        let fuse_flags_out = Command::new("pkg-config")
        .arg("fuse3")
        .arg("--cflags")
        .output()
        .expect("failed to execute process");

        if !fuse_flags_out.status.success() {
            panic!("Could not find fuse3 using 'pkg-config fuse3 --cflags'");
        }

        let out = fuse_flags_out.stdout;

        let iflags = from_utf8(&out[..out.len() - 1])
                .expect("Non utf output")
                .trim();

         cc::Build::new()
        .include(iflags)
        .define("_FILE_OFFSET_BITS", "64")
        .warnings(false)
        .flag("-Wall")
        .file("read.c")
        .compile("fsyncer");
    }
}
