//! JUnit XML, which most CI systems show as a test report.

use crane_core::runner::Report;

fn esc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&apos;".into(),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => " ".into(),
            c => c.to_string(),
        })
        .collect()
}

pub fn render(suite: &str, r: &Report) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(&format!(
        "<testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" time=\"{:.3}\">\n",
        esc(suite),
        r.outcomes.len(),
        r.failed(),
        r.elapsed_ms as f64 / 1000.0
    ));
    for o in &r.outcomes {
        let time = o.total_ms.unwrap_or(0) as f64 / 1000.0;
        out.push_str(&format!("  <testcase classname=\"{}\" name=\"{}\" time=\"{time:.3}\"", esc(suite), esc(&o.path)));
        if o.passed {
            out.push_str("/>\n");
            continue;
        }
        let mut lines: Vec<String> = o.error.iter().cloned().collect();
        lines.extend(o.checks.iter().filter(|c| !c.passed).map(|c| format!("{}{}", c.check, c.detail.as_deref().map(|d| format!(": {d}")).unwrap_or_default())));
        if lines.is_empty() {
            lines.push(format!("answered {}", o.status.map_or("nothing".into(), |s| s.to_string())));
        }
        out.push_str(&format!(">\n    <failure message=\"{}\">{}</failure>\n  </testcase>\n", esc(&lines[0]), esc(&lines.join("\n"))));
    }
    out.push_str("</testsuite>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crane_core::checks::CheckResult;
    use crane_core::runner::Outcome;

    #[test]
    fn writes_passes_and_failures() {
        let ok = Outcome { path: "a.http".into(), name: "A".into(), method: "GET".into(), status: Some(200), total_ms: Some(12), error: None, checks: vec![], saved: vec![], passed: true };
        let bad = Outcome {
            path: "b<&>.http".into(),
            checks: vec![CheckResult { check: "status 200".into(), passed: false, detail: Some("got 500".into()) }],
            status: Some(500),
            passed: false,
            ..ok.clone()
        };
        let xml = render("api", &Report { outcomes: vec![ok, bad], elapsed_ms: 1500 });
        assert!(xml.contains("<testsuite name=\"api\" tests=\"2\" failures=\"1\" time=\"1.500\">"));
        assert!(xml.contains("<testcase classname=\"api\" name=\"a.http\" time=\"0.012\"/>"));
        assert!(xml.contains("name=\"b&lt;&amp;&gt;.http\""));
        assert!(xml.contains("<failure message=\"status 200: got 500\">status 200: got 500</failure>"));
    }
}
