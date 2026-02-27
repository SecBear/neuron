use brain::{BrainConfig, BrainError, RunRequest, run};
use neuron_mcp::{McpClient, McpServer};
use neuron_tool::{ToolDyn, ToolRegistry};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    if let Err(err) = run_cli().await {
        eprintln!("brain error: {err}");
        std::process::exit(1);
    }
}

async fn run_cli() -> Result<(), BrainError> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut command = "run".to_string();
    if let Some(first) = args.first().cloned() {
        args.remove(0);
        if first == "run" || first == "mcp" || first == "--help" || first == "-h" {
            command = first;
        } else if first.starts_with('-') {
            command = "run".to_string();
            args.insert(0, first);
        } else {
            return Err(BrainError::Config(format!("unknown subcommand: {first}")));
        }
    }

    if command == "--help" || command == "-h" {
        print_usage();
        return Ok(());
    }

    if command == "mcp" {
        return run_mcp(args).await;
    }

    let mut config_path: Option<PathBuf> = None;
    let mut prompt: Option<String> = None;
    let mut state_dir: Option<PathBuf> = None;
    let mut mcp_path: Option<PathBuf> = None;
    let mut allowed_tools: Vec<String> = Vec::new();

    let mut remaining: Vec<String> = args;
    while let Some(flag) = remaining.first().cloned() {
        remaining.remove(0);
        match flag.as_str() {
            "--config" => config_path = Some(PathBuf::from(take_arg("--config", &mut remaining)?)),
            "--prompt" => prompt = Some(take_arg("--prompt", &mut remaining)?),
            "--state-dir" => {
                state_dir = Some(PathBuf::from(take_arg("--state-dir", &mut remaining)?))
            }
            "--mcp" => mcp_path = Some(PathBuf::from(take_arg("--mcp", &mut remaining)?)),
            "--allow-tool" => allowed_tools.push(take_arg("--allow-tool", &mut remaining)?),
            other => return Err(BrainError::Config(format!("unknown flag: {other}"))),
        }
    }

    let mut config = if let Some(path) = config_path {
        BrainConfig::from_path(&path)?
    } else {
        let default_path = PathBuf::from("brain.json");
        if default_path.exists() {
            BrainConfig::from_path(&default_path)?
        } else {
            BrainConfig::default()
        }
    };

    if let Some(path) = state_dir.clone() {
        config.state_dir = path;
    }

    if let Some(mcp) = &mcp_path {
        config.mcp_files.push(mcp.clone());
    }

    let user_message = match prompt {
        Some(text) => text,
        None => {
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            let trimmed = buffer.trim().to_string();
            if trimmed.is_empty() {
                return Err(BrainError::Config(
                    "missing prompt: pass --prompt or pipe stdin".to_string(),
                ));
            }
            trimmed
        }
    };

    let request = RunRequest {
        user_message,
        state_dir,
        mcp_path,
        allowed_tools: if allowed_tools.is_empty() {
            None
        } else {
            Some(allowed_tools)
        },
        ..RunRequest::default()
    };

    let result = run(config, request).await?;
    println!("{}", result.final_answer);
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
struct McpFile {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerEntry>,
    #[serde(default, rename = "x-brain")]
    x_brain: Option<XBrainConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct McpServerEntry {
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct XBrainConfig {
    allowlist: Option<Vec<String>>,
    #[serde(default)]
    denylist: Vec<String>,
    #[serde(default)]
    aliases: HashMap<String, String>,
}

async fn run_mcp(args: Vec<String>) -> Result<(), BrainError> {
    let mut config_path: Option<PathBuf> = None;
    let mut artifact_dir: Option<PathBuf> = None;
    let mut mcp_paths: Vec<PathBuf> = Vec::new();

    let mut remaining = args;
    while let Some(flag) = remaining.first().cloned() {
        remaining.remove(0);
        match flag.as_str() {
            "serve" => {}
            "--config" => config_path = Some(PathBuf::from(take_arg("--config", &mut remaining)?)),
            "--artifact-dir" => {
                artifact_dir = Some(PathBuf::from(take_arg("--artifact-dir", &mut remaining)?))
            }
            "--mcp" => mcp_paths.push(PathBuf::from(take_arg("--mcp", &mut remaining)?)),
            other => return Err(BrainError::Config(format!("unknown flag: {other}"))),
        }
    }

    let mut config = if let Some(path) = config_path {
        BrainConfig::from_path(&path)?
    } else {
        let default_path = PathBuf::from("brain.json");
        if default_path.exists() {
            BrainConfig::from_path(&default_path)?
        } else {
            BrainConfig::default()
        }
    };

    if let Some(path) = artifact_dir {
        config.artifact_dir = path;
    }

    let mut files = config.mcp_files.clone();
    files.extend(mcp_paths);

    let (acquisition_registry, _clients) = load_acquisition_tools(files).await?;

    let manager = Arc::new(brain::v2::JobManager::new(
        config.artifact_dir.clone(),
        acquisition_registry,
    ));
    let exposed = brain::v2::backend_registry(manager);

    McpServer::new(exposed, "brain", env!("CARGO_PKG_VERSION"))
        .serve_stdio()
        .await?;

    let _ = _clients;
    Ok(())
}

async fn load_acquisition_tools(
    mcp_files: Vec<PathBuf>,
) -> Result<(ToolRegistry, Vec<McpClient>), BrainError> {
    let mut allowlist: Option<Vec<String>> = None;
    let mut denylist: HashSet<String> = HashSet::new();
    let mut aliases: HashMap<String, String> = HashMap::new();

    let mut servers = Vec::<McpServerEntry>::new();
    for path in mcp_files {
        let content = std::fs::read_to_string(path)?;
        let parsed: McpFile = serde_json::from_str(&content)?;
        if let Some(x) = parsed.x_brain {
            if let Some(incoming) = x.allowlist {
                let set: HashSet<String> = allowlist
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .chain(incoming.into_iter())
                    .collect();
                allowlist = Some(set.into_iter().collect());
            }
            for denied in x.denylist {
                denylist.insert(denied);
            }
            aliases.extend(x.aliases);
        }
        servers.extend(parsed.mcp_servers.into_values());
    }

    let allowset = allowlist.map(|items| items.into_iter().collect::<HashSet<String>>());

    let mut clients = Vec::new();
    let mut registry = ToolRegistry::new();

    for server in servers {
        if let Some(command) = server.command {
            let mut cmd = tokio::process::Command::new(command);
            cmd.args(server.args);
            let client = McpClient::connect_stdio(cmd).await?;
            let tools = client.discover_tools().await?;
            register_tools(&mut registry, tools, &aliases, &allowset, &denylist);
            clients.push(client);
        } else if let Some(url) = server.url {
            let client = McpClient::connect_sse(&url).await?;
            let tools = client.discover_tools().await?;
            register_tools(&mut registry, tools, &aliases, &allowset, &denylist);
            clients.push(client);
        }
    }

    Ok((registry, clients))
}

fn register_tools(
    registry: &mut ToolRegistry,
    tools: Vec<Arc<dyn ToolDyn>>,
    aliases: &HashMap<String, String>,
    allowset: &Option<HashSet<String>>,
    denyset: &HashSet<String>,
) {
    for tool in tools {
        let tool_name = tool.name().to_string();
        let wrapped: Arc<dyn ToolDyn> = if let Some(alias) = aliases.get(&tool_name) {
            Arc::new(AliasedTool::new(alias.clone(), tool))
        } else {
            tool
        };
        if is_tool_allowed(wrapped.name(), allowset, denyset) {
            registry.register(wrapped);
        }
    }
}

fn is_tool_allowed(
    name: &str,
    allowset: &Option<HashSet<String>>,
    denyset: &HashSet<String>,
) -> bool {
    if denyset.contains(name) {
        return false;
    }
    match allowset {
        Some(allowed) => allowed.contains(name),
        None => true,
    }
}

struct AliasedTool {
    alias: String,
    inner: Arc<dyn ToolDyn>,
}

impl AliasedTool {
    fn new(alias: String, inner: Arc<dyn ToolDyn>) -> Self {
        Self { alias, inner }
    }
}

impl ToolDyn for AliasedTool {
    fn name(&self) -> &str {
        &self.alias
    }
    fn description(&self) -> &str {
        self.inner.description()
    }
    fn input_schema(&self) -> serde_json::Value {
        self.inner.input_schema()
    }
    fn call(
        &self,
        input: serde_json::Value,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<serde_json::Value, neuron_tool::ToolError>>
                + Send
                + '_,
        >,
    > {
        self.inner.call(input)
    }
}

fn take_arg(flag: &str, remaining: &mut Vec<String>) -> Result<String, BrainError> {
    if remaining.is_empty() {
        return Err(BrainError::Config(format!("missing value for {flag}")));
    }
    Ok(remaining.remove(0))
}

fn print_usage() {
    println!(
        "brain run [--config brain.json] [--prompt TEXT] [--state-dir PATH] [--mcp PATH] [--allow-tool NAME]\n\
brain mcp serve [--config brain.json] [--artifact-dir PATH] [--mcp PATH]"
    );
}
