//! Shared plugin protocol types for meta subprocess plugins.
//!
//! This crate defines the communication protocol between the meta CLI host
//! and its subprocess plugins (meta-git, meta-project, meta-rust, etc.).
//!
//! The protocol works as follows:
//! 1. Host discovers plugins via `--meta-plugin-info` (plugin responds with `PluginInfo` JSON)
//! 2. Host invokes plugins via `--meta-plugin-exec` (sends `PluginRequest` JSON on stdin)
//! 3. Plugin responds with either a `PlanResponse` JSON (commands to execute) or direct output

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;

// ============================================================================
// Plugin Discovery Types
// ============================================================================

/// Metadata about a plugin, returned in response to `--meta-plugin-info`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub commands: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub help: Option<PluginHelp>,
}

/// Help information for a plugin's commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHelp {
    /// Usage string (e.g., "meta git <command> [args...]")
    pub usage: String,
    /// Command descriptions (command name -> description)
    #[serde(default)]
    pub commands: HashMap<String, String>,
    /// Example usage strings
    #[serde(default)]
    pub examples: Vec<String>,
    /// Additional note (e.g., how to run raw commands)
    #[serde(default)]
    pub note: Option<String>,
}

// ============================================================================
// Host-to-Plugin Communication
// ============================================================================

/// A request from the meta CLI host to a plugin, sent as JSON on stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub options: PluginRequestOptions,
}

/// Options passed from the host to the plugin as part of the request.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PluginRequestOptions {
    #[serde(default)]
    pub json_output: bool,
    #[serde(default)]
    pub verbose: bool,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub silent: bool,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub include_filters: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_filters: Option<Vec<String>>,
}

// ============================================================================
// Plugin-to-Host Response
// ============================================================================

/// An execution plan returned by a plugin, containing commands for the host to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub commands: Vec<PlannedCommand>,
    /// Whether to run commands in parallel (overrides CLI --parallel if set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel: Option<bool>,
}

/// A single command to be executed by the host via loop_lib.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedCommand {
    /// Directory to execute in (relative to meta root or absolute)
    pub dir: String,
    /// Command to execute
    pub cmd: String,
}

/// Wrapper for the execution plan response (the JSON envelope plugins emit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanResponse {
    pub plan: ExecutionPlan,
}

// ============================================================================
// Command Result
// ============================================================================

/// The result of a plugin command execution.
pub enum CommandResult {
    /// A plan of commands to execute via loop_lib
    Plan(Vec<PlannedCommand>, Option<bool>),
    /// A message to display (no commands to execute)
    Message(String),
    /// An error occurred
    Error(String),
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Serialize and print an execution plan to stdout.
pub fn output_execution_plan(commands: Vec<PlannedCommand>, parallel: Option<bool>) {
    let response = PlanResponse {
        plan: ExecutionPlan { commands, parallel },
    };
    println!("{}", serde_json::to_string(&response).unwrap());
}

/// Read and parse a `PluginRequest` from stdin.
pub fn read_request_from_stdin() -> anyhow::Result<PluginRequest> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let request: PluginRequest = serde_json::from_str(&input)?;
    Ok(request)
}

// ============================================================================
// Plugin Harness
// ============================================================================

/// Definition of a plugin, used by `run_plugin()` to eliminate main.rs boilerplate.
pub struct PluginDefinition {
    pub info: PluginInfo,
    /// The execute function: receives the parsed request and returns a CommandResult.
    pub execute: fn(PluginRequest) -> CommandResult,
}

/// Run a plugin's main loop. Handles `--meta-plugin-info` and `--meta-plugin-exec` flags.
///
/// This replaces the boilerplate main() function in each plugin binary.
/// Plugins only need to define their `PluginInfo` and an execute function.
pub fn run_plugin(plugin: PluginDefinition) {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("This binary is a meta plugin. Use via: meta {}", plugin.info.name);
        std::process::exit(1);
    }

    match args[1].as_str() {
        "--meta-plugin-info" => {
            let json = serde_json::to_string_pretty(&plugin.info).unwrap();
            println!("{}", json);
        }
        "--meta-plugin-exec" => {
            let request = match read_request_from_stdin() {
                Ok(req) => req,
                Err(e) => {
                    eprintln!("Failed to parse plugin request: {e}");
                    std::process::exit(1);
                }
            };

            match (plugin.execute)(request) {
                CommandResult::Plan(commands, parallel) => {
                    output_execution_plan(commands, parallel);
                }
                CommandResult::Message(msg) => {
                    if !msg.is_empty() {
                        println!("{}", msg);
                    }
                }
                CommandResult::Error(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "--help" | "-h" => {
            if let Some(help) = &plugin.info.help {
                println!("{}", help.usage);
                println!();
                if !help.commands.is_empty() {
                    println!("Commands:");
                    for (cmd, desc) in &help.commands {
                        println!("  {:<20} {}", cmd, desc);
                    }
                    println!();
                }
                if !help.examples.is_empty() {
                    println!("Examples:");
                    for ex in &help.examples {
                        println!("  {}", ex);
                    }
                    println!();
                }
                if let Some(note) = &help.note {
                    println!("{}", note);
                }
            } else {
                println!("meta {} v{}", plugin.info.name, plugin.info.version);
                if let Some(desc) = &plugin.info.description {
                    println!("{}", desc);
                }
            }
        }
        _ => {
            eprintln!("Unknown flag: {}. This binary is a meta plugin.", args[1]);
            eprintln!("Use via: meta {}", plugin.info.name);
            std::process::exit(1);
        }
    }
}
