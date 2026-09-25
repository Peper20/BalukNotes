//! Диагностика компиляции в виде, удобном клиенту и терминалу.

use std::fmt;

use serde::{Deserialize, Serialize};
use typst::diag::{Severity, SourceDiagnostic};
use typst::syntax::VirtualRoot;
use typst::{World, WorldExt};

/// Шум, который приходит при каждой HTML-компиляции и ничего не сообщает.
const NOISE: &[&str] = &["html export is under active development"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Diagnostic {
    pub severity: DiagSeverity,
    pub message: String,
    /// Файл от корня хранилища (`/Сеть/SSH.typ`) или пакета (`@preview/cetz:0.4.2/…`).
    pub file: Option<String>,
    /// Строка и столбец с единицы.
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub hints: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum DiagSeverity {
    Error,
    Warning,
}

impl Diagnostic {
    /// Ошибка без места в файле (например, «заметка не найдена»).
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: DiagSeverity::Error,
            message: message.into(),
            file: None,
            line: None,
            column: None,
            hints: Vec::new(),
        }
    }

    /// Предупреждение приложения (не Typst) о месте в файле хранилища.
    pub(crate) fn warning_at(message: &str, file: String, line: usize, column: usize, hint: &str) -> Self {
        Self {
            severity: DiagSeverity::Warning,
            message: message.to_owned(),
            file: Some(file),
            line: Some(line),
            column: Some(column),
            hints: vec![hint.to_owned()],
        }
    }

    pub(crate) fn from_typst(world: &dyn World, diag: &SourceDiagnostic) -> Self {
        let severity = match diag.severity {
            Severity::Error => DiagSeverity::Error,
            Severity::Warning => DiagSeverity::Warning,
        };
        let mut out = Self {
            severity,
            message: diag.message.to_string(),
            file: None,
            line: None,
            column: None,
            hints: diag.hints.iter().map(|h| h.v.to_string()).collect(),
        };
        if let Some(id) = diag.span.id() {
            out.file = Some(match id.root() {
                VirtualRoot::Project => id.vpath().get_with_slash().to_string(),
                VirtualRoot::Package(spec) => format!("{spec}{}", id.vpath().get_with_slash()),
            });
            if let (Ok(source), Some(range)) = (world.source(id), world.range(diag.span))
                && let Some((line, col)) = source.lines().byte_to_line_column(range.start)
            {
                out.line = Some(line + 1);
                out.column = Some(col + 1);
            }
        }
        out
    }

    pub(crate) fn is_noise(diag: &SourceDiagnostic) -> bool {
        NOISE.iter().any(|n| diag.message.contains(n))
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.severity {
            DiagSeverity::Error => "ошибка",
            DiagSeverity::Warning => "предупреждение",
        };
        match (&self.file, self.line, self.column) {
            (Some(file), Some(line), Some(col)) => write!(f, "{file}:{line}:{col}: ")?,
            (Some(file), ..) => write!(f, "{file}: ")?,
            _ => {}
        }
        write!(f, "{kind}: {}", self.message)?;
        for hint in &self.hints {
            write!(f, "\n  подсказка: {hint}")?;
        }
        Ok(())
    }
}
