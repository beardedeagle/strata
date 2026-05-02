#![forbid(unsafe_code)]

fn main() {
    if let Err(err) = mantle_runtime::run_mantle_from_env() {
        eprintln!("mantle: error: {err}");
        std::process::exit(1);
    }
}
