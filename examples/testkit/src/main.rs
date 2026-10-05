//! Local Anvil/PostgreSQL verification, Uniswap smoke and ERC20 rollback workflows.
mod erc20;
mod kit;
mod signal;
mod uniswap;
mod values;
mod verify;

use clap::{Parser, Subcommand};
use kit::Testkit;
use serde_json::json;
use signal::install_signal_handlers;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Returns a verification error so the workflow can record a failed report.
pub(crate) fn require(condition: bool, message: impl AsRef<str>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(message.as_ref()).into())
    }
}

#[derive(Parser)]
#[command(about = "Verify Raven with local Anvil and configured PostgreSQL")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build/test the workspace and verify both examples.
    Verify,
    /// Exercise pinned official V3 contracts without PostgreSQL or workspace tests.
    OfficialV3Smoke,
    /// Show ERC20 automatic rollback on a same-height Anvil fork.
    Erc20Rollback,
}

/// Executes the selected workflow and writes its final report.
fn run(workflow: Command) -> Result<bool> {
    let mut testkit = Testkit::new()?;
    let (report_name, result) = match workflow {
        Command::Verify => ("report.json", testkit.verify()),
        Command::OfficialV3Smoke => ("official-v3-smoke-report.json", testkit.official_v3_smoke()),
        Command::Erc20Rollback => ("rollback-report.json", testkit.erc20_rollback()),
    };
    if let Err(error) = result {
        testkit.report["status"] = json!("failed");
        testkit.report["error"] = json!(error.to_string());
    }
    if let Err(error) = testkit.cleanup() {
        testkit.report["status"] = json!("failed");
        testkit.report["cleanup_error"] = json!(error.to_string());
    }
    let report = testkit.work.join(report_name);
    std::fs::write(
        &report,
        format!("{}\n", serde_json::to_string_pretty(&testkit.report)?),
    )?;
    if testkit.report["database_retained"] == true {
        println!("Configured test database: {}", testkit.database_name);
        if let Some(schema) = testkit.report["schema"].as_str() {
            println!("ERC20 schema: {schema}");
        }
    }
    println!("Testkit report and process logs: {}", report.display());
    Ok(testkit.report["status"] == "passed")
}

/// Parses CLI options and exits with the workflow result.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    dotenv::dotenv().ok();
    let cli = Cli::parse();
    if let Err(error) = install_signal_handlers() {
        eprintln!("Cannot listen for shutdown signals: {error}");
        return std::process::ExitCode::FAILURE;
    }
    let passed = match tokio::task::spawn_blocking(move || match run(cli.command) {
        Ok(passed) => passed,
        Err(error) => {
            eprintln!("Testkit failed: {error}");
            false
        }
    })
    .await
    {
        Ok(passed) => passed,
        Err(error) => {
            eprintln!("Testkit task failed: {error}");
            false
        }
    };
    if passed {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
