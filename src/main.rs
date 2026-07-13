fn main() {
    let code = match quinte::cli::entrypoint() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("quinte: {error}");
            error.exit_code()
        }
    };
    std::process::exit(code);
}
