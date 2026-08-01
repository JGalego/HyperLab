//! Describing a tool to a model.

use serde::{Deserialize, Serialize};

/// A tool a model may ask for, described the way the Model Context Protocol
/// describes one.
///
/// HyperLab uses the same shape for the tools it *offers* (see the
/// `hyperlab-mcp` crate) and the tools it *passes on* to a model, so a tool
/// only ever has to be described once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    /// The name a model calls it by.
    pub name: String,
    /// What it does, written for a model to read. This is the whole user
    /// interface of a tool: if it is vague, the tool will be misused.
    pub description: String,
    /// A JSON Schema for the arguments.
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Describes a tool.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}
