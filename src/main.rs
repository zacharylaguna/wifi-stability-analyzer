mod metrics;
mod monitor;
mod storage;
mod web;
mod analysis;
mod gui;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

use crate::storage::MetricsStore;
use crate::monitor::WifiMonitor;
use crate::web::start_web_server;

#[derive(Parser)]
#[command(name = "wifi-stability-tracker")]
#[command(about = "A comprehensive WiFi stability debugging tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start monitoring WiFi stability
    Monitor {
        /// Interval between measurements in seconds
        #[arg(short, long, default_value = "5")]
        interval: u64,

        /// Path to store the database
        #[arg(short, long, default_value = "wifi_metrics.db")]
        database: PathBuf,

        /// Port for the web dashboard
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Path to store log files
        #[arg(short, long, default_value = "logs")]
        log_dir: PathBuf,

        /// Targets to ping for latency tests (comma-separated)
        #[arg(long, default_value = "8.8.8.8,1.1.1.1,google.com")]
        ping_targets: String,

        /// DNS servers to test (comma-separated)
        #[arg(long, default_value = "8.8.8.8,1.1.1.1")]
        dns_servers: String,

        /// Disable GUI window and use browser only
        #[arg(long, default_value = "false")]
        no_gui: bool,
    },
    /// Export collected data to JSON
    Export {
        /// Path to the database
        #[arg(short, long, default_value = "wifi_metrics.db")]
        database: PathBuf,

        /// Output file path
        #[arg(short, long, default_value = "wifi_export.json")]
        output: PathBuf,

        /// Start time filter (ISO 8601 format)
        #[arg(long)]
        start: Option<String>,

        /// End time filter (ISO 8601 format)
        #[arg(long)]
        end: Option<String>,
    },
    /// Analyze collected data and generate a report
    Analyze {
        /// Path to the database
        #[arg(short, long, default_value = "wifi_metrics.db")]
        database: PathBuf,

        /// Output report file
        #[arg(short, long, default_value = "wifi_report.txt")]
        output: PathBuf,
    },
    /// View the dashboard without starting new monitoring
    Dashboard {
        /// Path to the database
        #[arg(short, long, default_value = "wifi_metrics.db")]
        database: PathBuf,

        /// Port for the web dashboard
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Disable GUI window and use browser only
        #[arg(long, default_value = "false")]
        no_gui: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Monitor {
            interval,
            database,
            port,
            log_dir,
            ping_targets,
            dns_servers,
            no_gui,
        } => {
            // Set up logging
            std::fs::create_dir_all(&log_dir)?;
            let file_appender = RollingFileAppender::new(Rotation::HOURLY, &log_dir, "wifi-monitor.log");
            let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

            tracing_subscriber::registry()
                .with(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
                .with(fmt::layer().with_writer(std::io::stdout))
                .with(fmt::layer().json().with_writer(non_blocking))
                .init();

            info!("Starting WiFi Stability Tracker");
            info!("Database: {:?}", database);
            info!("Monitoring interval: {}s", interval);
            info!("Web dashboard: http://localhost:{}", port);

            // Reset database - delete existing file if present
            if database.exists() {
                info!("Removing existing database file");
                std::fs::remove_file(&database)?;
            }

            // Initialize storage
            let store = Arc::new(MetricsStore::new(&database)?);

            // Parse targets
            let ping_targets: Vec<String> = ping_targets.split(',').map(|s| s.trim().to_string()).collect();
            let dns_servers: Vec<String> = dns_servers.split(',').map(|s| s.trim().to_string()).collect();

            // Create monitor
            let monitor = WifiMonitor::new(
                store.clone(),
                interval,
                ping_targets,
                dns_servers,
            );

            // Start web server in background
            let web_store = store.clone();
            let web_port = port;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Err(e) = start_web_server(web_store, web_port).await {
                        tracing::error!("Web server error: {}", e);
                    }
                });
            });

            // Give web server time to start
            std::thread::sleep(std::time::Duration::from_secs(2));

            // Start monitoring in background
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    monitor.start().await;
                });
            });

            // Launch GUI or wait for Ctrl+C
            if !no_gui {
                info!("Launching GUI window...");
                gui::launch_gui(port)?;
            } else {
                info!("Running in headless mode. Press Ctrl+C to stop monitoring");
                info!("Open http://localhost:{} in your browser", port);
                tokio::signal::ctrl_c().await?;
                info!("Shutting down...");
            }

            Ok(())
        }
        Commands::Export {
            database,
            output,
            start,
            end,
        } => {
            let store = MetricsStore::new(&database)?;
            let data = store.export_json(start.as_deref(), end.as_deref())?;
            std::fs::write(&output, data)?;
            println!("Exported data to {:?}", output);
            Ok(())
        }
        Commands::Analyze { database, output } => {
            let store = MetricsStore::new(&database)?;
            let report = analysis::generate_report(&store)?;
            std::fs::write(&output, &report)?;
            println!("{}", report);
            println!("\nReport saved to {:?}", output);
            Ok(())
        }
        Commands::Dashboard { database, port, no_gui } => {
            tracing_subscriber::registry()
                .with(EnvFilter::from_default_env().add_directive(Level::INFO.into()))
                .with(fmt::layer())
                .init();

            info!("Starting dashboard-only mode");
            info!("Web dashboard: http://localhost:{}", port);

            let store = Arc::new(MetricsStore::new(&database)?);
            
            // Start web server in background thread
            let web_port = port;
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    if let Err(e) = start_web_server(store, web_port).await {
                        tracing::error!("Web server error: {}", e);
                    }
                });
            });

            // Give web server time to start
            std::thread::sleep(std::time::Duration::from_secs(2));

            // Launch GUI or wait for Ctrl+C
            if !no_gui {
                info!("Launching GUI window...");
                gui::launch_gui(port)?;
            } else {
                info!("Open http://localhost:{} in your browser", port);
                tokio::signal::ctrl_c().await?;
                info!("Shutting down...");
            }

            Ok(())
        }
    }
}
