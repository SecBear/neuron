use brain::{BrainConfig, BrainError, RunRequest, run};
use std::io::Read;
use std::path::PathBuf;

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
        if first == "run" || first == "--help" || first == "-h" {
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

fn take_arg(flag: &str, remaining: &mut Vec<String>) -> Result<String, BrainError> {
    if remaining.is_empty() {
        return Err(BrainError::Config(format!("missing value for {flag}")));
    }
    Ok(remaining.remove(0))
}

fn print_usage() {
    println!(
        "brain run [--config brain.json] [--prompt TEXT] [--state-dir PATH] [--mcp PATH] [--allow-tool NAME]"
    );
}
