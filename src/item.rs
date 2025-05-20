use std::{fmt, io, ops::Deref};

/// [`crate::Process`] stream output
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProcessItem {
    /// A stdout chunk printed by the process.
    Output(String),
    /// A stderr chunk printed by the process or internal error message
    Error(String),
    /// Indication that the process exit successful
    Exit(String),
}

impl Deref for ProcessItem {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Output(s) => s,
            Self::Error(s) => s,
            Self::Exit(s) => s,
        }
    }
}

impl fmt::Display for ProcessItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

impl fmt::Debug for ProcessItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Output(out) => write!(f, "[Output] {out}"),
            Self::Error(err) => write!(f, "[Error] {err}"),
            Self::Exit(code) => write!(f, "[Exit] {code}"),
        }
    }
}
impl From<(bool, io::Result<String>)> for ProcessItem {
    fn from((is_stdout, line): (bool, io::Result<String>)) -> Self {
        match line {
            Ok(line) if is_stdout => Self::Output(line),
            Ok(line) => Self::Error(line),
            Err(e) => Self::Error(e.to_string()),
        }
    }
}

impl ProcessItem {
    /// Returns `true` if the process item is [`Output`].
    ///
    /// [`Output`]: ProcessItem::Output
    #[must_use]
    pub const fn is_output(&self) -> bool {
        matches!(self, Self::Output(..))
    }

    /// Returns `true` if the process item is [`Error`].
    ///
    /// [`Error`]: ProcessItem::Error
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(..))
    }

    /// Returns `true` if the process item is [`Exit`].
    ///
    /// [`Exit`]: ProcessItem::Exit
    #[must_use]
    pub const fn is_exit(&self) -> bool {
        matches!(self, Self::Exit(..))
    }

    /// Returns Some(`true`) if the process item is [`Exit`] and returned 0
    ///
    /// [`Exit`]: ProcessItem::Exit
    #[must_use]
    pub fn is_success(&self) -> Option<bool> {
        self.as_exit().map(|s| s.trim() == "0")
    }

    /// Return exit code if [`ProcessItem`] is [`ProcessItem::Exit`]
    #[must_use]
    pub const fn as_exit(&self) -> Option<&String> {
        if let Self::Exit(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Return inner reference [`String`] value if [`ProcessItem`] is [`ProcessItem::Error`]
    #[must_use]
    pub const fn as_error(&self) -> Option<&String> {
        if let Self::Error(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Return inner reference [`String`] value if [`ProcessItem`] is [`ProcessItem::Output`]
    #[must_use]
    pub const fn as_output(&self) -> Option<&String> {
        if let Self::Output(v) = self {
            Some(v)
        } else {
            None
        }
    }
}
