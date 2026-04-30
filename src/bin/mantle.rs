fn main() {
    if let Err(err) = strata::cli::run_mantle_from_env() {
        eprintln!("mantle: error: {err}");
        std::process::exit(1);
    }
}
