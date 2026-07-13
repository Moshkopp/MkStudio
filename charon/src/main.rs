fn main() -> std::io::Result<()> {
    let config = charon::ServerConfig::from_env()?;
    println!(
        "Charon {} lauscht auf http://{}",
        env!("CARGO_PKG_VERSION"),
        config.bind
    );
    charon::serve(config)
}
