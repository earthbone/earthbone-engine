//! Saved SQL queries over harness identities, based on integrate_build.py summaries.

use crate::{Error, Result};

/// Saved query names supported by every history backend.
pub const NAMES: &[&str] = &[
    "regressions",
    "flaky",
    "new-failures",
    "memory",
    "slowest-tests",
    "slowest-binaries",
    "rows",
    "play-path",
    "agent-cost",
    "earlyoom",
    "compile-errors",
    "summary",
];

/// Select SQL and bound text arguments. Commit arguments accept recorded prefixes.
pub fn saved(name: &str, args: &[String]) -> Result<(&'static str, Vec<String>)> {
    let commit = || -> Result<Vec<String>> {
        let value = args
            .first()
            .ok_or_else(|| Error::Input(format!("{name} requires a commit")))?;
        if value.is_empty() {
            return Err(Error::Input("empty commit".to_owned()));
        }
        Ok(vec![format!("{value}%")])
    };
    let query = match name {
        "regressions" => ("SELECT DISTINCT r.commit_id,t.package,t.binary,t.name,p.commit_id AS parent FROM tests t JOIN runs r ON r.id=t.run JOIN tests old ON old.package=t.package AND old.binary=t.binary AND old.name=t.name JOIN runs p ON p.id=old.run WHERE t.status IN ('fail','killed','timeout') AND old.status='pass' AND r.commit_id LIKE ?1 AND EXISTS (SELECT 1 FROM json_each(r.parents) WHERE p.commit_id LIKE json_each.value || '%' OR json_each.value LIKE p.commit_id || '%') ORDER BY t.package,t.binary,t.name", commit()?),
        "flaky" => ("SELECT r.commit_id,t.package,t.binary,t.name,SUM(CASE WHEN t.status='pass' THEN 1 ELSE 0 END) AS passes,SUM(CASE WHEN t.status='fail' THEN 1 ELSE 0 END) AS failures FROM tests t JOIN runs r ON r.id=t.run WHERE r.commit_id IS NOT NULL GROUP BY r.commit_id,t.package,t.binary,t.name HAVING passes>0 AND failures>0 ORDER BY r.commit_id,t.name", vec![]),
        "new-failures" => ("SELECT DISTINCT r.commit_id,t.package,t.binary,t.name FROM tests t JOIN runs r ON r.id=t.run WHERE t.status IN ('fail','killed','timeout') AND r.started>(SELECT MIN(started) FROM runs WHERE commit_id LIKE ?1) AND NOT EXISTS (SELECT 1 FROM tests old JOIN runs base ON base.id=old.run WHERE base.commit_id LIKE ?1 AND old.package=t.package AND old.binary=t.binary AND old.name=t.name AND old.status IN ('fail','killed','timeout')) ORDER BY r.commit_id,t.name", commit()?),
        "memory" => ("SELECT r.commit_id,x.binary,x.test,MAX(x.peak_rss_kib) AS peak_rss_kib FROM resources x JOIN runs r ON r.id=x.run GROUP BY r.commit_id,x.binary,x.test ORDER BY peak_rss_kib DESC", vec![]),
        "slowest-tests" => ("SELECT r.commit_id,t.package,t.binary,t.name,MAX(t.duration) AS seconds FROM tests t JOIN runs r ON r.id=t.run WHERE t.duration IS NOT NULL GROUP BY r.commit_id,t.package,t.binary,t.name ORDER BY seconds DESC LIMIT 100", vec![]),
        "slowest-binaries" => ("SELECT r.commit_id,x.binary,MAX(x.seconds) AS seconds FROM resources x JOIN runs r ON r.id=x.run WHERE x.seconds IS NOT NULL GROUP BY r.commit_id,x.binary ORDER BY seconds DESC LIMIT 100", vec![]),
        "rows" => ("SELECT t.backlog_row,t.package,t.binary,t.name,r.commit_id,t.status FROM tests t JOIN runs r ON r.id=t.run WHERE t.status IN ('fail','killed','timeout') ORDER BY t.backlog_row,t.package,t.name", vec![]),
        "play-path" => ("SELECT r.commit_id,c.stop,c.check_name,c.owner_row,c.status,COUNT(*) AS observations,MAX(c.elapsed) AS elapsed FROM play_checks c JOIN runs r ON r.id=c.run GROUP BY r.commit_id,c.stop,c.check_name,c.owner_row,c.status ORDER BY r.commit_id,elapsed,c.check_name", vec![]),
        "agent-cost" => ("SELECT rid,name,model,kind,MAX(minutes) AS minutes,SUM(CAST(json_extract(tokens,'$.input_tokens') AS INTEGER)) AS input_tokens,SUM(CAST(json_extract(tokens,'$.cached_input_tokens') AS INTEGER)) AS cached_input_tokens,SUM(CAST(json_extract(tokens,'$.output_tokens') AS INTEGER)) AS output_tokens,SUM(CAST(json_extract(tokens,'$.reasoning_output_tokens') AS INTEGER)) AS reasoning_output_tokens,SUM(capacity_stops) AS capacity_stops,MAX(retries) AS retries,MAX(reply_status) AS reply_status,MAX(commit_id) AS commit_id,MAX(fixed) AS fixed,MAX(left) AS left FROM agents GROUP BY rid,name,model,kind ORDER BY minutes DESC", vec![]),
        "earlyoom" => ("SELECT timestamp,process,command,peak_rss_kib,r.commit_id FROM resources x JOIN runs r ON r.id=x.run WHERE earlyoom=1 ORDER BY timestamp", vec![]),
        "compile-errors" => ("SELECT c.unit,c.severity,c.error_code,c.message,c.file,c.line,r.commit_id,COUNT(*) AS occurrences FROM compile c JOIN runs r ON r.id=c.run GROUP BY c.unit,c.severity,c.error_code,c.message,c.file,c.line,r.commit_id ORDER BY c.unit,occurrences DESC", vec![]),
        "summary" => ("SELECT l.text AS line FROM evidence_lines l JOIN runs r ON r.id=l.run WHERE r.commit_id LIKE ?1 AND r.log_path LIKE '%/summary.txt' ORDER BY r.log_path,l.ordinal", commit()?),
        _ => return Err(Error::Input(format!("unknown query {name}, choose {}", NAMES.join(", ")))),
    };
    Ok(query)
}
