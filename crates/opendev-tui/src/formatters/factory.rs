//! Formatter factory — dispatches tool names to the appropriate formatter.

use super::base::{FormattedOutput, ToolFormatter};
use super::bash_formatter::BashFormatter;
use super::directory_formatter::DirectoryFormatter;
use super::file_formatter::FileFormatter;
use super::generic_formatter::GenericFormatter;
use super::todo_formatter::TodoFormatter;
use super::tool_registry::{ResultFormat, lookup_tool};

/// Factory that selects the right formatter for a given tool name.
pub struct FormatterFactory;

impl FormatterFactory {
    /// Get the appropriate formatter for a tool name and format its output.
    pub fn format<'a>(tool_name: &str, output: &str) -> FormattedOutput<'a> {
        let formatter = Self::formatter_for(tool_name);
        formatter.format(tool_name, output)
    }

    /// Return the formatter that handles a given tool name.
    fn formatter_for(tool_name: &str) -> Box<dyn ToolFormatter> {
        match lookup_tool(tool_name).result_format {
            ResultFormat::Bash => Box::new(BashFormatter),
            ResultFormat::File => Box::new(FileFormatter),
            ResultFormat::Directory => Box::new(DirectoryFormatter),
            ResultFormat::Generic => Box::new(GenericFormatter),
            ResultFormat::Todo => Box::new(TodoFormatter),
        }
    }
}

#[cfg(test)]
#[path = "factory_tests.rs"]
mod tests;
