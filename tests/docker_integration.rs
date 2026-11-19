use std::process::Command;

#[test]
#[ignore]
fn docker_end_to_end_ubuntu_and_manjaro_tests() {
    let status = Command::new("bash")
        .arg("tests/docker/run_docker_tests.sh")
        .status()
        .expect("failed to execute tests/docker/run_docker_tests.sh");
    assert!(status.success(), "Docker integration tests failed");
}
