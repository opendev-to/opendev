//! Skill creator controller for the TUI.
//!
//! Manages form state for creating skill files, including name,
//! description, prompt content, and invocability settings.

/// Specification for a skill, produced by validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSpec {
    pub name: String,
    pub description: String,
    pub content: String,
    pub is_user_invocable: bool,
}

/// Controller for the skill creation form.
pub struct SkillCreatorController {
    name: String,
    description: String,
    content: String,
    is_user_invocable: bool,
}

impl SkillCreatorController {
    /// Create a new skill creator controller with empty fields.
    pub fn new() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            content: String::new(),
            is_user_invocable: true,
        }
    }

    /// Set the skill name.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Get the current skill name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Set the skill description.
    pub fn set_description(&mut self, description: impl Into<String>) {
        self.description = description.into();
    }

    /// Get the current description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Set the skill content (prompt text).
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
    }

    /// Get the current content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Set whether this skill can be invoked directly by users.
    pub fn set_user_invocable(&mut self, invocable: bool) {
        self.is_user_invocable = invocable;
    }

    /// Whether this skill is user-invocable.
    pub fn is_user_invocable(&self) -> bool {
        self.is_user_invocable
    }

    /// Validate the current form state and produce a [`SkillSpec`].
    ///
    /// Returns an error string describing the first validation failure.
    pub fn validate(&self) -> Result<SkillSpec, String> {
        if self.name.trim().is_empty() {
            return Err("Skill name is required".into());
        }
        if self.description.trim().is_empty() {
            return Err("Skill description is required".into());
        }
        if self.content.trim().is_empty() {
            return Err("Skill content is required".into());
        }
        Ok(SkillSpec {
            name: self.name.trim().to_string(),
            description: self.description.trim().to_string(),
            content: self.content.clone(),
            is_user_invocable: self.is_user_invocable,
        })
    }

    /// Reset all fields to their default values.
    pub fn reset(&mut self) {
        self.name.clear();
        self.description.clear();
        self.content.clear();
        self.is_user_invocable = true;
    }
}

impl Default for SkillCreatorController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "skill_creator_tests.rs"]
mod tests;
