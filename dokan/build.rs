#[cfg(target_os = "windows")]
extern crate cc;
#[cfg(target_os = "windows")]
const DOKAN_PATH: &str = "C:\\Program Files\\Dokan\\Dokan Library-1.2.1\\";

fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut lib = "cargo:rustc-link-search=".to_string();
        lib.push_str(DOKAN_PATH);
        lib.push_str("lib");
        println!("{}", lib);

        let mut include = DOKAN_PATH.to_string();
        include.push_str("include");

        cc::Build::new()
            .define("_UNICODE", None)
            .define("UNICODE", None)
            .include(include)
            .warnings(false)
            .flag("-Wall")
            .file("helper.c")
            .compile("helper");
    }
}
