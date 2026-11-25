//! Marqdo command line interface: the composition root.
//!
//! The CLI loads Markdown modules, runs the frontend, and either reports
//! diagnostics (`check`/`inspect`) or executes through the runtime and
//! capability host (`run`/`call`). It never holds a second interpreter.

mod analysis;
mod args;
mod commands;
mod document_refs;
mod extension_descriptors;
mod extension_selection;
mod loader;
mod observation;
mod output;
mod project_prepare;
mod provider_manifest;
mod stdlib;
mod tool;

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "marqdo",
    version,
    about = "Markdown-native programming language (independent Rust interpreter)",
    long_about = "Marqdo executes .mq.md documents: headings declare @exec/@knowledge modes, Strong/Emph/inline Code carry one statement each, and if/else/while/end/return/params decide control flow. `check` and `inspect` never execute project code; `run` and `call` execute through the capability host."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Output and capture options for run and call.
#[derive(Args, Clone)]
struct ObserveArgs {
    /// Select terminal text, a JSON observation snapshot or live JSON Lines.
    #[arg(long, value_enum, default_value = "text")]
    output: observation::OutputFormat,
    /// Atomically save a private observation snapshot of this run.
    #[arg(long, value_name = "PATH")]
    observation_file: Option<PathBuf>,
    /// Publish the value of one exactly named local variable (repeatable).
    #[arg(long = "observe-variable", value_name = "NAME")]
    observe_variable: Vec<String>,
    /// Publish the value of one exactly named parameter (repeatable).
    #[arg(long = "observe-parameter", value_name = "NAME")]
    observe_parameter: Vec<String>,
    /// Allow selected Text values to be published (second explicit consent).
    #[arg(long = "observe-text-values")]
    observe_text_values: bool,
    /// Maximum encoded source bytes for one observed value (0 = unlimited).
    #[arg(long = "observe-max-value-bytes", value_name = "BYTES")]
    observe_max_value_bytes: Option<usize>,
    /// Override the structural/value event budget (0 = unlimited).
    #[arg(long = "observe-max-events", value_name = "COUNT")]
    observe_max_events: Option<u64>,
    /// Override the approximate structural/value byte budget (0 = unlimited).
    #[arg(long = "observe-max-bytes", value_name = "BYTES")]
    observe_max_bytes: Option<u64>,
}

impl ObserveArgs {
    fn options(&self) -> observation::ObservationOptions {
        let mut options = observation::ObservationOptions {
            output: self.output,
            file: self.observation_file.clone(),
            variables: self.observe_variable.clone(),
            parameters: self.observe_parameter.clone(),
            allow_text_values: self.observe_text_values,
            ..observation::ObservationOptions::default()
        };
        if let Some(bytes) = self.observe_max_value_bytes {
            options.max_value_bytes = bytes;
        }
        if let Some(events) = self.observe_max_events {
            options.max_events = events;
        }
        if let Some(bytes) = self.observe_max_bytes {
            options.max_bytes = bytes;
        }
        options
    }
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
    /// Manage and dispatch generic external CLI tools.
    Tool(ToolArgs),
    /// Unknown top-level command: dispatch to a registered external tool.
    #[command(external_subcommand)]
    External(Vec<OsString>),
    /// Internal isolated plugin worker (not a user-facing command).
    #[command(name = "__plugin-worker", hide = true)]
    PluginWorker(PluginWorkerArgs),
}

#[derive(Args)]
struct PluginWorkerArgs {
    /// plugin.yaml path.
    manifest: PathBuf,
    /// Plugin package directory.
    base_dir: PathBuf,
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
    /// Grant network access to a host name or IP literal (repeatable, ADR-0034/0047).
    #[arg(long = "allow-net", value_name = "HOST")]
    allow_net: Vec<String>,
    /// Grant write access to a project-relative glob (repeatable).
    #[arg(long = "allow-write", value_name = "GLOB")]
    allow_write: Vec<String>,
    /// Grant read access to an environment variable (repeatable, ADR-0043).
    #[arg(long = "allow-env", value_name = "NAME")]
    allow_env: Vec<String>,
    /// Grant execution of an exact program path (repeatable, ADR-0044).
    #[arg(long = "allow-process", value_name = "PROGRAM")]
    allow_process: Vec<String>,
    /// Grant OS entropy reads (ADR-0046).
    #[arg(long = "allow-random")]
    allow_random: bool,
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
    /// Override the total committed addressable-memory budget in bytes.
    #[arg(long = "max-memory", value_name = "BYTES")]
    max_memory: Option<usize>,
    /// Override the per-region committed addressable-memory budget in bytes.
    #[arg(long = "max-region-memory", value_name = "BYTES")]
    max_region_memory: Option<usize>,
    /// Wall-clock timeout in seconds.
    #[arg(long)]
    timeout: Option<u64>,
    /// Explicitly trusted plugin manifest/directory to load (repeatable).
    #[arg(long = "plugin", value_name = "PATH")]
    plugin: Vec<PathBuf>,
    /// Explicitly selected resource provider v2 manifest (repeatable).
    ///
    /// Selecting a provider never authorizes it; `--allow-provider NAME` is
    /// required as well. `check`/`inspect` do not load providers.
    #[arg(long = "provider", value_name = "PATH")]
    provider: Vec<PathBuf>,
    /// Grant one selected resource provider by its exact manifest name
    /// (repeatable; wildcards are not supported).
    #[arg(long = "allow-provider", value_name = "NAME")]
    allow_provider: Vec<String>,
    /// Register an external foreign-kernel provider (repeatable).
    #[arg(long = "kernel", value_name = "PATH")]
    kernel: Vec<PathBuf>,
    /// Explicitly trust in-process native plugin loading (unsafe by nature).
    #[arg(long = "trust-native-plugin")]
    trust_native_plugin: bool,
}

impl SandboxArgs {
    fn settings(&self) -> commands::SandboxSettings {
        commands::SandboxSettings {
            mode: self.mode.clone(),
            allow_read: self.allow_read.clone(),
            allow_net: self.allow_net.clone(),
            allow_write: self.allow_write.clone(),
            allow_env: self.allow_env.clone(),
            allow_process: self.allow_process.clone(),
            allow_random: self.allow_random,
            allow_foreign: self.allow_foreign,
            allow_plugin: self.allow_plugin.clone(),
            fuel: self.fuel,
            stack: self.stack,
            max_output: self.max_output,
            max_memory: self.max_memory,
            max_region_memory: self.max_region_memory,
            timeout: self.timeout,
            plugins: self.plugin.clone(),
            providers: self.provider.clone(),
            allow_provider: self.allow_provider.clone(),
            trust_native_plugin: self.trust_native_plugin,
            kernels: self.kernel.clone(),
        }
    }
}

#[derive(Args)]
struct RunArgs {
    /// Entry document (defaults to index.mq.md).
    file: Option<PathBuf>,
    #[command(flatten)]
    sandbox: SandboxArgs,
    #[command(flatten)]
    observe: ObserveArgs,
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
    #[command(flatten)]
    observe: ObserveArgs,
}

#[derive(Args)]
struct CheckArgs {
    /// Document to check (defaults to index.mq.md).
    file: Option<PathBuf>,
    /// Also validate Markdown reference/link targets (`check --refs`).
    #[arg(long)]
    refs: bool,
    /// Declare a plugin manifest for static export-signature checking.
    #[arg(long = "plugin", value_name = "PATH")]
    plugin: Vec<PathBuf>,
    /// Write the current public Analysis JSON for the checked project.
    #[arg(long, value_name = "PATH")]
    emit: Option<PathBuf>,
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
    /// Print declared/loaded extension identities (never grants).
    #[arg(long)]
    extensions: bool,
    /// Explicitly selected plugin manifest for extension inspection (repeatable).
    #[arg(long = "plugin", value_name = "PATH")]
    plugin: Vec<PathBuf>,
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
struct ToolArgs {
    #[command(subcommand)]
    command: ToolCommand,
}

#[derive(Subcommand)]
enum ToolCommand {
    /// Install a tool from a local tool.json (or its directory).
    Install(ToolInstallArgs),
    /// List installed external tools.
    List(ToolListArgs),
    /// Remove an installed external tool.
    Remove(ToolRemoveArgs),
}

#[derive(Args)]
struct ToolInstallArgs {
    /// Path to tool.json or the tool package directory.
    manifest: PathBuf,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ToolListArgs {
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct ToolRemoveArgs {
    /// Tool command to remove.
    command: String,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct SyncArgs {
    /// Do not fetch pinned Git revisions; verify and materialize only from the local cache.
    #[arg(long)]
    locked: bool,
    /// Machine readable JSON output.
    #[arg(long)]
    json: bool,
}

fn main() {
    // The interpreter and the bundled pure stdlib recurse on the parse/execute
    // path. Windows gives the main thread 1 MiB by default (Linux/macOS use
    // 8 MiB), which overflowed on the regex self-test. Run the CLI on an
    // explicit 8 MiB thread on every platform so behavior does not depend on
    // the host's main-thread default.
    const STACK_BYTES: usize = 8 * 1024 * 1024;
    let code = match std::thread::Builder::new()
        .name("marqdo-main".to_string())
        .stack_size(STACK_BYTES)
        .spawn(dispatch)
    {
        Ok(handle) => handle.join().unwrap_or(101),
        Err(error) => {
            eprintln!("error: cannot start the marqdo main thread: {error}");
            1
        }
    };
    std::process::exit(code);
}

fn dispatch() -> i32 {
    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => commands::init(args.dir.as_deref()),
        Command::Check(args) => commands::check(
            args.file.as_deref(),
            args.json,
            args.refs,
            &args.plugin,
            args.emit.as_deref(),
        ),
        Command::Inspect(args) => {
            let flags = commands::InspectFlags {
                ast: args.ast,
                units: args.units,
                signature: args.signature,
                ir: args.ir,
                refs: args.refs,
                platform: args.platform,
                extensions: args.extensions,
            };
            commands::inspect(args.file.as_deref(), &flags, args.json, &args.plugin)
        }
        Command::Run(args) => commands::execute(
            args.file.as_deref(),
            None,
            Ok(Vec::new()),
            &args.sandbox.settings(),
            &args.observe.options(),
        ),
        Command::Call(args) => commands::execute(
            Some(&args.file),
            Some(&args.function),
            commands::parse_named_args(&args.args),
            &args.sandbox.settings(),
            &args.observe.options(),
        ),
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
        Command::Tool(args) => match args.command {
            ToolCommand::Install(args) => tool::install(&args.manifest, args.json),
            ToolCommand::List(args) => tool::list(args.json),
            ToolCommand::Remove(args) => tool::remove(&args.command, args.json),
        },
        Command::External(args) => tool::dispatch(&args),
        Command::PluginWorker(args) => {
            marqdo_plugin_abi::worker::serve(&args.manifest, &args.base_dir)
        }
    }
}

#[cfg(test)]
#[test]
fn top_level_commands_are_reviewed() {
    use clap::CommandFactory;

    let cli = Cli::command();
    let mut names: Vec<_> = cli
        .get_subcommands()
        .map(|command| command.get_name())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "__plugin-worker",
            "add",
            "call",
            "check",
            "fmt",
            "init",
            "inspect",
            "lock",
            "plugin",
            "remove",
            "run",
            "sync",
            "tool"
        ],
        "review changes under ADR-0064"
    );
}
