//! Command line arguments, read by hand to keep the CLI small.

use std::path::PathBuf;

pub const USAGE: &str = "Usage: crane-cli run <folder or .http file> [--env NAME] [--var NAME=VALUE]... [--junit FILE] [--bail] [--stack]";

pub const HELP: &str = "crane-cli: runs Crane collections and checks the answers.

Usage:
  crane-cli run <folder or .http file> [options]

Options:
  --env NAME           Use this environment's variables (by name or id).
  --envs FILE          Read environments from FILE instead of the app's.
                       Environment files never hold secret values, so a
                       CI environment can be kept in the repository.
  --var NAME=VALUE     Set a variable for this run (can repeat). Variables
                       also come from CRANE_VAR_<NAME> environment
                       variables, the way to pass CI secrets. Both count
                       as secrets: they are hidden in the output.
  --junit FILE         Also write a JUnit XML report, for CI test views.
  --bail               Stop at the first failure.
  --stack              Start the services in crane.toml first, and stop
                       them afterwards (Linux).
  --timeout SECONDS    How long one request may take (default 30).
  --yes-production     Allow requests that change data in a production
                       environment.
  -h, --help           Show this help.
  -V, --version        Show the version.

Secret variables of the environment are read from the system keyring.

Exit codes: 0 everything passed, 1 something failed, 2 the run could not start.";

const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub path: PathBuf,
    pub env: Option<String>,
    pub envs: Option<PathBuf>,
    pub vars: Vec<(String, String)>,
    pub junit: Option<PathBuf>,
    pub bail: bool,
    pub stack: bool,
    pub timeout: u64,
    pub yes_production: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run(Args),
    Help,
    Version,
}

pub fn parse(mut it: impl Iterator<Item = String>) -> Result<Command, String> {
    match it.next().as_deref() {
        None | Some("-h" | "--help" | "help") => return Ok(Command::Help),
        Some("-V" | "--version") => return Ok(Command::Version),
        Some("run") => {}
        Some(other) => return Err(format!("unknown command \"{other}\"")),
    }
    let mut path = None;
    let mut a = Args {
        path: PathBuf::new(),
        env: None,
        envs: None,
        vars: vec![],
        junit: None,
        bail: false,
        stack: false,
        timeout: DEFAULT_TIMEOUT_SECS,
        yes_production: false,
    };
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| it.next().ok_or_else(|| format!("{flag} needs a value"));
        match arg.as_str() {
            "--env" => a.env = Some(value("--env")?),
            "--envs" => a.envs = Some(value("--envs")?.into()),
            "--junit" => a.junit = Some(value("--junit")?.into()),
            "--var" => {
                let v = value("--var")?;
                let (n, val) = v.split_once('=').ok_or_else(|| format!("--var wants NAME=VALUE, not \"{v}\""))?;
                if !crane_core::vars::is_var_name(n) {
                    return Err(format!("\"{n}\" is not a usable variable name"));
                }
                a.vars.push((n.to_string(), val.to_string()));
            }
            "--timeout" => a.timeout = value("--timeout")?.parse().map_err(|_| "--timeout wants a number of seconds".to_string())?,
            "--bail" => a.bail = true,
            "--stack" => a.stack = true,
            "--yes-production" => a.yes_production = true,
            "-h" | "--help" => return Ok(Command::Help),
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            p if path.is_none() => path = Some(PathBuf::from(p)),
            extra => return Err(format!("only one folder or file, got also \"{extra}\"")),
        }
    }
    a.path = path.ok_or("run needs a folder or .http file")?;
    Ok(Command::Run(a))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Command, String> {
        parse(s.split_whitespace().map(String::from))
    }

    #[test]
    fn reads_a_full_run() {
        let Command::Run(a) = p("run api-tests --env Local --var token=abc --var base=x --junit out.xml --bail --stack --timeout 5").unwrap() else { panic!() };
        assert_eq!(a.path, PathBuf::from("api-tests"));
        assert_eq!(a.env.as_deref(), Some("Local"));
        assert_eq!(a.vars, [("token".into(), "abc".into()), ("base".into(), "x".into())]);
        assert_eq!(a.junit, Some(PathBuf::from("out.xml")));
        assert!(a.bail && a.stack && !a.yes_production);
        assert_eq!(a.timeout, 5);
    }

    #[test]
    fn helps_and_explains() {
        assert_eq!(p("").unwrap(), Command::Help);
        assert_eq!(p("--version").unwrap(), Command::Version);
        assert!(p("run").unwrap_err().contains("needs a folder"));
        assert!(p("run x --var nope").unwrap_err().contains("NAME=VALUE"));
        assert!(p("run x --var 2x=1").unwrap_err().contains("not a usable"));
        assert!(p("run x --wat").unwrap_err().contains("unknown option"));
        assert!(p("run x y").unwrap_err().contains("only one"));
        assert!(p("walk x").unwrap_err().contains("unknown command"));
    }
}
