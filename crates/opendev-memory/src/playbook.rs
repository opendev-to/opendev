//! ACE Playbook: structured context store for strategies and insights.
//!
//! Mirrors `opendev/core/context_engineering/memory/playbook.py`.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::delta::{DeltaBatch, DeltaOperation, DeltaOperationType};

/// Single playbook entry storing a strategy or insight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bullet {
    pub id: String,
    pub section: String,
    pub content: String,
    #[serde(default)]
    pub helpful: i64,
    #[serde(default)]
    pub harmful: i64,
    #[serde(default)]
    pub neutral: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Bullet {
    /// Apply metadata updates to counters.
    pub fn apply_metadata(&mut self, metadata: &HashMap<String, i64>) {
        if let Some(&v) = metadata.get("helpful") {
            self.helpful = v;
        }
        if let Some(&v) = metadata.get("harmful") {
            self.harmful = v;
        }
        if let Some(&v) = metadata.get("neutral") {
            self.neutral = v;
        }
    }

    /// Increment a counter (helpful/harmful/neutral).
    pub fn tag(&mut self, tag: &str, increment: i64) -> Result<(), String> {
        match tag {
            "helpful" => self.helpful += increment,
            "harmful" => self.harmful += increment,
            "neutral" => self.neutral += increment,
            _ => return Err(format!("Unsupported tag: {tag}")),
        }
        self.updated_at = Utc::now().to_rfc3339();
        Ok(())
    }
}

/// Structured context store for accumulated strategies and insights.
///
/// The Playbook replaces traditional message history with a curated collection
/// of strategy entries (bullets) that evolve based on execution feedback.
#[derive(Debug, Clone)]
pub struct Playbook {
    bullets: HashMap<String, Bullet>,
    sections: HashMap<String, Vec<String>>,
    next_id: u64,
}

impl Playbook {
    /// Create a new empty playbook.
    pub fn new() -> Self {
        Self {
            bullets: HashMap::new(),
            sections: HashMap::new(),
            next_id: 0,
        }
    }

    /// Add a new bullet to the playbook.
    pub fn add_bullet(
        &mut self,
        section: &str,
        content: &str,
        bullet_id: Option<&str>,
        metadata: Option<&HashMap<String, i64>>,
    ) -> &Bullet {
        let id = bullet_id
            .map(String::from)
            .unwrap_or_else(|| self.generate_id(section));
        let now = Utc::now().to_rfc3339();
        let mut bullet = Bullet {
            id: id.clone(),
            section: section.to_string(),
            content: content.to_string(),
            helpful: 0,
            harmful: 0,
            neutral: 0,
            created_at: now.clone(),
            updated_at: now,
        };
        if let Some(meta) = metadata {
            bullet.apply_metadata(meta);
        }
        self.bullets.insert(id.clone(), bullet);
        self.sections
            .entry(section.to_string())
            .or_default()
            .push(id.clone());
        self.bullets.get(&id).unwrap()
    }

    /// Update an existing bullet.
    pub fn update_bullet(
        &mut self,
        bullet_id: &str,
        content: Option<&str>,
        metadata: Option<&HashMap<String, i64>>,
    ) -> Option<&Bullet> {
        let bullet = self.bullets.get_mut(bullet_id)?;
        if let Some(c) = content {
            bullet.content = c.to_string();
        }
        if let Some(meta) = metadata {
            bullet.apply_metadata(meta);
        }
        bullet.updated_at = Utc::now().to_rfc3339();
        self.bullets.get(bullet_id)
    }

    /// Tag a bullet to update its counters.
    pub fn tag_bullet(&mut self, bullet_id: &str, tag: &str, increment: i64) -> Option<&Bullet> {
        let bullet = self.bullets.get_mut(bullet_id)?;
        let _ = bullet.tag(tag, increment);
        self.bullets.get(bullet_id)
    }

    /// Remove a bullet from the playbook.
    pub fn remove_bullet(&mut self, bullet_id: &str) {
        if let Some(bullet) = self.bullets.remove(bullet_id)
            && let Some(section_ids) = self.sections.get_mut(&bullet.section)
        {
            section_ids.retain(|id| id != bullet_id);
            if section_ids.is_empty() {
                self.sections.remove(&bullet.section);
            }
        }
    }

    /// Get a bullet by ID.
    pub fn get_bullet(&self, bullet_id: &str) -> Option<&Bullet> {
        self.bullets.get(bullet_id)
    }

    /// Get all bullets.
    pub fn bullets(&self) -> Vec<&Bullet> {
        self.bullets.values().collect()
    }

    /// Get number of bullets.
    pub fn bullet_count(&self) -> usize {
        self.bullets.len()
    }

    /// Get section names.
    pub fn section_names(&self) -> Vec<&str> {
        self.sections.keys().map(String::as_str).collect()
    }

    // ------------------------------------------------------------------ //
    // Serialization
    // ------------------------------------------------------------------ //

    /// Convert to serializable dictionary.
    pub fn to_dict(&self) -> serde_json::Value {
        let bullets_map: serde_json::Map<String, serde_json::Value> = self
            .bullets
            .iter()
            .map(|(id, bullet)| (id.clone(), serde_json::to_value(bullet).unwrap_or_default()))
            .collect();
        serde_json::json!({
            "bullets": bullets_map,
            "sections": self.sections,
            "next_id": self.next_id,
        })
    }

    /// Load from dictionary.
    pub fn from_dict(payload: &serde_json::Value) -> Self {
        let mut instance = Self::new();

        if let Some(bullets_obj) = payload.get("bullets").and_then(|v| v.as_object()) {
            for (id, val) in bullets_obj {
                if let Ok(bullet) = serde_json::from_value::<Bullet>(val.clone()) {
                    instance.bullets.insert(id.clone(), bullet);
                }
            }
        }

        if let Some(sections_obj) = payload.get("sections").and_then(|v| v.as_object()) {
            for (section, ids_val) in sections_obj {
                if let Some(ids_arr) = ids_val.as_array() {
                    let ids: Vec<String> = ids_arr
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    instance.sections.insert(section.clone(), ids);
                }
            }
        }

        instance.next_id = payload.get("next_id").and_then(|v| v.as_u64()).unwrap_or(0);

        instance
    }

    /// Serialize to JSON string.
    pub fn dumps(&self) -> String {
        serde_json::to_string_pretty(&self.to_dict()).unwrap_or_default()
    }

    /// Deserialize from JSON string.
    pub fn loads(data: &str) -> Result<Self, serde_json::Error> {
        let payload: serde_json::Value = serde_json::from_str(data)?;
        Ok(Self::from_dict(&payload))
    }

    /// Save playbook to JSON file.
    pub fn save_to_file(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.dumps())
    }

    /// Load playbook from JSON file.
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        Ok(Self::loads(&content)?)
    }

    // ------------------------------------------------------------------ //
    // Delta operations
    // ------------------------------------------------------------------ //

    /// Apply a batch of delta operations.
    pub fn apply_delta(&mut self, delta: &DeltaBatch) {
        for operation in &delta.operations {
            self.apply_operation(operation);
        }
    }

    /// Apply a single delta operation.
    fn apply_operation(&mut self, operation: &DeltaOperation) {
        match operation.op_type {
            DeltaOperationType::Add => {
                self.add_bullet(
                    &operation.section,
                    operation.content.as_deref().unwrap_or(""),
                    operation.bullet_id.as_deref(),
                    if operation.metadata.is_empty() {
                        None
                    } else {
                        Some(&operation.metadata)
                    },
                );
            }
            DeltaOperationType::Update => {
                if let Some(ref bid) = operation.bullet_id {
                    self.update_bullet(
                        bid,
                        operation.content.as_deref(),
                        if operation.metadata.is_empty() {
                            None
                        } else {
                            Some(&operation.metadata)
                        },
                    );
                }
            }
            DeltaOperationType::Tag => {
                if let Some(ref bid) = operation.bullet_id {
                    let valid_tags = ["helpful", "harmful", "neutral"];
                    for (tag, &increment) in &operation.metadata {
                        if valid_tags.contains(&tag.as_str()) {
                            self.tag_bullet(bid, tag, increment);
                        }
                    }
                }
            }
            DeltaOperationType::Remove => {
                if let Some(ref bid) = operation.bullet_id {
                    self.remove_bullet(bid);
                }
            }
        }
    }

    // ------------------------------------------------------------------ //
    // Presentation helpers
    // ------------------------------------------------------------------ //

    /// Return playbook as formatted string for LLM prompting.
    pub fn as_prompt(&self) -> String {
        if self.bullets.is_empty() {
            return String::new();
        }
        let mut parts = Vec::new();
        let mut sorted_sections: Vec<_> = self.sections.iter().collect();
        sorted_sections.sort_by_key(|(name, _)| *name);

        for (section, bullet_ids) in sorted_sections {
            parts.push(format!("## {section}"));
            for bid in bullet_ids {
                if let Some(bullet) = self.bullets.get(bid) {
                    let counters = format!(
                        "(helpful={}, harmful={}, neutral={})",
                        bullet.helpful, bullet.harmful, bullet.neutral
                    );
                    parts.push(format!("- [{}] {} {}", bullet.id, bullet.content, counters));
                }
            }
        }
        parts.join("\n")
    }

    /// Get playbook statistics.
    pub fn stats(&self) -> PlaybookStats {
        let mut helpful = 0i64;
        let mut harmful = 0i64;
        let mut neutral = 0i64;
        for bullet in self.bullets.values() {
            helpful += bullet.helpful;
            harmful += bullet.harmful;
            neutral += bullet.neutral;
        }
        PlaybookStats {
            sections: self.sections.len(),
            bullets: self.bullets.len(),
            helpful,
            harmful,
            neutral,
        }
    }

    // ------------------------------------------------------------------ //
    // Internal helpers
    // ------------------------------------------------------------------ //

    fn generate_id(&mut self, section: &str) -> String {
        self.next_id += 1;
        let prefix = section
            .split_whitespace()
            .next()
            .unwrap_or("bullet")
            .to_lowercase();
        format!("{prefix}-{:05}", self.next_id)
    }
}

impl Default for Playbook {
    fn default() -> Self {
        Self::new()
    }
}

/// Playbook statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookStats {
    pub sections: usize,
    pub bullets: usize,
    pub helpful: i64,
    pub harmful: i64,
    pub neutral: i64,
}

#[cfg(test)]
#[path = "playbook_tests.rs"]
mod tests;
