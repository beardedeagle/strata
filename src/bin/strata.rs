fn main() {
    if let Err(err) = strata::cli::run_strata_from_env() {
        eprintln!("strata: error: {err}");
        std::process::exit(1);
    }
}
