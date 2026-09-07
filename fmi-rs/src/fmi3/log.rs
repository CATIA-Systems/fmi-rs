use std::{
    cell::RefCell,
    io::{IsTerminal, Write},
    path::Path,
};

use colored::Colorize;

use crate::fmi3::types::fmi3Status;

pub trait Logger {
    fn log_call(&self, status: fmi3Status, message: &str);
    fn log_message(&self, status: fmi3Status, category: &str, message: &str);
}

pub struct DefaultLogger {
    pub stream: Box<RefCell<dyn Write>>,
    pub is_terminal: bool,
}

impl DefaultLogger {
    pub fn new<S>(stream: S) -> Self
    where
        S: Write + IsTerminal + 'static,
    {
        let is_terminal = stream.is_terminal();
        DefaultLogger {
            stream: Box::new(RefCell::new(stream)),
            is_terminal,
        }
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let file = std::fs::File::create(path)?;
        Ok(Self::new(file))
    }
}

impl Default for DefaultLogger {
    fn default() -> Self {
        DefaultLogger::new(std::io::stderr())
    }
}

impl Logger for DefaultLogger {
    fn log_call(&self, _status: fmi3Status, message: &str) {
        let prefix = if self.is_terminal {
            "[FMI]".bright_black()
        } else {
            "[FMI]".normal()
        };
        writeln!(self.stream.borrow_mut(), "{prefix} {message}")
            .unwrap_or_else(|e| eprintln!("Failed to write log message: {e}"));
    }

    fn log_message(&self, status: fmi3Status, category: &str, message: &str) {
        let message = message.trim_end();

        if self.is_terminal {
            let prefix = match status {
                fmi3Status::Ok => "[INFO]".bright_blue(),
                fmi3Status::Warning => "[WARNING]".yellow(),
                fmi3Status::Error => "[ERROR]".bright_red(),
                fmi3Status::Discard => "[DISCARD]".bright_red(),
                fmi3Status::Fatal => "[FATAL]".bright_red(),
            };
            writeln!(self.stream.borrow_mut(), "{prefix} [{category}] {message}")
                .unwrap_or_else(|e| eprintln!("Failed to write log message: {e}"));
        } else {
            let prefix = match status {
                fmi3Status::Ok => "[INFO]",
                fmi3Status::Warning => "[WARNING]",
                fmi3Status::Error => "[ERROR]",
                fmi3Status::Discard => "[DISCARD]",
                fmi3Status::Fatal => "[FATAL]",
            };
            writeln!(self.stream.borrow_mut(), "{prefix} [{category}] {message}")
                .unwrap_or_else(|e| eprintln!("Failed to write log message: {e}"));
        };
    }
}
