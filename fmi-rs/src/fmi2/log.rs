use std::{
    cell::RefCell,
    io::{IsTerminal, Write},
    path::Path,
};

use colored::Colorize;

use crate::fmi2::types::fmi2Status;

pub trait Logger {
    fn log_call(&self, status: fmi2Status, message: &str);
    fn log_message(&self, status: fmi2Status, category: &str, message: &str);
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
    fn log_call(&self, _status: fmi2Status, message: &str) {
        let prefix = if self.is_terminal {
            "[FMI]".bright_black()
        } else {
            "[FMI]".normal()
        };
        writeln!(self.stream.borrow_mut(), "{prefix} {message}")
            .unwrap_or_else(|e| eprintln!("Failed to write log message: {e}"));
    }

    fn log_message(&self, status: fmi2Status, category: &str, message: &str) {
        let message = message.trim_end();

        if self.is_terminal {
            let prefix = match status {
                fmi2Status::fmi2OK => "[INFO]".bright_blue(),
                fmi2Status::fmi2Warning => "[WARNING]".yellow(),
                fmi2Status::fmi2Error => "[ERROR]".bright_red(),
                fmi2Status::fmi2Discard => "[DISCARD]".bright_red(),
                fmi2Status::fmi2Fatal => "[FATAL]".bright_red(),
                fmi2Status::fmi2Pending => "[PENDING]".bright_red(),
            };
            writeln!(self.stream.borrow_mut(), "{prefix} [{category}] {message}")
                .unwrap_or_else(|e| eprintln!("Failed to write log message: {e}"));
        } else {
            let prefix = match status {
                fmi2Status::fmi2OK => "[INFO]",
                fmi2Status::fmi2Warning => "[WARNING]",
                fmi2Status::fmi2Error => "[ERROR]",
                fmi2Status::fmi2Discard => "[DISCARD]",
                fmi2Status::fmi2Fatal => "[FATAL]",
                fmi2Status::fmi2Pending => "[PENDING]",
            };
            writeln!(self.stream.borrow_mut(), "{prefix} [{category}] {message}")
                .unwrap_or_else(|e| eprintln!("Failed to write log message: {e}"));
        };
    }
}
