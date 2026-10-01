//! Findings and the exit status a gate produces.

use std::fmt::Write as _;

/// One violation, with the file and line that produced it.
#[derive(Debug)]
pub(crate) struct Finding {
    /// Path relative to the repository root.
    pub(crate) file: String,
    /// One-based line number.
    pub(crate) line: usize,
    /// What is wrong and what the rule requires.
    pub(crate) message: String,
}

/// What one gate observed.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    findings: Vec<Finding>,
    notes: Vec<String>,
}

impl Outcome {
    /// Records a violation.
    pub(crate) fn fail(&mut self, file: String, line: usize, message: String) {
        self.findings.push(Finding {
            file,
            line,
            message,
        });
    }

    /// Records a fact for the report, such as how much was inspected.
    pub(crate) fn note(&mut self, note: String) {
        self.notes.push(note);
    }

    /// Whether the gate found any violation.
    #[must_use]
    pub(crate) fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// Prints every note, then every finding, then the summary.
    #[must_use]
    pub(crate) fn report(&self, gate: &str) -> String {
        let mut text = String::new();
        for note in &self.notes {
            let _ = writeln!(text, "checked: {note}");
        }
        for finding in &self.findings {
            let _ = writeln!(
                text,
                "{}:{}: {}",
                finding.file, finding.line, finding.message
            );
        }
        let _ = writeln!(text, "{gate}: {} finding(s)", self.findings.len());
        text
    }
}
