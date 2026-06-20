//! image-retrieval CLI entry point.
//!
//! Supports two command paths:
//! - `run`: execute a full image retrieval task from a QueryPlan.
//! - `self-check`: run readiness checks before a formal task.

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "image-retrieval",
    about = "General-purpose image search, retrieval, validation, and delivery packaging CLI",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Execute a full image retrieval task from a QueryPlan.
    Run {
        /// Path to the QueryPlan JSON file.
        #[arg(short, long, default_value = "query_plan.json")]
        plan: String,
    },
    /// Run readiness self-checks (no search, retrieval, or delivery).
    SelfCheck {
        /// Path to the QueryPlan JSON file for validation.
        #[arg(short, long, default_value = "query_plan.json")]
        plan: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Run { plan } => {
            println!(
                "image-retrieval: run with plan '{}' (not yet implemented)",
                plan
            );
        }
        Command::SelfCheck { plan } => {
            println!(
                "image-retrieval: self-check with plan '{}' (not yet implemented)",
                plan
            );
        }
    }
}
