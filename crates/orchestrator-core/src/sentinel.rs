//! Worker sentinel parser.
//!
//! Ports the contract from
//! `thurbeen-skills/skills/orchestrate/scripts/parse-result.sh`:
//! the worker emits a final `===RESULT===` line followed by a single
//! line of JSON whose `status` field is required.

use serde::{Deserialize, Serialize};

const SENTINEL_LINE: &str = "===RESULT===";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Result {
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artifact: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pr_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bd_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Debug)]
pub enum Outcome {
    Found(Result),
    NotFound,
    Malformed(String),
}

pub fn parse(input: &str) -> Outcome {
    let lines: Vec<&str> = input.lines().collect();
    let Some(idx) = lines
        .iter()
        .rposition(|line| line.trim_end() == SENTINEL_LINE)
    else {
        return Outcome::NotFound;
    };

    let payload = lines
        .iter()
        .skip(idx + 1)
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim());

    let Some(payload) = payload else {
        return Outcome::Malformed("sentinel present but no JSON payload follows".to_owned());
    };

    match serde_json::from_str::<Result>(payload) {
        Ok(result) if result.status.is_empty() => {
            Outcome::Malformed("sentinel JSON missing required `status` field".to_owned())
        }
        Ok(result) => Outcome::Found(result),
        Err(err) => Outcome::Malformed(format!("sentinel JSON malformed: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_sentinel() {
        let input = r#"some output
===RESULT===
{"status":"ok","artifact":"abc","notes":"done"}
"#;
        let Outcome::Found(result) = parse(input) else {
            panic!("expected Found");
        };
        assert_eq!(result.status, "ok");
        assert_eq!(result.artifact, "abc");
        assert_eq!(result.notes, "done");
    }

    #[test]
    fn returns_not_found_when_sentinel_absent() {
        assert!(matches!(parse("just some output\n"), Outcome::NotFound));
    }

    #[test]
    fn returns_malformed_when_payload_missing() {
        let input = "===RESULT===\n";
        assert!(matches!(parse(input), Outcome::Malformed(_)));
    }

    #[test]
    fn returns_malformed_when_json_invalid() {
        let input = "===RESULT===\nnot json\n";
        assert!(matches!(parse(input), Outcome::Malformed(_)));
    }

    #[test]
    fn returns_malformed_when_status_missing() {
        let input = r#"===RESULT===
{"artifact":"x"}
"#;
        assert!(matches!(parse(input), Outcome::Malformed(_)));
    }

    #[test]
    fn last_sentinel_wins_when_multiple_present() {
        let input = r#"===RESULT===
{"status":"error","notes":"first"}
===RESULT===
{"status":"ok","notes":"second"}
"#;
        let Outcome::Found(result) = parse(input) else {
            panic!("expected Found");
        };
        assert_eq!(result.status, "ok");
        assert_eq!(result.notes, "second");
    }

    #[test]
    fn ignores_blank_lines_between_sentinel_and_payload() {
        let input = "===RESULT===\n\n   \n{\"status\":\"ok\"}\n";
        let Outcome::Found(result) = parse(input) else {
            panic!("expected Found");
        };
        assert_eq!(result.status, "ok");
    }

    #[test]
    fn tolerates_trailing_whitespace_on_sentinel_line() {
        let input = "===RESULT===   \n{\"status\":\"ok\"}\n";
        assert!(matches!(parse(input), Outcome::Found(_)));
    }
}
