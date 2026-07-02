mod api;
mod db;
mod import;
mod money;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kachet", about = "Keyboard-first accounting")]
struct Cli {
    /// Path to the SQLite database
    #[arg(long, default_value = "kachet.db")]
    db: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Import a GnuCash XML file (gzipped or plain)
    Import { file: PathBuf },
    /// Run the web server
    Serve {
        #[arg(long, default_value = "127.0.0.1:8710")]
        addr: String,
        /// Serve frontend assets from this directory instead of the
        /// copy embedded in the binary (useful during development)
        #[arg(long)]
        static_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let pool = db::open(&cli.db).await?;

    match cli.command {
        Command::Import { file } => {
            let stats = import::import_file(&pool, &file).await?;
            println!(
                "Imported {} commodities, {} accounts, {} transactions, {} splits, {} prices",
                stats.commodities, stats.accounts, stats.transactions, stats.splits, stats.prices
            );
        }
        Command::Serve { addr, static_dir } => {
            let app = api::router(pool, static_dir.as_deref());
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            println!("kachet listening on http://{addr}");
            axum::serve(listener, app).await?;
        }
    }
    Ok(())
}
