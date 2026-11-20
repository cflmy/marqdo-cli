//! Marqdo command line interface: the composition root.
//!
//! The CLI loads Markdown modules, runs the frontend, and either reports
//! diagnostics (`check`/`inspect`) or executes through the runtime and
//! capability host (`run`/`call`). It never holds a second interpreter.

mod args;
mod commands;
mod loader;
mod output;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "marqdo",
    version,
    about = "Markdown-native programming language (independent Rust interpreter)",
    long_about = "Marqdo executes Markdown documents whose AST structures carry program meaning. `check` and `inspect` never execute project code; `run` and `call` execute through the capability host."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialise a new Marqdo project.
    Init(InitArgs),
    /// Run the entry function (`main` by default) of a document.
    Run(RunArgs),
    /// Call a named function with typed arguments.
    Call(CallArgs),
    /// Statically check a document without executing it.
    Check(CheckArgs),
    /// Inspect AST, units, signatures, IR, references or platform support.
    Inspect(InspectArgs),
    /// Conservative formatting: normalises line endings and trailing newlines.
    Fmt(FmtArgs),
    /// Plugin artifact inspection (never loads plugin code).
    Plugin(PluginArgs),
    /// Add a dependency to marqdo.yaml.
    Add(AddArgs),
    /// Remove a dependency from marqdo.yaml.
    Remove(RemoveArgs),
    /// Generate a deterministic marqdo.lock for the current project.
    Lock(LockArgs),
    /// Verify that path dependencies match the lockfile.
    Sync(SyncArgs),
}

#[derive(Args)]
struct InitArgs {
    /// Directory to initialise (defaults to the current directory).
    dir: Option<PathBuf>,
}

#[derive(Args, Clone)]
struct SandboxArgs {
    /// Sandbox mode: pure, project or strict.
    #[arg(long, default_value = "project")]
    mode: String,
    /// Grant read access to a project-relative glob (repeatable).
    #[arg(long = "allow-read", value_name = "GLOB")]
    allow_read: Vec<String>,
    /// Grant write access to a project-relative glob (repeatable).
    #[arg(long = "allow-write", value_name = "GLOB")]
    allow_write: Vec<String>,
    /// Grant explicit foreign kernel execution.
    #[arg(long = "allow-foreign")]
    allow_foreign: bool,
    /// Grant a plugin name execution permission (repeatable).
    #[arg(long = "allow-plugin", value_name = "NAME")]
    allow_plugin: Vec<String>,
    /// Override the evaluation fuel budget.
    #[arg(long)]
    fuel: Option<u64>,
    /// Override the call stack depth limit.
    #[arg(long)]
    stack: Option<usize>,
    /// Override the program output byte limit.
    #[arg(long = "max-output")]
    max_output: Option<usize>,
    /// Wall-clock timeout in seconds.
    #[arg(long)]
    timeout: Option<u64>,
    /// Explicitly trusted plugin manifest/directory to load (repeatable).
    #[arg(long = "plugin", value_name = "PATH")]
    plugin: Vec<PathBuf>,
    /// Explicitly trust in-process native plugin loading (unsafe by nature).
    #[arg(long = "trust-native-plugin")]
    trust_native_plugin: bool,
}

impl SandboxArgs {
    fn settings(&self) -> commands::SandboxSettings {
        commands::SandboxSettings {
            mode: self.mode.clone(),
            allow_read: self.allow_read.clone(),
            allow_write: self.allow_write.clone(),
            allow_foreign: self.allow_foreign,
            allow_plugin: self.allow_plugin.clone(),
            fuel: self.fuel,
            stack: self.stack,
            max_output: self.max_output,
            timeout: self.timeout,
            plugins: self.plugin.clone(),
            trust_native_plugin: self.trust_native_plugin,
        }
    }
}

#[derive(Args)]
struct RunArgs {
    /// Entry document (defaults to index.mq.md).
    file: Option<PathBuf>,
    #[command(flatten)]
    sandbox: SandboxArgs,
    /// Machine readable JSON output; program stdout is embedded, never mixed.
    #[arg(long)]
    json: bool,
    /// Include the host effect trace.
    #[arg(long)]
    trace: bool,
}

#[derive(Args)]
struct CallArgs {
    /// Entry document.
    file: PathBuf,
    /// Function name (canonical or namespace-qualified).
    function: String,
    /// Named argument as `name=value` with a constant expression value.
    #[arg(long = "arg", value_name = "NAME=VALUE")]
    args: Vec<String>,
    #[command(flatten)]
    sandbox: SandboxArgs,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
    /// Include the host effect trace.
    #[arg(long)]
    trace: bool,
}

#[derive(Args)]
struct CheckArgs {
    /// Document to check (defaults to index.mq.md).
    file: Option<PathBuf>,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct InspectArgs {
    /// Document to inspect (defaults to index.mq.md).
    file: Option<PathBuf>,
    /// Print the canonical Markdown AST.
    #[arg(long)]
    ast: bool,
    /// Print heading units and function scopes.
    #[arg(long)]
    units: bool,
    /// Print function signatures.
    #[arg(long)]
    signature: bool,
    /// Print executable IR.
    #[arg(long)]
    ir: bool,
    /// Print static reference edges.
    #[arg(long)]
    refs: bool,
    /// Print sandbox/platform capability report.
    #[arg(long)]
    platform: bool,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct FmtArgs {
    /// Document to format (defaults to index.mq.md).
    file: Option<PathBuf>,
    /// Check whether formatting is needed without writing.
    #[arg(long = "check")]
    check: bool,
}

#[derive(Args)]
struct PluginArgs {
    #[command(subcommand)]
    command: PluginCommand,
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Inspect a plugin manifest/artifact without loading it.
    Inspect(PluginInspectArgs),
}

#[derive(Args)]
struct PluginInspectArgs {
    /// Plugin directory or plugin.yaml path.
    target: PathBuf,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct AddArgs {
    /// Dependency name used by front-matter imports (`@name`).
    name: String,
    /// Project-relative path dependency.
    #[arg(long)]
    path: Option<PathBuf>,
    /// Git URL dependency (requires --rev).
    #[arg(long)]
    git: Option<String>,
    /// Exact git revision for a git dependency.
    #[arg(long)]
    rev: Option<String>,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct RemoveArgs {
    /// Dependency name.
    name: String,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct LockArgs {
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct SyncArgs {
    /// Fail instead of silently updating when the lockfile is stale.
    #[arg(long)]
    locked: bool,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Init(args) => commands::init(args.dir.as_deref()),
        Command::Check(args) => commands::check(args.file.as_deref(), args.json),
        Command::Inspect(args) => {
            let flags = commands::InspectFlags {
                ast: args.ast,
                units: args.units,
                signature: args.signature,
                ir: args.ir,
                refs: args.refs,
                platform: args.platform,
            };
            commands::inspect(args.file.as_deref(), &flags, args.json)
        }
        Command::Run(args) => commands::execute(
            args.file.as_deref(),
            None,
            &[],
            &args.sandbox.settings(),
            args.json,
            args.trace,
        ),
        Command::Call(args) => match commands::parse_named_args(&args.args) {
            Ok(named) => commands::execute(
                Some(&args.file),
                Some(&args.function),
                &named,
                &args.sandbox.settings(),
                args.json,
                args.trace,
            ),
            Err(error) => {
                eprintln!("error: {error}");
                2
            }
        },
        Command::Fmt(args) => commands::fmt(args.file.as_deref(), args.check),
        Command::Plugin(args) => match args.command {
            PluginCommand::Inspect(args) => commands::plugin_inspect(&args.target, args.json),
        },
        Command::Add(args) => commands::add(
            &args.name,
            args.path.as_deref(),
            args.git.as_deref(),
            args.rev.as_deref(),
            args.json,
        ),
        Command::Remove(args) => commands::remove(&args.name, args.json),
        Command::Lock(args) => commands::lock(args.json),
        Command::Sync(args) => commands::sync(args.locked, args.json),
    };
    std::process::exit(code);
}
