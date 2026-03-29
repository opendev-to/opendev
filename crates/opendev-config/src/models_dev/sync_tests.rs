use super::*;

#[test]
fn test_convert_provider_to_internal() {
    let provider_data = serde_json::json!({
        "name": "TestAI",
        "env": ["TESTAI_API_KEY"],
        "api": "https://api.testai.com/v1",
        "models": {
            "test-model": {
                "id": "test-model",
                "name": "Test Model",
                "limit": {"context": 8192, "output": 4096},
                "cost": {"input": 0.5, "output": 1.0},
                "modalities": {"input": ["text", "image"]},
                "reasoning": false
            }
        }
    });

    let result = convert_provider_to_internal("testai", &provider_data).unwrap();
    assert_eq!(result["id"], "testai");
    assert_eq!(result["name"], "TestAI");
    assert_eq!(result["api_key_env"], "TESTAI_API_KEY");

    let model = &result["models"]["test-model"];
    assert_eq!(model["context_length"], 8192);
    assert!(
        model["capabilities"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("vision"))
    );
    assert!(model["recommended"].as_bool().unwrap());
}
