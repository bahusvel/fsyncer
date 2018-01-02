extern crate cc;

fn main() {
    cc::Build::new()
        .include("../include")
        .define("_FILE_OFFSET_BITS", "64")
        .warnings(false)
        .flag("-Wall")
        .file("decode.c")
        .file("main.c")
        .compile("fsyncer_client");
}
