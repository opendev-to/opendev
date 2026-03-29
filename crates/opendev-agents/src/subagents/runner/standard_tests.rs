use super::*;

#[test]
fn test_standard_runner_name() {
    let runner = StandardReactRunner::new(ReactLoopConfig::default());
    assert_eq!(runner.name(), "StandardReactRunner");
}
