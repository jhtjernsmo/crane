//! What the CLI prints: one line per request, failed checks under it, a
//! summary at the end. Colours only when writing to a terminal.

use std::io::IsTerminal;

use crane_core::runner::{Outcome, Report};

pub struct Printer {
    color: bool,
}

const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

impl Printer {
    pub fn new() -> Self {
        Self { color: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() }
    }

    fn paint(&self, color: &str, text: &str) -> String {
        if self.color { format!("{color}{text}{RESET}") } else { text.to_string() }
    }

    pub fn header(&self, collection: &str, env: Option<&str>, count: usize) {
        let env = env.map(|e| format!(" · {e}")).unwrap_or_default();
        println!("{}", self.paint(DIM, &format!("Crane · {collection}{env} · {count} request(s)")));
    }

    pub fn note(&self, text: &str) {
        println!("{}", self.paint(DIM, &format!("  {text}")));
    }

    pub fn outcome(&self, o: &Outcome) {
        let mark = if o.passed { self.paint(GREEN, "PASS") } else { self.paint(RED, "FAIL") };
        let status = o.status.map(|s| s.to_string()).unwrap_or_else(|| "---".into());
        let time = o.total_ms.map(|t| format!("{t} ms")).unwrap_or_default();
        println!("{mark}  {:<7}{:<4} {:<40} {}", o.method, status, o.path, self.paint(DIM, &time));
        if let Some(e) = &o.error {
            println!("        {}", self.paint(RED, e));
        }
        for c in o.checks.iter().filter(|c| !c.passed) {
            let detail = c.detail.as_deref().map(|d| format!(": {d}")).unwrap_or_default();
            println!("        {} {}{detail}", self.paint(RED, "x"), c.check);
        }
    }

    pub fn summary(&self, r: &Report) {
        let line = format!("{} passed, {} failed, {:.1} s", r.passed(), r.failed(), r.elapsed_ms as f64 / 1000.0);
        println!("{}", if r.failed() == 0 { self.paint(GREEN, &line) } else { self.paint(RED, &line) });
    }
}
