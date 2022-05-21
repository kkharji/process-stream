use std::{fmt::Display, io};

#[derive(Debug)]
/// [`crate::Process`] stream output
pub enum ProcessItem {
    /// A stdout chunk printed by the process.
    Output(String),
    /// A stderr chunk printed by the process or internal error message
    Error(String),
    /// Indication that the process exit successful
    Exit(i32),
}

impl ProcessItem {
    /// Returns `true` if the process item is [`Output`].
    ///
    /// [`Output`]: ProcessItem::Output
    #[must_use]
    pub fn is_output(&self) -> bool {
        matches!(self, Self::Output(..))
    }

    /// Returns `true` if the process item is [`Error`].
    ///
    /// [`Error`]: ProcessItem::Error
    #[must_use]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(..))
    }

    /// Returns `true` if the process item is [`Exit`].
    ///
    /// [`Exit`]: ProcessItem::Exit
    #[must_use]
    pub fn is_exit(&self) -> bool {
        matches!(self, Self::Exit(..))
    }

    /// Return exit code if [`ProcessItem`] is [`ProcessItem::Exit`]
    pub fn as_exit(&self) -> Option<&i32> {
        if let Self::Exit(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Return inner reference [`String`] value if [`ProcessItem`] is [`ProcessItem::Error`]
    pub fn as_error(&self) -> Option<&String> {
        if let Self::Error(v) = self {
            Some(v)
        } else {
            None
        }
    }

    /// Return inner reference [`String`] value if [`ProcessItem`] is [`ProcessItem::Output`]
    pub fn as_output(&self) -> Option<&String> {
        if let Self::Output(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

impl From<(bool, io::Result<String>)> for ProcessItem {
    fn from(v: (bool, io::Result<String>)) -> Self {
        match v.1 {
            Ok(line) if v.0 => Self::Output(line),
            Ok(line) => Self::Error(line),
            Err(e) => Self::Error(e.to_string()),
        }
    }
}

impl Display for ProcessItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessItem::Output(line) => line.fmt(f),
            ProcessItem::Error(line) => line.fmt(f),
            ProcessItem::Exit(code) => write!(f, "[Exit] {code}"),
        }
    }
}
