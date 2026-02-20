mod commands;
mod config;
mod export;
mod import;
mod manifest;
mod patch;
mod wizard;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Network;
use patch::Severity;

#[derive(Parser)]
#[command(name = "soroban-registry", about = "Soroban Smart Contract Registry CLI", version)]
struct Cli {
    #[arg(long, env = "SOROBAN_REGISTRY_API_URL", default_value = "http://localhost:3001")]
    api_url: String,

    #[arg(long, env = "SOROBAN_NETWORK")]
    network: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Search {
        query: String,
        #[arg(long)]
        verified_only: bool,
    },
    Info {
        contract_id: String,
    },
    Publish {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        tags: Option<String>,
        #[arg(long)]
        publisher: String,
    },
    List {
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    Migrate {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        wasm: String,
        #[arg(long)]
        simulate_fail: bool,
        #[arg(long)]
        dry_run: bool,
    },
    Export {
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "export.tar.gz")]
        output: String,
        #[arg(long, default_value = ".")]
        contract_dir: String,
    },
    Import {
        #[arg(long)]
        archive: String,
        #[arg(long, default_value = ".")]
        output_dir: String,
    },
    Doc {
        contract_path: String,
        #[arg(long, default_value = "docs")]
        output: String,
    },
    Wizard {},
    History {
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    Patch {
        #[command(subcommand)]
        action: PatchCommands,
    },
    Template {
        #[command(subcommand)]
        action: TemplateCommands,
    },
    Config {
        #[command(subcommand)]
        action: ConfigSubcommands,
    },
    ScanDeps {
        #[arg(long)]
        contract_id: String,
        #[arg(long, default_value = ",")]
        dependencies: String,
        #[arg(long, default_value_t = false)]
        fail_on_high: bool,
    }
}

#[derive(Subcommand)]
enum ConfigSubcommands {
    Get {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        environment: String,
    },
    Set {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        environment: String,
        #[arg(long)]
        config_data: String,
        #[arg(long)]
        secrets_data: Option<String>,
        #[arg(long)]
        created_by: String,
    },
    History {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        environment: String,
    },
    Rollback {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        environment: String,
        #[arg(long)]
        version: i32,
        #[arg(long)]
        created_by: String,
    },
}

#[derive(Subcommand)]
enum PatchCommands {
    Create {
        #[arg(long)]
        version: String,
        #[arg(long)]
        hash: String,
        #[arg(long, default_value = "medium")]
        severity: String,
        #[arg(long, default_value = "100")]
        rollout: u8,
    },
    Notify {
        #[arg(long)]
        patch_id: String,
    },
    Apply {
        #[arg(long)]
        contract_id: String,
        #[arg(long)]
        patch_id: String,
    },
}

#[derive(Subcommand)]
enum TemplateCommands {
    List {
        #[arg(long)]
        category: Option<String>,
    },
    Clone {
        template: String,
        output_name: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        initial_supply: Option<String>,
        #[arg(long)]
        output_dir: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let network = config::resolve_network(cli.network)?;

    match cli.command {
        Commands::Search { query, verified_only } => {
            commands::search(&cli.api_url, &query, network, verified_only).await?;
        }
        Commands::Info { contract_id } => {
            commands::info(&cli.api_url, &contract_id, network).await?;
        }
        Commands::Publish { contract_id, name, description, category, tags, publisher } => {
            let tags_vec = tags
                .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default();
            commands::publish(
                &cli.api_url,
                &contract_id,
                &name,
                description.as_deref(),
                network,
                category.as_deref(),
                tags_vec,
                &publisher,
            )
            .await?;
        }
        Commands::List { limit } => {
            commands::list(&cli.api_url, limit, network).await?;
        }
        Commands::Migrate { contract_id, wasm, simulate_fail, dry_run } => {
            commands::migrate(&cli.api_url, &contract_id, &wasm, simulate_fail, dry_run).await?;
        }
        Commands::Export { id, output, contract_dir } => {
            commands::export(&cli.api_url, &id, &output, &contract_dir).await?;
        }
        Commands::Import { archive, output_dir } => {
            commands::import(&cli.api_url, &archive, network, &output_dir).await?;
        }
        Commands::Doc { contract_path, output } => {
            commands::doc(&contract_path, &output)?;
        }
        Commands::Wizard {} => {
            wizard::run(&cli.api_url).await?;
        }
        Commands::History { search, limit } => {
            wizard::show_history(search.as_deref(), limit)?;
        }
        Commands::Patch { action } => match action {
            PatchCommands::Create { version, hash, severity, rollout } => {
                let sev = severity.parse::<Severity>()?;
                commands::patch_create(&cli.api_url, &version, &hash, sev, rollout).await?;
            }
            PatchCommands::Notify { patch_id } => {
                commands::patch_notify(&cli.api_url, &patch_id).await?;
            }
            PatchCommands::Apply { contract_id, patch_id } => {
                commands::patch_apply(&cli.api_url, &contract_id, &patch_id).await?;
            }
        },
        Commands::Template { action } => match action {
            TemplateCommands::List { category } => {
                commands::template_list(&cli.api_url, category.as_deref()).await?;
            }
            TemplateCommands::Clone { template, output_name, symbol, initial_supply, output_dir } => {
                commands::template_clone(
                    &cli.api_url,
                    &template,
                    &output_name,
                    symbol.as_deref(),
                    initial_supply.as_deref(),
                    output_dir.as_deref(),
                )
                .await?;
            }
        },
        Commands::Config { action } => match action {
            ConfigSubcommands::Get { contract_id, environment } => {
                commands::config_get(&cli.api_url, &contract_id, &environment).await?;
            }
            ConfigSubcommands::Set { contract_id, environment, config_data, secrets_data, created_by } => {
                commands::config_set(&cli.api_url, &contract_id, &environment, &config_data, secrets_data.as_deref(), &created_by).await?;
            }
            ConfigSubcommands::History { contract_id, environment } => {
                commands::config_history(&cli.api_url, &contract_id, &environment).await?;
            }
            ConfigSubcommands::Rollback { contract_id, environment, version, created_by } => {
                commands::config_rollback(&cli.api_url, &contract_id, &environment, version, &created_by).await?;
            }
        },
        Commands::ScanDeps { contract_id, dependencies, fail_on_high } => {
            commands::scan_deps(&cli.api_url, &contract_id, &dependencies, fail_on_high).await?;
        },
    }

    Ok(())
}
