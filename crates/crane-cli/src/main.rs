//! `crane-cli run <folder or file>`: runs a Crane collection and checks the
//! answers, for CI and the terminal. Exit code 0 when everything passed,
//! 1 when something failed, 2 when the run could not start.

mod args;
mod junit;
mod output;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use crane_core::SendOptions;
use crane_core::collection::Collection;
use crane_core::env::{EnvFile, Environment, SecretStore};
use crane_core::runner::{RunOptions, request_paths, run};

use args::{Args, Command};

/// Variables can also come from the environment, the usual way to hand
/// secrets to a CI job: `CRANE_VAR_token=...` gives `{{token}}`.
const VAR_PREFIX: &str = "CRANE_VAR_";
const WRITE_METHODS: [&str; 4] = ["POST", "PUT", "PATCH", "DELETE"];

fn main() -> ExitCode {
    // The container engine runs this program again as its helper.
    #[cfg(target_os = "linux")]
    crane_box::helper::run_if_helper();

    let args = match args::parse(std::env::args().skip(1)) {
        Ok(Command::Run(a)) => a,
        Ok(Command::Import(a)) => {
            return match import(&a) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("crane-cli: {e}");
                    ExitCode::from(2)
                }
            };
        }
        Ok(Command::Help) => {
            println!("{}", args::HELP);
            return ExitCode::SUCCESS;
        }
        Ok(Command::Version) => {
            println!("crane-cli {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("crane-cli: {e}\n\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("crane-cli: {e}");
            return ExitCode::from(2);
        }
    };
    match runtime.block_on(run_cli(args)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("crane-cli: {e}");
            ExitCode::from(2)
        }
    }
}

/// Where the app keeps its environments on this system.
fn default_env_file() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| b.join("dev.crane.app/environments.json"))
}

fn pick_env(a: &Args) -> Result<Option<Environment>, String> {
    let Some(wanted) = &a.env else { return Ok(None) };
    let file = a.envs.clone().or_else(default_env_file).ok_or("Could not find the environments file; pass it with --envs.")?;
    let envs = EnvFile::load(&file).map_err(|e| e.to_string())?;
    envs.environments
        .into_iter()
        .find(|e| e.id == *wanted || e.name.eq_ignore_ascii_case(wanted))
        .map(Some)
        .ok_or_else(|| format!("There is no environment called {wanted} in {}.", file.display()))
}

/// Secret variables of an environment come from the system keyring. In CI
/// there usually is none: pass them as CRANE_VAR_<name> instead, which are
/// looked at first.
fn store() -> Box<dyn SecretStore> {
    Box::new(crane_core::keyring_store::KeyringSecrets)
}

/// `crane-cli import`: writes the requests, adds the environments, and says
/// what did not come over.
fn import(a: &args::ImportArgs) -> Result<(), String> {
    let imported = crane_core::import::read(&a.source).map_err(|e| e.to_string())?;
    let col = Collection::open(&a.into).map_err(|e| e.to_string())?;
    let env_file = a.envs.clone().or_else(default_env_file).ok_or("Could not find the environments file; pass it with --envs.")?;
    let envs = EnvFile::load(&env_file).map_err(|e| e.to_string())?;
    let (next, report) = crane_core::import::apply(&imported, &col, &envs, store().as_ref()).map_err(|e| e.to_string())?;
    if !report.environments.is_empty() {
        next.save(&env_file).map_err(|e| e.to_string())?;
    }
    for line in output::import_summary(imported.from, &col.root().display().to_string(), &env_file.display().to_string(), &report) {
        println!("{line}");
    }
    Ok(())
}

/// The folder to open and the requests in it to run.
fn target(path: &Path) -> Result<(Collection, Option<String>), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.is_dir() {
        return Ok((Collection::open(path).map_err(|e| e.to_string())?, None));
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let name = path.file_name().ok_or("That is not a file.")?.to_string_lossy().into_owned();
    Ok((Collection::open(parent).map_err(|e| e.to_string())?, Some(name)))
}

async fn run_cli(a: Args) -> Result<bool, String> {
    let env = pick_env(&a)?;
    let (col, single) = target(&a.path)?;
    let paths = match single {
        Some(p) => vec![p],
        None => request_paths(&col.tree().map_err(|e| e.to_string())?),
    };
    if paths.is_empty() {
        return Err(format!("There are no .http files in {}.", a.path.display()));
    }
    if let Some(e) = env.as_ref().filter(|e| e.production)
        && !a.yes_production
    {
        let writes = paths.iter().filter_map(|p| col.read(p).ok()).filter(|r| WRITE_METHODS.contains(&r.request.method.as_str())).count();
        if writes > 0 {
            return Err(format!("{} is a production environment and {writes} request(s) change data. Add --yes-production if you mean it.", e.name));
        }
    }
    let mut vars: Vec<(String, String)> = std::env::vars().filter_map(|(k, v)| k.strip_prefix(VAR_PREFIX).map(|n| (n.to_string(), v))).collect();
    vars.extend(a.vars.clone());

    let policy = std::sync::Arc::new(crane_core::policy::Policy::load_system());
    if let Some(problem) = &policy.problem {
        eprintln!("crane-cli: {problem} The strictest rules apply.");
    }
    if a.stack && !policy.containers {
        return Err(policy.refusal("Running containers"));
    }
    let out = output::Printer::new();
    out.header(&col.name(), env.as_ref().map(|e| e.name.as_str()), paths.len());

    #[cfg(target_os = "linux")]
    let stack = if a.stack { Some(start_stack(&col, env.as_ref(), &vars, &out).await?) } else { None };
    #[cfg(not(target_os = "linux"))]
    if a.stack {
        return Err("--stack needs Linux for now.".into());
    }

    let store = store();
    let opts = RunOptions {
        env,
        store: store.as_ref(),
        vars,
        send: SendOptions { timeout: Duration::from_secs(a.timeout), policy, ..SendOptions::default() },
        bail: a.bail,
    };
    let report = run(&col, &paths, opts, |o| out.outcome(o)).await;
    out.summary(&report);

    #[cfg(target_os = "linux")]
    if let Some((engine, running)) = stack {
        out.note("Stopping the stack");
        let _ = crane_box::stack::run::down(&engine, running).await;
    }

    if let Some(file) = &a.junit {
        std::fs::write(file, junit::render(&col.name(), &report)).map_err(|e| format!("Could not write {}: {e}", file.display()))?;
    }
    Ok(report.failed() == 0)
}

#[cfg(target_os = "linux")]
async fn start_stack(
    col: &Collection,
    env: Option<&Environment>,
    vars: &[(String, String)],
    out: &output::Printer,
) -> Result<(crane_box::engine::Engine, crane_box::stack::run::RunningStack), String> {
    use crane_box::stack::{FILE, StackFile};
    let text = std::fs::read_to_string(col.root().join(FILE)).map_err(|_| format!("--stack needs a {FILE} in {}.", col.root().display()))?;
    let file = StackFile::parse(&text)?;
    let base = default_env_file().and_then(|p| p.parent().map(|d| d.join("containers"))).ok_or("No folder for container files.")?;
    let engine = crane_box::engine::Engine::new(base, crane_box::engine::Helper::this_program().map_err(|e| e.to_string())?);
    let store = store();
    let lookup = |name: &str| -> Option<String> {
        if let Some((_, v)) = vars.iter().find(|(k, _)| k == name) {
            return Some(v.clone());
        }
        let e = env?;
        let var = e.vars.iter().find(|v| v.name == name)?;
        if var.secret { store.get(&e.id, name).ok().flatten() } else { Some(var.value.clone()) }
    };
    let fill = |text: &str| crane_core::vars::substitute(text, &lookup).map_err(|missing| format!("No value for {}.", missing.join(", ")));
    out.note("Starting the stack from crane.toml");
    let running = crane_box::stack::run::up(&engine, &file, col.root(), &fill, &|ev| out.note(&format!("{}: {}", ev.service, ev.step)))
        .await
        .map_err(|e| e.to_string())?;
    Ok((engine, running))
}
