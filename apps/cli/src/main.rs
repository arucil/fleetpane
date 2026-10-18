use std::{
    io::{self, Write},
    net::SocketAddr,
    process::ExitCode,
};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "Usage: fleetpane-cli [LOOPBACK_IP:PORT] [CURSOR]\nPrint one page of active Paseo sessions as JSON. Default: 127.0.0.1:6767"
        );
        return Ok(());
    }
    if args.len() > 2 {
        return Err("expected an address and optional cursor; use --help".into());
    }
    let address: SocketAddr = args
        .first()
        .map(String::as_str)
        .unwrap_or("127.0.0.1:6767")
        .parse()?;
    let page = fleetpane_client::fetch_directory(address, args.get(1).map(String::as_str))?;
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, &page)?;
    writeln!(output)?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fleetpane: {error}");
            ExitCode::FAILURE
        }
    }
}
