use crate::config::Network;
use anyhow::{Context, Result};
use colored::Colorize;
use serde_json::json;
use shared::{extract_abi, generate_markdown};
use std::fs;

use crate::patch::{PatchManager, Severity};

pub async fn search(
    api_url: &str,
    query: &str,
    network: Network,
    verified_only: bool,
) -> Result<()> {
    let client = reqwest::Client::new();
    let mut url = format!("{}/api/contracts?query={}&network={}", api_url, query, network);

    if verified_only {
        url.push_str("&verified_only=true");
    }

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to search contracts")?;

    let data: serde_json::Value = response.json().await?;
    let items = data["items"].as_array().context("Invalid response")?;

    println!("\n{}", "Search Results:".bold().cyan());
    println!("{}", "=".repeat(80).cyan());

    if items.is_empty() {
        println!("{}", "No contracts found.".yellow());
        return Ok(());
    }

    for contract in items {
        let name = contract["name"].as_str().unwrap_or("Unknown");
        let contract_id = contract["contract_id"].as_str().unwrap_or("");
        let is_verified = contract["is_verified"].as_bool().unwrap_or(false);
        let network = contract["network"].as_str().unwrap_or("");

        println!("\n{} {}", "●".green(), name.bold());
        println!("  ID: {}", contract_id.bright_black());
        println!(
            "  Status: {} | Network: {}",
            if is_verified {
                "✓ Verified".green()
            } else {
                "○ Unverified".yellow()
            },
            network.bright_blue()
        );

        if let Some(desc) = contract["description"].as_str() {
            println!("  {}", desc.bright_black());
        }
    }

    println!("\n{}", "=".repeat(80).cyan());
    println!("Found {} contract(s)\n", items.len());

    Ok(())
}

pub async fn info(api_url: &str, contract_id: &str, network: Network) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts/{}?network={}", api_url, contract_id, network);

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to fetch contract info")?;

    if !response.status().is_success() {
        anyhow::bail!("Contract not found on {}", network);
    }

    let contract: serde_json::Value = response.json().await?;

    println!("\n{}", "Contract Information:".bold().cyan());
    println!("{}", "=".repeat(80).cyan());

    println!("\n{}: {}", "Name".bold(), contract["name"].as_str().unwrap_or("Unknown"));
    println!("{}: {}", "Contract ID".bold(), contract["contract_id"].as_str().unwrap_or(""));
    println!("{}: {}", "Network".bold(), contract["network"].as_str().unwrap_or("").bright_blue());

    let is_verified = contract["is_verified"].as_bool().unwrap_or(false);
    println!(
        "{}: {}",
        "Verified".bold(),
        if is_verified {
            "✓ Yes".green()
        } else {
            "○ No".yellow()
        }
    );

    if let Some(desc) = contract["description"].as_str() {
        println!("\n{}: {}", "Description".bold(), desc);
    }

    if let Some(tags) = contract["tags"].as_array() {
        if !tags.is_empty() {
            print!("\n{}: ", "Tags".bold());
            for (i, tag) in tags.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{}", tag.as_str().unwrap_or("").bright_magenta());
            }
            println!();
        }
    }

    println!("\n{}", "=".repeat(80).cyan());
    println!();

    Ok(())
}

pub async fn publish(
    api_url: &str,
    contract_id: &str,
    name: &str,
    description: Option<&str>,
    network: Network,
    category: Option<&str>,
    tags: Vec<String>,
    publisher: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts", api_url);

    let payload = json!({
        "contract_id": contract_id,
        "name": name,
        "description": description,
        "network": network.to_string(),
        "category": category,
        "tags": tags,
        "publisher_address": publisher,
    });

    println!("\n{}", "Publishing contract...".bold().cyan());

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .context("Failed to publish contract")?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to publish: {}", error_text);
    }

    let contract: serde_json::Value = response.json().await?;

    println!("{}", "✓ Contract published successfully!".green().bold());
    println!("\n{}: {}", "Name".bold(), contract["name"].as_str().unwrap_or(""));
    println!("{}: {}", "ID".bold(), contract["contract_id"].as_str().unwrap_or(""));
    println!("{}: {}", "Network".bold(), contract["network"].as_str().unwrap_or("").bright_blue());
    println!();

    Ok(())
}

pub async fn list(api_url: &str, limit: usize, network: Network) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts?page_size={}&network={}", api_url, limit, network);

    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to list contracts")?;

    let data: serde_json::Value = response.json().await?;
    let items = data["items"].as_array().context("Invalid response")?;

    println!("\n{}", "Recent Contracts:".bold().cyan());
    println!("{}", "=".repeat(80).cyan());

    if items.is_empty() {
        println!("{}", "No contracts found.".yellow());
        return Ok(());
    }

    for (i, contract) in items.iter().enumerate() {
        let name = contract["name"].as_str().unwrap_or("Unknown");
        let contract_id = contract["contract_id"].as_str().unwrap_or("");
        let is_verified = contract["is_verified"].as_bool().unwrap_or(false);
        let network = contract["network"].as_str().unwrap_or("");

        println!(
            "\n{}. {} {}",
            i + 1,
            name.bold(),
            if is_verified { "✓".green() } else { "".normal() }
        );
        println!("   {} | {}", contract_id.bright_black(), network.bright_blue());
    }

    println!("\n{}", "=".repeat(80).cyan());
    println!();

    Ok(())
}

pub async fn migrate(
    api_url: &str,
    contract_id: &str,
    wasm_path: &str,
    simulate_fail: bool,
    dry_run: bool,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::fs;
    use tokio::process::Command;

    println!("\n{}", "Migration Tool".bold().cyan());
    println!("{}", "=".repeat(80).cyan());

    // 1. Read WASM file
    let wasm_bytes = fs::read(wasm_path)
        .with_context(|| format!("Failed to read WASM file at {}", wasm_path))?;

    // 2. Compute Hash
    let mut hasher = Sha256::new();
    hasher.update(&wasm_bytes);
    let wasm_hash = hex::encode(hasher.finalize());

    println!("Contract ID: {}", contract_id.green());
    println!("WASM Path:   {}", wasm_path);
    println!("WASM Hash:   {}", wasm_hash.bright_black());
    println!("Size:        {} bytes", wasm_bytes.len());

    if dry_run {
        println!("\n{}", "[DRY RUN] No changes will be made.".yellow().bold());
        println!("Would create migration record...");
        println!("Would execute: soroban contract invoke --id {} --wasm {} ...", contract_id, wasm_path);
        return Ok(());
    }

    // 3. Create Migration Record (Pending)
    let client = reqwest::Client::new();
    let create_url = format!("{}/api/migrations", api_url);

    let payload = json!({
        "contract_id": contract_id,
        "wasm_hash": wasm_hash,
    });

    print!("\nInitializing migration... ");
    let response = client.post(&create_url)
        .json(&payload)
        .send()
        .await
        .context("Failed to contact registry API")?;

    if !response.status().is_success() {
        println!("{}", "Failed".red());
        let err = response.text().await?;
        anyhow::bail!("API Error: {}", err);
    }

    let migration: serde_json::Value = response.json().await?;
    let migration_id = migration["id"].as_str().unwrap();
    println!("{}", "OK".green());
    println!("Migration ID: {}", migration_id);

    // 4. Execute Migration (Mock or Real)
    println!("\n{}", "Executing migration logic...".bold());

    // Check if soroban is installed
    let version_output = Command::new("soroban")
        .arg("--version")
        .output()
        .await;

    let (status, log_output) = if version_output.is_err() {
        println!("{}", "Warning: 'soroban' CLI not found. Running in MOCK mode.".yellow());

        if simulate_fail {
            println!("{}", "Simulating FAILURE...".red());
            (shared::models::MigrationStatus::Failed, "Simulation: Migration failed as requested.".to_string())
        } else {
            println!("{}", "Simulating SUCCESS...".green());
            (shared::models::MigrationStatus::Success, "Simulation: Migration succeeded.".to_string())
        }
    } else {
        // Real execution would go here. For now we will just mock it even if soroban exists
        // because we don't have a real contract to invoke in this environment.
        println!("{}", "Soroban CLI found, but full integration is pending. Running in MOCK mode.".yellow());
        if simulate_fail {
            println!("{}", "Simulating FAILURE...".red());
            (shared::models::MigrationStatus::Failed, "Simulation: Migration failed as requested.".to_string())
        } else {
            println!("{}", "Simulating SUCCESS...".green());
            (shared::models::MigrationStatus::Success, "Simulation: Migration executed successfully via soroban CLI (mocked).".to_string())
        }
    };

    // 5. Update Status
    let update_url = format!("{}/api/migrations/{}", api_url, migration_id);
    let update_payload = json!({
        "status": status,
        "log_output": log_output
    });

    let update_res = client.put(&update_url)
        .json(&update_payload)
        .send()
        .await
        .context("Failed to update migration status")?;

    if !update_res.status().is_success() {
        println!("{}", "Failed to update status!".red());
    } else {
        println!("\n{}", "Migration recorded successfully.".green().bold());
        if status == shared::models::MigrationStatus::Failed {
            println!("{}", "Status: FAILED".red().bold());
        } else {
            println!("{}", "Status: SUCCESS".green().bold());
        }
    }

    Ok(())
}

pub async fn export(
    api_url: &str,
    contract_id: &str,
    output: &str,
    contract_dir: &str,
) -> Result<()> {
    println!("\n{}", "Exporting contract...".bold().cyan());

    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts/{}", api_url, contract_id);

    let (name, network) = match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let data: serde_json::Value = resp.json().await?;
            (
                data["name"].as_str().unwrap_or(contract_id).to_string(),
                data["network"].as_str().unwrap_or("unknown").to_string(),
            )
        }
        _ => (contract_id.to_string(), "unknown".to_string()),
    };

    let source = std::path::Path::new(contract_dir);
    anyhow::ensure!(source.is_dir(), "contract directory does not exist: {}", contract_dir);

    crate::export::create_archive(
        source,
        std::path::Path::new(output),
        contract_id,
        &name,
        &network,
    )?;

    println!("{}", "✓ Export complete!".green().bold());
    println!("  {}: {}", "Output".bold(), output);
    println!("  {}: {}", "Contract".bold(), contract_id.bright_black());
    println!("  {}: {}\n", "Name".bold(), name);

    Ok(())
}

pub async fn import(
    api_url: &str,
    archive: &str,
    network: Network,
    output_dir: &str,
) -> Result<()> {
    println!("\n{}", "Importing contract...".bold().cyan());

    let archive_path = std::path::Path::new(archive);
    anyhow::ensure!(archive_path.is_file(), "archive not found: {}", archive);

    let dest = std::path::Path::new(output_dir);

    let manifest = crate::import::extract_and_verify(archive_path, dest)?;

    println!("{}", "✓ Import complete — integrity verified!".green().bold());
    println!("  {}: {}", "Contract".bold(), manifest.contract_id.bright_black());
    println!("  {}: {}", "Name".bold(), manifest.name);
    println!("  {}: {}", "Network".bold(), network.to_string().bright_blue());
    println!("  {}: {}", "SHA-256".bold(), manifest.sha256.bright_black());
    println!("  {}: {}", "Exported At".bold(), manifest.exported_at);
    println!("  {}: {} file(s)", "Contents".bold(), manifest.contents.len());
    println!("  {}: {}", "Extracted To".bold(), output_dir);

    println!(
        "\n  {} To register on {}, run:",
        "→".bright_black(),
        network.to_string().bright_blue()
    );
    println!(
        "    soroban-registry publish --contract-id {} --name \"{}\" --network {} --publisher <address>\n",
        manifest.contract_id, manifest.name, network
    );

    Ok(())
}

fn severity_colored(sev: &Severity) -> colored::ColoredString {
    match sev {
        Severity::Critical => "CRITICAL".red().bold(),
        Severity::High => "HIGH".yellow().bold(),
        Severity::Medium => "MEDIUM".cyan(),
        Severity::Low => "LOW".normal(),
    }
}

pub async fn patch_create(
    api_url: &str,
    version: &str,
    hash: &str,
    severity: Severity,
    rollout: u8,
) -> Result<()> {
    println!("\n{}", "Creating security patch...".bold().cyan());

    let patch = PatchManager::create(api_url, version, hash, severity, rollout).await?;

    println!("{}", "✓ Patch created!".green().bold());
    println!("  {}: {}", "ID".bold(), patch.id);
    println!("  {}: {}", "Target Version".bold(), patch.target_version);
    println!("  {}: {}", "Severity".bold(), severity_colored(&patch.severity));
    println!("  {}: {}", "New WASM Hash".bold(), patch.new_wasm_hash.bright_black());
    println!("  {}: {}%\n", "Rollout".bold(), patch.rollout_percentage);

    if matches!(patch.severity, Severity::Critical | Severity::High) {
        println!(
            "  {} {}",
            "⚠".red(),
            format!("{} severity — immediate action recommended", severity_colored(&patch.severity)).red()
        );
    }

    Ok(())
}

pub async fn patch_notify(api_url: &str, patch_id: &str) -> Result<()> {
    println!("\n{}", "Identifying vulnerable contracts...".bold().cyan());

    let (patch, contracts) = PatchManager::find_vulnerable(api_url, patch_id).await?;

    println!(
        "\n{} {} patch for version {}",
        "⚠".bold(),
        severity_colored(&patch.severity),
        patch.target_version.bold()
    );
    println!("{}", "=".repeat(80).cyan());

    if contracts.is_empty() {
        println!("{}", "No vulnerable contracts found.".green());
        return Ok(());
    }

    for (i, c) in contracts.iter().enumerate() {
        let cid = c["contract_id"].as_str().unwrap_or("");
        let name = c["name"].as_str().unwrap_or("Unknown");
        let net = c["network"].as_str().unwrap_or("");
        println!(
            "  {}. {} ({}) [{}]",
            i + 1,
            name.bold(),
            cid.bright_black(),
            net.bright_blue()
        );
    }

    println!("\n{}", "=".repeat(80).cyan());
    println!("{} vulnerable contract(s) found\n", contracts.len());

    Ok(())
}

pub async fn patch_apply(
    api_url: &str,
    contract_id: &str,
    patch_id: &str,
) -> Result<()> {
    println!("\n{}", "Applying security patch...".bold().cyan());

    let audit = PatchManager::apply(api_url, contract_id, patch_id).await?;

    println!("{}", "✓ Patch applied successfully!".green().bold());
    println!("  {}: {}", "Contract".bold(), audit.contract_id);
    println!("  {}: {}", "Patch".bold(), audit.patch_id);
    println!("  {}: {}\n", "Applied At".bold(), audit.applied_at);

    Ok(())
}

pub async fn template_list(api_url: &str, category: Option<&str>) -> Result<()> {
    let client = reqwest::Client::new();
    let mut url = format!("{}/api/templates", api_url);
    if let Some(cat) = category {
        url.push_str(&format!("?category={}", cat));
    }

    let response = client.get(&url).send().await.context("Failed to fetch templates")?;
    if !response.status().is_success() {
        anyhow::bail!("API error: {}", response.text().await?);
    }

    let templates: serde_json::Value = response.json().await?;
    let items = templates.as_array().context("Invalid response")?;

    println!("\n{}", "Available Templates:".bold().cyan());
    println!("{}", "=".repeat(80).cyan());

    if items.is_empty() {
        println!("{}", "No templates found.".yellow());
        return Ok(());
    }

    for t in items {
        let name = t["name"].as_str().unwrap_or("Unknown");
        let slug = t["slug"].as_str().unwrap_or("");
        let category = t["category"].as_str().unwrap_or("");
        let version = t["version"].as_str().unwrap_or("1.0.0");
        let installs = t["install_count"].as_i64().unwrap_or(0);
        let desc = t["description"].as_str().unwrap_or("");

        println!("\n{} {} {}", "●".green(), name.bold(), format!("v{}", version).bright_black());
        println!("  slug: {} | category: {} | installs: {}", slug.bright_blue(), category.bright_magenta(), installs);
        if !desc.is_empty() {
            println!("  {}", desc.bright_black());
        }
        println!("  Clone: soroban-registry template clone {} my-{}", slug, slug);
    }

    println!("\n{}", "=".repeat(80).cyan());
    println!("Found {} template(s)\n", items.len());

    Ok(())
}

pub async fn template_clone(
    api_url: &str,
    template: &str,
    output_name: &str,
    symbol: Option<&str>,
    initial_supply: Option<&str>,
    output_dir: Option<&str>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/templates/{}/clone", api_url, template);

    let mut params = serde_json::Map::new();
    if let Some(sym) = symbol {
        params.insert("SYMBOL".into(), serde_json::Value::String(sym.to_string()));
    }
    if let Some(supply) = initial_supply {
        params.insert("INITIAL_SUPPLY".into(), serde_json::Value::String(supply.to_string()));
    }

    let payload = json!({
        "name": output_name,
        "parameters": params,
    });

    println!("\n{}", "Cloning template...".bold().cyan());

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .context("Failed to clone template")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to clone template: {}", response.text().await?);
    }

    let result: serde_json::Value = response.json().await?;
    let source = result["source_code"].as_str().unwrap_or("");
    let version = result["version"].as_str().unwrap_or("1.0.0");

    let dir = output_dir.unwrap_or(".");
    fs::create_dir_all(dir).context("Failed to create output directory")?;

    let snake_name: String = output_name
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let file_path = format!("{}/{}.rs", dir, snake_name);
    fs::write(&file_path, source).context("Failed to write contract source")?;

    println!("{}", "✓ Template cloned successfully!".green().bold());
    println!("  {}: {}", "Template".bold(), template.bright_blue());
    println!("  {}: {}", "Contract".bold(), output_name.bold());
    println!("  {}: {}", "Version".bold(), version);
    println!("  {}: {}", "Output".bold(), file_path.bright_black());
    println!("\n  {} Next steps:", "→".bright_black());
    println!("    1. Review and customise {}", file_path);
    println!("    2. cargo build --target wasm32-unknown-unknown --release");
    println!("    3. soroban-registry publish --contract-id <ID> --name \"{}\" --publisher <ADDR>\n", output_name);

    Ok(())
}

pub async fn config_get(api_url: &str, contract_id: &str, environment: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts/{}/config?environment={}", api_url, contract_id, environment);

    let response = client.get(&url).send().await.context("Failed to fetch configuration")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to get config: {}", response.text().await.unwrap_or_default());
    }

    let config: serde_json::Value = response.json().await?;

    println!("\n{}", "Contract Configuration (Latest):".bold().cyan());
    println!("{}", "=".repeat(80).cyan());
    println!("{}: {}", "Contract ID".bold(), contract_id);
    println!("{}: {}", "Environment".bold(), environment);
    println!("{}: {}", "Version".bold(), config["version"].as_i64().unwrap_or(0));
    println!("{}: {}", "Contains Secrets".bold(), config["has_secrets"].as_bool().unwrap_or(false));
    println!("{}: {}", "Created By".bold(), config["created_by"].as_str().unwrap_or("Unknown"));
    println!("{}:", "Config Data".bold());
    println!("{}", serde_json::to_string_pretty(&config["config_data"]).unwrap_or_default().green());
    println!();

    Ok(())
}

pub async fn config_set(
    api_url: &str,
    contract_id: &str,
    environment: &str,
    config_data: &str,
    secrets_data: Option<&str>,
    created_by: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts/{}/config", api_url, contract_id);

    let mut payload = json!({
        "environment": environment,
        "config_data": serde_json::from_str::<serde_json::Value>(config_data).context("Invalid config JSON")?,
        "created_by": created_by,
    });

    if let Some(sec) = secrets_data {
        let sec_json: serde_json::Value = serde_json::from_str(sec).context("Invalid secrets JSON")?;
        payload["secrets_data"] = sec_json;
    }

    println!("\n{}", "Publishing configuration...".bold().cyan());

    let response = client.post(&url).json(&payload).send().await.context("Failed to set configuration")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to set config: {}", response.text().await.unwrap_or_default());
    }

    let config: serde_json::Value = response.json().await?;

    println!("{}", "✓ Configuration published successfully!".green().bold());
    println!("  {}: {}", "Environment".bold(), environment);
    println!("  {}: {}", "New Version".bold(), config["version"].as_i64().unwrap_or(0));
    println!();

    Ok(())
}

pub async fn config_history(api_url: &str, contract_id: &str, environment: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts/{}/config/history?environment={}", api_url, contract_id, environment);

    let response = client.get(&url).send().await.context("Failed to fetch configuration history")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to get config history: {}", response.text().await.unwrap_or_default());
    }

    let configs: Vec<serde_json::Value> = response.json().await?;

    println!("\n{}", "Configuration History:".bold().cyan());
    println!("{}", "=".repeat(80).cyan());

    if configs.is_empty() {
        println!("{}", "No configurations found.".yellow());
        return Ok(());
    }

    for (i, config) in configs.iter().enumerate() {
        println!(
            "  {}. {} (v{}) - By: {}",
            i + 1,
            config["created_at"].as_str().unwrap_or("Unknown Date").bright_black(),
            config["version"].as_i64().unwrap_or(0),
            config["created_by"].as_str().unwrap_or("Unknown").bright_blue()
        );
    }
    println!();

    Ok(())
}

pub async fn config_rollback(
    api_url: &str,
    contract_id: &str,
    environment: &str,
    version: i32,
    created_by: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts/{}/config/rollback?environment={}", api_url, contract_id, environment);

    let payload = json!({
        "roll_back_to_version": version,
        "created_by": created_by,
    });

    println!("\n{}", format!("Rolling back configuration to v{}...", version).bold().cyan());

    let response = client.post(&url).json(&payload).send().await.context("Failed to rollback configuration")?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to rollback config: {}", response.text().await.unwrap_or_default());
    }

    let config: serde_json::Value = response.json().await?;

    println!("{}", "✓ Configuration rolled back successfully!".green().bold());
    println!("  {}: {}", "Environment".bold(), environment);
    println!("  {}: {}", "New Active Version".bold(), config["version"].as_i64().unwrap_or(0));
    println!();

    Ok(())
}

pub async fn scan_deps(
    api_url: &str,
    contract_id: &str,
    dependencies: &str,
    fail_on_high: bool,
) -> Result<()> {
    println!("\n{}", "Scanning Dependencies...".bold().cyan());

    let client = reqwest::Client::new();
    let url = format!("{}/api/contracts/{}/scan", api_url, contract_id);

    // Parse dependencies
    let mut deps_list = Vec::new();
    for dep_pair in dependencies.split(',') {
        if dep_pair.is_empty() {
            continue;
        }
        let parts: Vec<&str> = dep_pair.split('@').collect();
        if parts.len() == 2 {
            deps_list.push(json!({
                "package_name": parts[0].trim(),
                "version": parts[1].trim()
            }));
        }
    }

    let payload = json!({
        "dependencies": deps_list,
    });

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .context("Failed to run dependency scan")?;

    if !response.status().is_success() {
        anyhow::bail!("Scan failed: {}", response.text().await.unwrap_or_default());
    }

    let report: serde_json::Value = response.json().await?;
    let findings = report["findings"].as_array().unwrap();

    if findings.is_empty() {
        println!("{}", "✓ No vulnerabilities found!".green().bold());
        return Ok(());
    }

    let mut has_high_severity = false;
    println!("\n{}", "Vulnerabilities Found:".bold().red());
    println!("{}", "=".repeat(80).red());

    for finding in findings {
        let package = finding["package_name"].as_str().unwrap_or("Unknown");
        let version = finding["current_version"].as_str().unwrap_or("Unknown");
        let severity = finding["severity"].as_str().unwrap_or("Unknown");
        let cve_id = finding["cve_id"].as_str().unwrap_or("Unknown");
        let recommended = finding["recommended_version"].as_str().unwrap_or("None");

        let sev_enum = severity.parse::<Severity>().unwrap_or(Severity::Low);
        if matches!(sev_enum, Severity::Critical | Severity::High) {
            has_high_severity = true;
        }

        println!("  {} {}@{} - {}", severity_colored(&sev_enum), package, version, cve_id.bold());
        println!("    {} Recommended patch: {}", "↳".bright_black(), recommended.green());
    }

    println!("\n{}", "=".repeat(80).red());
    println!("{} issue(s) detected\n", findings.len());

    if fail_on_high && has_high_severity {
        std::process::exit(1);
    }

    Ok(())
}
