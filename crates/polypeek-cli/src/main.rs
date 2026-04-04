use std::{fs::File, io, process};

use polypeek_core::{
    builtin::register_builtins, port::Port, registry::Registry, session::Session,
    terminal::TerminalPort,
};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), polypeek_core::error::Error> {
    let args: Vec<String> = std::env::args().collect();

    let path = match args.get(1).map(String::as_str) {
        Some("--help" | "-h") | None => {
            eprintln!("usage: polypeek <file>");
            eprintln!("       polypeek -    (read from stdin)");
            process::exit(0);
        }
        Some(p) => p,
    };

    let mut registry = Registry::new();
    register_builtins(&mut registry);

    let session = Session::new(&registry);
    let mut port = TerminalPort::new();

    let src: Box<dyn io::Read + Send + 'static> = if path == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(File::open(path).map_err(polypeek_core::error::Error::Io)?)
    };

    let msgs = session.run(src, port.accepts())?;
    port.render(msgs)?;

    Ok(())
}
