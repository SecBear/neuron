//! Brain v1: agentic research assistant POC.

use layer0::content::Content;
use layer0::id::{AgentId, SessionId};
use layer0::operator::Operator;
use layer0::operator::{OperatorConfig, OperatorInput, TriggerType};
use layer0::state::{StateReader, StateStore};
use neuron_hooks::HookRegistry;
use neuron_mcp::McpClient;
use neuron_op_react::{ReactConfig, ReactOperator};
use neuron_orch_kit::{LocalEffectExecutor, OrchestratedRunner};
use neuron_orch_local::LocalOrch;
use neuron_provider_anthropic::AnthropicProvider;
use neuron_provider_ollama::OllamaProvider;
use neuron_provider_openai::OpenAIProvider;
use neuron_state_fs::FsStore;
use neuron_state_memory::MemoryStore;
use neuron_tool::{ToolDyn, ToolError, ToolRegistry};
use neuron_turn::context::NoCompaction;
use neuron_turn::provider::{Provider, ProviderError};
use neuron_turn::types::{ContentPart, ProviderRequest, ProviderResponse, StopReason, TokenUsage};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use thiserror::Error;

const EFFECT_TOOL_NAMES: [&str; 5] = [
    "write_memory",
    "delete_memory",
    "delegate",
    "handoff",
    "signal",
];

/// Errors returned by brain runtime.
#[derive(Debug, Error)]
pub enum BrainError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("orchestrator error: {0}")]
    Orchestrator(#[from] layer0::OrchError),
    #[error("kit error: {0}")]
    Kit(#[from] neuron_orch_kit::KitError),
    #[error("operator error: {0}")]
    Operator(#[from] layer0::OperatorError),
    #[error("mcp error: {0}")]
    Mcp(#[from] neuron_mcp::McpError),
    #[error("config error: {0}")]
    Config(String),
    #[error("runtime error: {0}")]
    Runtime(String),
}

/// Runtime config loaded from `brain.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrainConfig {
    pub controller: ModelProfile,
    pub workers: WorkerProfiles,
    pub mcp_files: Vec<PathBuf>,
    pub allowlist: Option<Vec<String>>,
    pub denylist: Vec<String>,
    pub disable_local_tools: bool,
    pub state_dir: PathBuf,
    pub artifact_dir: PathBuf,
}

impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            controller: ModelProfile::default(),
            workers: WorkerProfiles::default(),
            mcp_files: vec![],
            allowlist: None,
            denylist: vec![],
            disable_local_tools: false,
            state_dir: default_state_dir(),
            artifact_dir: default_artifact_dir(),
        }
    }
}

impl BrainConfig {
    pub fn from_path(path: &Path) -> Result<Self, BrainError> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }
}

/// Model/provider profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelProfile {
    pub provider: String,
    pub model: String,
    pub api_key: Option<String>,
    pub api_key_env: Option<String>,
    pub base_url: Option<String>,
}

impl Default for ModelProfile {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: "llama3.2:1b".to_string(),
            api_key: None,
            api_key_env: None,
            base_url: None,
        }
    }
}

/// Worker profile including budget controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkerProfile {
    pub provider: ModelProfile,
    pub max_tokens: u32,
    pub max_turns: u32,
    pub max_cost_usd: Option<f64>,
}

impl Default for WorkerProfile {
    fn default() -> Self {
        Self {
            provider: ModelProfile::default(),
            max_tokens: 1024,
            max_turns: 3,
            max_cost_usd: None,
        }
    }
}

/// Worker tool profiles keyed by stable tool ids.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct WorkerProfiles {
    pub sonnet_summarize: WorkerProfile,
    pub nano_extract: WorkerProfile,
    pub codex_generate_patch: WorkerProfile,
}

/// Per-run request options.
#[derive(Debug, Clone, Default)]
pub struct RunRequest {
    pub user_message: String,
    pub session: Option<String>,
    pub state_dir: Option<PathBuf>,
    pub mcp_path: Option<PathBuf>,
    pub allowed_tools: Option<Vec<String>>,
}

/// Result for one brain run.
#[derive(Debug, Clone)]
pub struct BrainRunResult {
    pub final_answer: String,
    pub tool_calls: Vec<String>,
    pub worker_json: Option<Value>,
    pub exposed_tools: Vec<String>,
}

/// Default filesystem state location.
pub fn default_state_dir() -> PathBuf {
    PathBuf::from(".brain/state")
}

/// Default artifact root for human-readable outputs.
pub fn default_artifact_dir() -> PathBuf {
    PathBuf::from(".brain/artifacts")
}

/// Execute brain with configured live providers.
pub async fn run(config: BrainConfig, request: RunRequest) -> Result<BrainRunResult, BrainError> {
    let controller_provider = build_provider(&config.controller)?;
    let workers = WorkerProviderSet {
        sonnet_summarize: build_provider(&config.workers.sonnet_summarize.provider)?,
        nano_extract: build_provider(&config.workers.nano_extract.provider)?,
        codex_generate_patch: build_provider(&config.workers.codex_generate_patch.provider)?,
    };

    let worker_capture = Arc::new(Mutex::new(None));
    run_with_providers(
        controller_provider,
        workers,
        &config,
        request,
        McpMode::Live,
        worker_capture,
    )
    .await
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

#[derive(Debug, Clone, Default, Deserialize)]
struct XBrainConfig {
    allowlist: Option<Vec<String>>,
    #[serde(default)]
    denylist: Vec<String>,
    #[serde(default)]
    aliases: HashMap<String, String>,
}

enum McpMode {
    Live,
    Injected(Vec<Arc<dyn ToolDyn>>),
}

struct WorkerProviderSet {
    sonnet_summarize: AnyProvider,
    nano_extract: AnyProvider,
    codex_generate_patch: AnyProvider,
}

struct Assembly {
    registry: ToolRegistry,
    exposed_tools: Vec<String>,
    operator_allowlist: Vec<String>,
    mcp_clients: Vec<McpClient>,
}

async fn run_with_providers(
    controller_provider: AnyProvider,
    workers: WorkerProviderSet,
    config: &BrainConfig,
    request: RunRequest,
    mcp_mode: McpMode,
    worker_capture: Arc<Mutex<Option<Value>>>,
) -> Result<BrainRunResult, BrainError> {
    let mut assembly = assemble_tools(
        config,
        &request,
        mcp_mode,
        workers,
        Arc::clone(&worker_capture),
    )
    .await?;

    let state_dir = request
        .state_dir
        .clone()
        .unwrap_or_else(|| config.state_dir.clone());
    std::fs::create_dir_all(&state_dir)?;

    let state = Arc::new(FsStore::new(&state_dir));
    let state_reader: Arc<dyn StateReader> = state.clone();
    let state_store: Arc<dyn StateStore> = state;

    let react = ReactOperator::new(
        controller_provider,
        std::mem::take(&mut assembly.registry),
        Box::new(NoCompaction),
        HookRegistry::new(),
        state_reader,
        ReactConfig {
            system_prompt: "You are brain, a research/planning assistant. Use tools for work and return concise final answers.".to_string(),
            default_model: config.controller.model.clone(),
            default_max_tokens: 4096,
            default_max_turns: 10,
        },
    );

    let mut orch = LocalOrch::new();
    orch.register(AgentId::new("brain"), Arc::new(react));

    let runner = OrchestratedRunner::new(
        Arc::new(orch),
        Arc::new(LocalEffectExecutor::new(state_store)),
    );

    let mut input = OperatorInput::new(Content::text(request.user_message), TriggerType::User);
    if let Some(session) = request.session {
        input.session = Some(SessionId::new(session));
    }
    let mut operator_config = OperatorConfig::default();
    operator_config.allowed_tools = Some(assembly.operator_allowlist.clone());
    input.config = Some(operator_config);

    let _mcp_clients = assembly.mcp_clients;
    let trace = runner.run(AgentId::new("brain"), input).await?;
    let output = trace
        .outputs
        .last()
        .ok_or_else(|| BrainError::Runtime("brain run produced no output".to_string()))?;

    let final_answer = output.message.as_text().unwrap_or_default().to_string();
    let tool_calls = output
        .metadata
        .tools_called
        .iter()
        .map(|record| record.name.clone())
        .collect();

    let worker_json = worker_capture.lock().ok().and_then(|guard| guard.clone());

    Ok(BrainRunResult {
        final_answer,
        tool_calls,
        worker_json,
        exposed_tools: assembly.exposed_tools,
    })
}

async fn assemble_tools(
    config: &BrainConfig,
    request: &RunRequest,
    mcp_mode: McpMode,
    workers: WorkerProviderSet,
    worker_capture: Arc<Mutex<Option<Value>>>,
) -> Result<Assembly, BrainError> {
    let run_id = request
        .session
        .clone()
        .unwrap_or_else(|| format!("run-{}", epoch_millis()));
    let artifact_root = config.artifact_dir.join(&run_id);

    let mut allowlist = config.allowlist.clone();
    let mut denylist: HashSet<String> = config.denylist.iter().cloned().collect();
    let mut aliases = HashMap::<String, String>::new();

    let mut mcp_files = config.mcp_files.clone();
    if let Some(path) = &request.mcp_path {
        mcp_files.push(path.clone());
    }

    let mut parsed_servers = Vec::<McpServerEntry>::new();
    for path in &mcp_files {
        let parsed = parse_mcp_file(path)?;
        merge_allowlist(
            &mut allowlist,
            parsed.x_brain.as_ref().and_then(|x| x.allowlist.clone()),
        );
        if let Some(xb) = parsed.x_brain {
            for denied in xb.denylist {
                denylist.insert(denied);
            }
            aliases.extend(xb.aliases);
        }
        parsed_servers.extend(parsed.mcp_servers.into_values());
    }

    let allowset = allowlist
        .clone()
        .map(|items| items.into_iter().collect::<HashSet<String>>());

    let mut mcp_clients = Vec::new();
    let mut discovered_mcp_tools = Vec::<Arc<dyn ToolDyn>>::new();

    match mcp_mode {
        McpMode::Live => {
            for server in parsed_servers {
                if let Some(command) = server.command {
                    let mut cmd = tokio::process::Command::new(command);
                    cmd.args(server.args);
                    let client = McpClient::connect_stdio(cmd).await?;
                    let tools = client.discover_tools().await?;
                    discovered_mcp_tools.extend(tools);
                    mcp_clients.push(client);
                } else if let Some(url) = server.url {
                    let client = McpClient::connect_sse(&url).await?;
                    let tools = client.discover_tools().await?;
                    discovered_mcp_tools.extend(tools);
                    mcp_clients.push(client);
                }
            }
        }
        McpMode::Injected(tools) => {
            discovered_mcp_tools.extend(tools);
        }
    }

    let mut registry = ToolRegistry::new();

    let mut support_tools: Vec<Arc<dyn ToolDyn>> = Vec::new();

    let write_artifact: Arc<dyn ToolDyn> = Arc::new(WriteArtifactTool::new(artifact_root));
    if is_tool_allowed(write_artifact.name(), &allowset, &denylist) {
        support_tools.push(write_artifact);
    }

    for tool in discovered_mcp_tools {
        let tool_name = tool.name().to_string();
        let aliased = if let Some(alias) = aliases.get(&tool_name) {
            Arc::new(AliasedTool::new(alias.clone(), tool)) as Arc<dyn ToolDyn>
        } else {
            tool
        };
        if is_tool_allowed(aliased.name(), &allowset, &denylist) {
            support_tools.push(aliased);
        }
    }

    support_tools.sort_by(|a, b| a.name().cmp(b.name()));
    support_tools.dedup_by(|a, b| a.name() == b.name());

    for tool in &support_tools {
        registry.register(Arc::clone(tool));
    }

    let support_tools = Arc::new(support_tools);

    if !config.disable_local_tools {
        register_if_allowed(
            &mut registry,
            Arc::new(LoopingWorkerTool {
                name: "sonnet_summarize",
                description: "Research/summarize using tools; returns JSON {summary,key_points,artifact_refs?}.",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": {"type": "string"},
                        "goal": {"type": "string"}
                    },
                    "required": ["text", "goal"]
                }),
                provider: workers.sonnet_summarize,
                profile: config.workers.sonnet_summarize.clone(),
                subagent: Some(SubagentConfig {
                    provider: workers.nano_extract.clone(),
                    profile: config.workers.nano_extract.clone(),
                }),
                support_tools: Arc::clone(&support_tools),
                capture: Some(Arc::clone(&worker_capture)),
            }),
            &allowset,
            &denylist,
        );

        register_if_allowed(
            &mut registry,
            Arc::new(LoopingWorkerTool {
                name: "nano_extract",
                description: "Extract structured data from text as JSON.",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "text": {"type": "string"},
                        "schema": {"type": "object"}
                    },
                    "required": ["text", "schema"]
                }),
                provider: workers.nano_extract,
                profile: config.workers.nano_extract.clone(),
                subagent: None,
                support_tools: Arc::clone(&support_tools),
                capture: None,
            }),
            &allowset,
            &denylist,
        );

        register_if_allowed(
            &mut registry,
            Arc::new(LoopingWorkerTool {
                name: "codex_generate_patch",
                description: "Generate candidate patch text and notes as JSON.",
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "task": {"type": "string"},
                        "context": {"type": "object"}
                    },
                    "required": ["task", "context"]
                }),
                provider: workers.codex_generate_patch,
                profile: config.workers.codex_generate_patch.clone(),
                subagent: None,
                support_tools: Arc::clone(&support_tools),
                capture: None,
            }),
            &allowset,
            &denylist,
        );
    }

    let request_allowset = request
        .allowed_tools
        .as_ref()
        .map(|items| items.iter().cloned().collect::<HashSet<String>>());

    let mut exposed_tools: Vec<String> = registry
        .iter()
        .map(|tool| tool.name().to_string())
        .collect();
    exposed_tools.sort();

    if let Some(request_allow) = &request_allowset {
        exposed_tools.retain(|name| request_allow.contains(name));
    }

    let mut operator_allowlist = exposed_tools.clone();
    for effect in EFFECT_TOOL_NAMES {
        if !operator_allowlist.iter().any(|name| name == effect) {
            operator_allowlist.push(effect.to_string());
        }
    }

    Ok(Assembly {
        registry,
        exposed_tools,
        operator_allowlist,
        mcp_clients,
    })
}

fn register_if_allowed(
    registry: &mut ToolRegistry,
    tool: Arc<dyn ToolDyn>,
    allowset: &Option<HashSet<String>>,
    denyset: &HashSet<String>,
) {
    if is_tool_allowed(tool.name(), allowset, denyset) {
        registry.register(tool);
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

fn merge_allowlist(current: &mut Option<Vec<String>>, incoming: Option<Vec<String>>) {
    let Some(incoming) = incoming else {
        return;
    };
    let set: HashSet<String> = current
        .clone()
        .unwrap_or_default()
        .into_iter()
        .chain(incoming)
        .collect();
    *current = Some(set.into_iter().collect());
}

fn parse_mcp_file(path: &Path) -> Result<McpFile, BrainError> {
    let content = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn build_provider(profile: &ModelProfile) -> Result<AnyProvider, BrainError> {
    let provider_name = profile.provider.to_lowercase();
    match provider_name.as_str() {
        "openai" => {
            let key = resolve_api_key(profile)?;
            let mut provider = OpenAIProvider::new(key);
            if let Some(url) = &profile.base_url {
                provider = provider.with_url(url.clone());
            }
            Ok(AnyProvider::OpenAI(Arc::new(provider)))
        }
        "anthropic" => {
            let key = resolve_api_key(profile)?;
            let mut provider = AnthropicProvider::new(key);
            if let Some(url) = &profile.base_url {
                provider = provider.with_url(url.clone());
            }
            Ok(AnyProvider::Anthropic(Arc::new(provider)))
        }
        "ollama" => {
            let mut provider = OllamaProvider::new();
            if let Some(url) = &profile.base_url {
                provider = provider.with_url(url.clone());
            }
            Ok(AnyProvider::Ollama(Arc::new(provider)))
        }
        "mock" => Ok(AnyProvider::Mock(MockProvider::new(vec![]))),
        other => Err(BrainError::Config(format!("unsupported provider: {other}"))),
    }
}

fn resolve_api_key(profile: &ModelProfile) -> Result<String, BrainError> {
    if let Some(key) = &profile.api_key {
        return Ok(key.clone());
    }
    if let Some(env_name) = &profile.api_key_env {
        return std::env::var(env_name)
            .map_err(|_| BrainError::Config(format!("missing API key env var: {env_name}")));
    }
    Err(BrainError::Config(
        "missing api_key/api_key_env for provider".to_string(),
    ))
}

#[derive(Clone)]
enum AnyProvider {
    OpenAI(Arc<OpenAIProvider>),
    Anthropic(Arc<AnthropicProvider>),
    Ollama(Arc<OllamaProvider>),
    Mock(MockProvider),
}

impl Provider for AnyProvider {
    fn complete(
        &self,
        request: ProviderRequest,
    ) -> impl Future<Output = Result<ProviderResponse, ProviderError>> + Send {
        match self {
            AnyProvider::OpenAI(provider) => Box::pin(provider.complete(request))
                as Pin<Box<dyn Future<Output = Result<ProviderResponse, ProviderError>> + Send>>,
            AnyProvider::Anthropic(provider) => Box::pin(provider.complete(request))
                as Pin<Box<dyn Future<Output = Result<ProviderResponse, ProviderError>> + Send>>,
            AnyProvider::Ollama(provider) => Box::pin(provider.complete(request))
                as Pin<Box<dyn Future<Output = Result<ProviderResponse, ProviderError>> + Send>>,
            AnyProvider::Mock(provider) => Box::pin(provider.complete(request))
                as Pin<Box<dyn Future<Output = Result<ProviderResponse, ProviderError>> + Send>>,
        }
    }
}

struct MockProvider {
    responses: Arc<Mutex<VecDeque<ProviderResponse>>>,
}

impl Clone for MockProvider {
    fn clone(&self) -> Self {
        Self {
            responses: Arc::clone(&self.responses),
        }
    }
}

impl MockProvider {
    fn new(responses: Vec<ProviderResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }
}

impl Provider for MockProvider {
    fn complete(
        &self,
        _request: ProviderRequest,
    ) -> impl Future<Output = Result<ProviderResponse, ProviderError>> + Send {
        let next = self
            .responses
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front());
        async move {
            next.ok_or_else(|| {
                ProviderError::InvalidResponse("mock provider exhausted".to_string())
            })
        }
    }
}

#[derive(Clone)]
struct SubagentConfig {
    provider: AnyProvider,
    profile: WorkerProfile,
}

struct LoopingWorkerTool {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
    provider: AnyProvider,
    profile: WorkerProfile,
    subagent: Option<SubagentConfig>,
    support_tools: Arc<Vec<Arc<dyn ToolDyn>>>,
    capture: Option<Arc<Mutex<Option<Value>>>>,
}

impl ToolDyn for LoopingWorkerTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }

    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let mut tools = ToolRegistry::new();
            for tool in self.support_tools.iter() {
                tools.register(Arc::clone(tool));
            }

            if let Some(subagent_cfg) = self.subagent.clone() {
                tools.register(Arc::new(SpawnSubagentTool::new(
                    subagent_cfg,
                    Arc::clone(&self.support_tools),
                )));
            }

            let allowed_tools: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();

            let state: Arc<dyn StateReader> = Arc::new(MemoryStore::new());

            let op = ReactOperator::new(
                self.provider.clone(),
                tools,
                Box::new(NoCompaction),
                HookRegistry::new(),
                state,
                ReactConfig {
                    system_prompt: format!(
                        "You are a worker tool named {}. Use tools to do work. Return strictly valid JSON as your final response.",
                        self.name
                    ),
                    default_model: self.profile.provider.model.clone(),
                    default_max_tokens: self.profile.max_tokens,
                    default_max_turns: self.profile.max_turns,
                },
            );

            let payload = serde_json::json!({
                "tool": self.name,
                "input": input,
                "required_json": true
            });

            let mut op_input =
                OperatorInput::new(Content::text(payload.to_string()), TriggerType::Task);
            let mut cfg = OperatorConfig::default();
            cfg.model = Some(self.profile.provider.model.clone());
            cfg.max_turns = Some(self.profile.max_turns);
            if let Some(max_cost) = self.profile.max_cost_usd {
                cfg.max_cost = Decimal::from_f64_retain(max_cost);
            }
            cfg.allowed_tools = Some(allowed_tools);
            op_input.config = Some(cfg);

            let output = op
                .execute(op_input)
                .await
                .map_err(|err| ToolError::ExecutionFailed(err.to_string()))?;

            let text = output.message.as_text().unwrap_or_default().to_string();
            let parsed = parse_json_payload(&text)
                .map_err(|err| ToolError::ExecutionFailed(format!("invalid worker JSON: {err}")))?;

            if let Some(capture) = &self.capture {
                if let Ok(mut slot) = capture.lock() {
                    *slot = Some(parsed.clone());
                }
            }

            Ok(parsed)
        })
    }
}

struct SpawnSubagentTool {
    cfg: SubagentConfig,
    support_tools: Arc<Vec<Arc<dyn ToolDyn>>>,
}

impl SpawnSubagentTool {
    fn new(cfg: SubagentConfig, support_tools: Arc<Vec<Arc<dyn ToolDyn>>>) -> Self {
        Self { cfg, support_tools }
    }
}

impl ToolDyn for SpawnSubagentTool {
    fn name(&self) -> &str {
        "spawn_subagent"
    }

    fn description(&self) -> &str {
        "Spawn a bounded sub-agent to do research and return JSON."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task": {"type": "string"},
                "system": {"type": "string"}
            },
            "required": ["task"]
        })
    }

    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let task = input
                .get("task")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: task".to_string()))?
                .to_string();
            let system_addendum = input
                .get("system")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let mut tools = ToolRegistry::new();
            for tool in self.support_tools.iter() {
                tools.register(Arc::clone(tool));
            }

            let allowed_tools: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();
            let state: Arc<dyn StateReader> = Arc::new(MemoryStore::new());

            let op = ReactOperator::new(
                self.cfg.provider.clone(),
                tools,
                Box::new(NoCompaction),
                HookRegistry::new(),
                state,
                ReactConfig {
                    system_prompt: "You are a research sub-agent. Use tools. Return strictly valid JSON as your final response.".to_string(),
                    default_model: self.cfg.profile.provider.model.clone(),
                    default_max_tokens: self.cfg.profile.max_tokens,
                    default_max_turns: self.cfg.profile.max_turns,
                },
            );

            let mut op_input = OperatorInput::new(Content::text(task), TriggerType::Task);
            let mut cfg = OperatorConfig::default();
            cfg.model = Some(self.cfg.profile.provider.model.clone());
            cfg.max_turns = Some(self.cfg.profile.max_turns);
            if let Some(max_cost) = self.cfg.profile.max_cost_usd {
                cfg.max_cost = Decimal::from_f64_retain(max_cost);
            }
            cfg.allowed_tools = Some(allowed_tools);
            cfg.system_addendum = system_addendum;
            op_input.config = Some(cfg);

            let output = op
                .execute(op_input)
                .await
                .map_err(|err| ToolError::ExecutionFailed(err.to_string()))?;

            let text = output.message.as_text().unwrap_or_default().to_string();
            parse_json_payload(&text)
                .map_err(|err| ToolError::ExecutionFailed(format!("invalid subagent JSON: {err}")))
        })
    }
}

struct WriteArtifactTool {
    root: PathBuf,
}

impl WriteArtifactTool {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl ToolDyn for WriteArtifactTool {
    fn name(&self) -> &str {
        "write_artifact"
    }

    fn description(&self) -> &str {
        "Write a human-readable artifact file under the configured artifact root."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "relative_path": {"type": "string"},
                "content": {"type": "string"}
            },
            "required": ["content"]
        })
    }

    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let content = input
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: content".to_string()))?
                .to_string();

            let relative = input
                .get("relative_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("note-{}.md", epoch_millis()));

            let rel_path = validate_relative_path(&relative)
                .map_err(|e| ToolError::InvalidInput(e.to_string()))?;
            let full = self.root.join(&rel_path);

            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            }
            std::fs::write(&full, content)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

            Ok(serde_json::json!({
                "relative_path": rel_path.to_string_lossy(),
                "absolute_path": full.to_string_lossy()
            }))
        })
    }
}

fn validate_relative_path(path: &str) -> Result<PathBuf, BrainError> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return Err(BrainError::Config(
            "relative_path must not be absolute".to_string(),
        ));
    }
    for component in candidate.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(BrainError::Config(
                "relative_path must not contain '..'".to_string(),
            ));
        }
    }
    Ok(candidate)
}

fn epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
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

    fn input_schema(&self) -> Value {
        self.inner.input_schema()
    }

    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        self.inner.call(input)
    }
}

fn parse_json_payload(text: &str) -> Result<Value, serde_json::Error> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        if start < end {
            return serde_json::from_str::<Value>(&trimmed[start..=end]);
        }
    }

    serde_json::from_str::<Value>(trimmed)
}

/// Testing helpers for offline integration tests.
pub mod testing {
    use super::*;

    /// Scenario inputs for offline integration testing.
    pub struct OfflineBrainScenario {
        pub user_message: String,
        pub controller_responses: Vec<ProviderResponse>,
        pub worker_responses: Vec<ProviderResponse>,
        pub mcp_path: PathBuf,
        pub fake_mcp_tools: Vec<Arc<dyn ToolDyn>>,
    }

    /// Run brain fully offline with mocked controller/worker providers.
    pub async fn run_offline_brain(
        scenario: OfflineBrainScenario,
    ) -> Result<BrainRunResult, BrainError> {
        let temp_state_root = scenario
            .mcp_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".brain")
            .join("state");
        let temp_artifact_root = scenario
            .mcp_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".brain")
            .join("artifacts");

        let config = BrainConfig {
            mcp_files: vec![scenario.mcp_path],
            state_dir: temp_state_root,
            artifact_dir: temp_artifact_root,
            ..BrainConfig::default()
        };

        let workers = WorkerProviderSet {
            sonnet_summarize: AnyProvider::Mock(MockProvider::new(scenario.worker_responses)),
            nano_extract: AnyProvider::Mock(MockProvider::new(vec![])),
            codex_generate_patch: AnyProvider::Mock(MockProvider::new(vec![])),
        };

        run_with_providers(
            AnyProvider::Mock(MockProvider::new(scenario.controller_responses)),
            workers,
            &config,
            RunRequest {
                user_message: scenario.user_message,
                session: Some("offline-test".to_string()),
                ..RunRequest::default()
            },
            McpMode::Injected(scenario.fake_mcp_tools),
            Arc::new(Mutex::new(None)),
        )
        .await
    }

    /// Create a simple text response from a mocked provider.
    pub fn text_response(text: &str) -> ProviderResponse {
        ProviderResponse {
            content: vec![ContentPart::Text {
                text: text.to_string(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 10,
                cache_read_tokens: None,
                cache_creation_tokens: None,
            },
            model: "mock".to_string(),
            cost: Some(Decimal::ZERO),
            truncated: None,
        }
    }

    /// Create a tool-use response from a mocked provider.
    pub fn tool_use_response(id: &str, name: &str, input: Value) -> ProviderResponse {
        ProviderResponse {
            content: vec![ContentPart::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input,
            }],
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 10,
                cache_read_tokens: None,
                cache_creation_tokens: None,
            },
            model: "mock".to_string(),
            cost: Some(Decimal::ZERO),
            truncated: None,
        }
    }

    /// Build a static fake tool for MCP tool injection tests.
    pub fn fake_tool(name: &str, description: &str, output: Value) -> Arc<dyn ToolDyn> {
        Arc::new(StaticTool {
            name: name.to_string(),
            description: description.to_string(),
            output,
        })
    }

    struct StaticTool {
        name: String,
        description: String,
        output: Value,
    }

    impl ToolDyn for StaticTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        fn input_schema(&self) -> Value {
            serde_json::json!({"type":"object"})
        }

        fn call(
            &self,
            _input: Value,
        ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
            let output = self.output.clone();
            Box::pin(async move { Ok(output) })
        }
    }
}
