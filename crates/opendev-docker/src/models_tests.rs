use super::*;

#[test]
fn test_docker_config_defaults() {
    let cfg = DockerConfig::default();
    assert_eq!(cfg.image, "python:3.11");
    assert_eq!(cfg.memory, "4g");
    assert_eq!(cfg.cpus, "4");
    assert_eq!(cfg.network_mode, "bridge");
    assert_eq!(cfg.startup_timeout, 120.0);
    assert_eq!(cfg.pull_policy, "if-not-present");
    assert_eq!(cfg.server_port, 8000);
    assert!(cfg.environment.is_empty());
    assert!(cfg.shell_init.is_empty());
    assert_eq!(cfg.runtime_type, RuntimeType::Local);
}

#[test]
fn test_docker_config_serde_roundtrip() {
    let cfg = DockerConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: DockerConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.image, cfg.image);
    assert_eq!(parsed.server_port, cfg.server_port);
}

#[test]
fn test_runtime_type_serde() {
    let local = serde_json::to_string(&RuntimeType::Local).unwrap();
    assert_eq!(local, "\"local\"");
    let remote: RuntimeType = serde_json::from_str("\"remote\"").unwrap();
    assert_eq!(remote, RuntimeType::Remote);
}

#[test]
fn test_container_status_default() {
    let s = ContainerStatus::default();
    assert_eq!(s, ContainerStatus::Unknown);
}

#[test]
fn test_volume_mount_serde() {
    let mount = VolumeMount {
        host_path: "/home/user/code".into(),
        container_path: "/workspace".into(),
        read_only: false,
    };
    let json = serde_json::to_string(&mount).unwrap();
    let parsed: VolumeMount = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.host_path, mount.host_path);
    assert_eq!(parsed.container_path, mount.container_path);
    assert!(!parsed.read_only);
}

#[test]
fn test_port_mapping_default_protocol() {
    let json = r#"{"host_port": 8080, "container_port": 80}"#;
    let pm: PortMapping = serde_json::from_str(json).unwrap();
    assert_eq!(pm.protocol, "tcp");
}

#[test]
fn test_bash_action_defaults() {
    let json = r#"{"command": "ls"}"#;
    let action: BashAction = serde_json::from_str(json).unwrap();
    assert_eq!(action.command, "ls");
    assert_eq!(action.session, "default");
    assert_eq!(action.timeout, 120.0);
    assert_eq!(action.check, CheckMode::Silent);
}

#[test]
fn test_bash_observation_serde() {
    let obs = BashObservation {
        output: "hello\n".into(),
        exit_code: Some(0),
        failure_reason: None,
    };
    let json = serde_json::to_string(&obs).unwrap();
    let parsed: BashObservation = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.exit_code, Some(0));
    assert!(parsed.failure_reason.is_none());
}

#[test]
fn test_create_session_request_default() {
    let req = CreateSessionRequest::default();
    assert_eq!(req.session, "default");
    assert_eq!(req.startup_timeout, 10.0);
}

#[test]
fn test_is_alive_response_default() {
    let resp = IsAliveResponse::default();
    assert_eq!(resp.status, "ok");
    assert!(resp.message.is_empty());
}

#[test]
fn test_container_spec_serde() {
    let spec = ContainerSpec {
        image: "ubuntu:22.04".into(),
        memory: "2g".into(),
        cpus: "2".into(),
        network_mode: "host".into(),
        volumes: vec![VolumeMount {
            host_path: "/tmp".into(),
            container_path: "/mnt".into(),
            read_only: true,
        }],
        ports: vec![PortMapping {
            host_port: 3000,
            container_port: 3000,
            protocol: "tcp".into(),
        }],
        environment: [("FOO".into(), "bar".into())].into(),
        entrypoint: None,
        command: Some(vec!["bash".into()]),
    };
    let json = serde_json::to_string(&spec).unwrap();
    let parsed: ContainerSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.image, "ubuntu:22.04");
    assert_eq!(parsed.volumes.len(), 1);
    assert!(parsed.volumes[0].read_only);
}

#[test]
fn test_check_mode_serde() {
    let raise: CheckMode = serde_json::from_str("\"raise\"").unwrap();
    assert_eq!(raise, CheckMode::Raise);
    let ignore: CheckMode = serde_json::from_str("\"ignore\"").unwrap();
    assert_eq!(ignore, CheckMode::Ignore);
}

#[test]
fn test_tool_result_success() {
    let r = ToolResult {
        success: true,
        output: Some("done".into()),
        error: None,
        exit_code: Some(0),
    };
    assert!(r.success);
    assert_eq!(r.output.as_deref(), Some("done"));
}

#[test]
fn test_tool_result_failure() {
    let r = ToolResult {
        success: false,
        output: None,
        error: Some("not found".into()),
        exit_code: Some(1),
    };
    assert!(!r.success);
    assert_eq!(r.error.as_deref(), Some("not found"));
}

#[test]
fn test_exception_transfer_serde() {
    let et = ExceptionTransfer {
        message: "boom".into(),
        class_name: "RuntimeError".into(),
        module: "docker".into(),
        traceback: String::new(),
        extra: Default::default(),
    };
    let json = serde_json::to_string(&et).unwrap();
    let parsed: ExceptionTransfer = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.class_name, "RuntimeError");
}
