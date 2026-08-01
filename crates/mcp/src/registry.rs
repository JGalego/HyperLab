//! Looking tools up and calling them.

use hyperlab_ai::ToolDefinition;
use hyperlab_runtime::Runtime;
use serde_json::Value as Json;

use crate::{
    error::{ToolError, ToolResult},
    tools::{TOOLS, Tool},
};

/// The set of tools HyperLab offers.
///
/// The registry is the whole surface an assistant has: if something is not
/// here, an assistant cannot do it. Widening what an assistant may do is
/// therefore a visible, reviewable change to one list.
#[derive(Debug, Default, Clone, Copy)]
pub struct ToolRegistry;

impl ToolRegistry {
    /// The registry of built-in tools.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Every tool, described for a model.
    #[must_use]
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        TOOLS.iter().map(Tool::definition).collect()
    }

    /// The names of every tool.
    pub fn names(&self) -> impl Iterator<Item = &'static str> {
        TOOLS.iter().map(|tool| tool.name)
    }

    /// Looks a tool up by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&'static Tool> {
        TOOLS.iter().find(|tool| tool.name == name)
    }

    /// Calls a tool.
    ///
    /// Every change a tool makes goes through the runtime's command bus, so
    /// anything done here appears in the undo history and in the UI exactly
    /// as if a person had done it.
    ///
    /// # Errors
    ///
    /// Returns [`ToolError::UnknownTool`] for a name that is not offered, and
    /// whatever the tool itself failed with otherwise.
    pub fn call(&self, runtime: &mut Runtime, name: &str, arguments: &Json) -> ToolResult<Json> {
        let tool = self
            .get(name)
            .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;
        (tool.run)(runtime, arguments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperlab_stack::Stack;
    use serde_json::json;

    #[test]
    fn every_tool_is_described_for_a_model() {
        let registry = ToolRegistry::new();
        for definition in registry.definitions() {
            assert!(!definition.description.is_empty(), "{}", definition.name);
            assert_eq!(
                definition.input_schema["type"], "object",
                "{} must take an object",
                definition.name
            );
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let registry = ToolRegistry::new();
        let mut names: Vec<&str> = registry.names().collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total);
    }

    #[test]
    fn an_unknown_tool_is_refused_by_name() {
        let mut runtime = Runtime::new(Stack::new("Test"));
        let error = ToolRegistry::new()
            .call(&mut runtime, "no_such_tool", &json!({}))
            .unwrap_err();
        assert_eq!(error, ToolError::UnknownTool("no_such_tool".into()));
    }
}
