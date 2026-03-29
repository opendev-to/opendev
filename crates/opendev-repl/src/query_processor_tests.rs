use super::*;

#[test]
fn test_query_result_default() {
    let result = QueryResult::default();
    assert!(result.content.is_empty());
    assert_eq!(result.operation_summary, "—");
    assert!(result.error.is_none());
    assert!(result.latency_ms.is_none());
}

#[test]
fn test_enhance_query_no_refs() {
    let qp = QueryProcessor::new();
    let query = "explain this code without any references";
    let enhanced = qp.enhance_query(query);
    assert_eq!(enhanced, query);
}
