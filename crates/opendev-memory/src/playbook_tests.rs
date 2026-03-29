use super::*;

#[test]
fn test_bullet_tag() {
    let now = Utc::now().to_rfc3339();
    let mut bullet = Bullet {
        id: "test-1".to_string(),
        section: "testing".to_string(),
        content: "Test content".to_string(),
        helpful: 0,
        harmful: 0,
        neutral: 0,
        created_at: now.clone(),
        updated_at: now,
    };

    bullet.tag("helpful", 1).unwrap();
    assert_eq!(bullet.helpful, 1);

    bullet.tag("harmful", 2).unwrap();
    assert_eq!(bullet.harmful, 2);

    assert!(bullet.tag("invalid", 1).is_err());
}

#[test]
fn test_bullet_apply_metadata() {
    let now = Utc::now().to_rfc3339();
    let mut bullet = Bullet {
        id: "test-1".to_string(),
        section: "testing".to_string(),
        content: "Test".to_string(),
        helpful: 0,
        harmful: 0,
        neutral: 0,
        created_at: now.clone(),
        updated_at: now,
    };

    let mut meta = HashMap::new();
    meta.insert("helpful".to_string(), 5);
    meta.insert("neutral".to_string(), 3);
    bullet.apply_metadata(&meta);

    assert_eq!(bullet.helpful, 5);
    assert_eq!(bullet.neutral, 3);
    assert_eq!(bullet.harmful, 0);
}

#[test]
fn test_playbook_add_and_get() {
    let mut pb = Playbook::new();
    pb.add_bullet("testing", "Write tests first", None, None);
    pb.add_bullet("testing", "Check coverage", None, None);
    pb.add_bullet("code_nav", "Search before read", None, None);

    assert_eq!(pb.bullet_count(), 3);
    assert_eq!(pb.section_names().len(), 2);
}

#[test]
fn test_playbook_add_with_id() {
    let mut pb = Playbook::new();
    let bullet = pb.add_bullet("testing", "Custom ID bullet", Some("custom-001"), None);
    assert_eq!(bullet.id, "custom-001");
    assert!(pb.get_bullet("custom-001").is_some());
}

#[test]
fn test_playbook_update() {
    let mut pb = Playbook::new();
    pb.add_bullet("testing", "Original content", Some("t-001"), None);

    let updated = pb.update_bullet("t-001", Some("Updated content"), None);
    assert!(updated.is_some());
    assert_eq!(updated.unwrap().content, "Updated content");

    // Non-existent bullet returns None
    assert!(pb.update_bullet("nonexistent", Some("x"), None).is_none());
}

#[test]
fn test_playbook_tag() {
    let mut pb = Playbook::new();
    pb.add_bullet("testing", "Test", Some("t-001"), None);

    pb.tag_bullet("t-001", "helpful", 1);
    pb.tag_bullet("t-001", "helpful", 1);
    pb.tag_bullet("t-001", "harmful", 1);

    let bullet = pb.get_bullet("t-001").unwrap();
    assert_eq!(bullet.helpful, 2);
    assert_eq!(bullet.harmful, 1);
}

#[test]
fn test_playbook_remove() {
    let mut pb = Playbook::new();
    pb.add_bullet("testing", "To be removed", Some("t-001"), None);
    assert_eq!(pb.bullet_count(), 1);

    pb.remove_bullet("t-001");
    assert_eq!(pb.bullet_count(), 0);
    assert!(pb.section_names().is_empty());

    // Removing non-existent bullet is a no-op
    pb.remove_bullet("nonexistent");
}

#[test]
fn test_playbook_serialization_roundtrip() {
    let mut pb = Playbook::new();
    pb.add_bullet("testing", "Write tests", Some("t-001"), None);
    pb.add_bullet("code_nav", "Search first", Some("cn-001"), None);
    pb.tag_bullet("t-001", "helpful", 3);

    let json = pb.dumps();
    let restored = Playbook::loads(&json).unwrap();

    assert_eq!(restored.bullet_count(), 2);
    let bullet = restored.get_bullet("t-001").unwrap();
    assert_eq!(bullet.helpful, 3);
    assert_eq!(bullet.content, "Write tests");
}

#[test]
fn test_playbook_file_persistence() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("playbook.json");

    let mut pb = Playbook::new();
    pb.add_bullet("testing", "Test bullet", Some("t-001"), None);
    pb.save_to_file(&path).unwrap();

    let loaded = Playbook::load_from_file(&path).unwrap();
    assert_eq!(loaded.bullet_count(), 1);
    assert_eq!(loaded.get_bullet("t-001").unwrap().content, "Test bullet");
}

#[test]
fn test_playbook_apply_delta() {
    let mut pb = Playbook::new();
    pb.add_bullet("testing", "Existing bullet", Some("t-001"), None);

    let delta = DeltaBatch {
        reasoning: "Adding and updating".to_string(),
        operations: vec![
            DeltaOperation {
                op_type: DeltaOperationType::Add,
                section: "new_section".to_string(),
                content: Some("New bullet".to_string()),
                bullet_id: Some("n-001".to_string()),
                metadata: HashMap::new(),
            },
            DeltaOperation {
                op_type: DeltaOperationType::Tag,
                section: "testing".to_string(),
                content: None,
                bullet_id: Some("t-001".to_string()),
                metadata: [("helpful".to_string(), 1)].into_iter().collect(),
            },
        ],
    };

    pb.apply_delta(&delta);
    assert_eq!(pb.bullet_count(), 2);
    assert_eq!(pb.get_bullet("t-001").unwrap().helpful, 1);
    assert!(pb.get_bullet("n-001").is_some());
}

#[test]
fn test_playbook_apply_delta_remove() {
    let mut pb = Playbook::new();
    pb.add_bullet("testing", "To remove", Some("t-001"), None);

    let delta = DeltaBatch {
        reasoning: "Removing harmful bullet".to_string(),
        operations: vec![DeltaOperation {
            op_type: DeltaOperationType::Remove,
            section: "testing".to_string(),
            content: None,
            bullet_id: Some("t-001".to_string()),
            metadata: HashMap::new(),
        }],
    };

    pb.apply_delta(&delta);
    assert_eq!(pb.bullet_count(), 0);
}

#[test]
fn test_playbook_as_prompt() {
    let mut pb = Playbook::new();
    pb.add_bullet("code_nav", "Search before read", Some("cn-001"), None);
    pb.add_bullet("testing", "Run tests after changes", Some("t-001"), None);

    let prompt = pb.as_prompt();
    assert!(prompt.contains("## code_nav"));
    assert!(prompt.contains("## testing"));
    assert!(prompt.contains("[cn-001] Search before read"));
    assert!(prompt.contains("[t-001] Run tests after changes"));
}

#[test]
fn test_playbook_as_prompt_empty() {
    let pb = Playbook::new();
    assert!(pb.as_prompt().is_empty());
}

#[test]
fn test_playbook_stats() {
    let mut pb = Playbook::new();
    pb.add_bullet("testing", "Test", Some("t-001"), None);
    pb.add_bullet("nav", "Nav", Some("n-001"), None);
    pb.tag_bullet("t-001", "helpful", 3);
    pb.tag_bullet("t-001", "harmful", 1);
    pb.tag_bullet("n-001", "neutral", 2);

    let stats = pb.stats();
    assert_eq!(stats.sections, 2);
    assert_eq!(stats.bullets, 2);
    assert_eq!(stats.helpful, 3);
    assert_eq!(stats.harmful, 1);
    assert_eq!(stats.neutral, 2);
}

#[test]
fn test_playbook_generate_id() {
    let mut pb = Playbook::new();
    let b1_id = pb
        .add_bullet("file operations", "First", None, None)
        .id
        .clone();
    let b2_id = pb
        .add_bullet("file operations", "Second", None, None)
        .id
        .clone();

    assert!(b1_id.starts_with("file-"));
    assert!(b2_id.starts_with("file-"));
    assert_ne!(b1_id, b2_id);
}

#[test]
fn test_playbook_from_dict_empty() {
    let payload = serde_json::json!({});
    let pb = Playbook::from_dict(&payload);
    assert_eq!(pb.bullet_count(), 0);
}
