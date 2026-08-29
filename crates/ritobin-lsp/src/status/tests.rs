use super::*;

fn loading(msg: &str) -> TaskStatus {
    TaskStatus::Loading(msg.to_owned())
}

fn failed(msg: &str) -> TaskStatus {
    TaskStatus::Failed(msg.to_owned())
}

#[test]
fn nothing_started_yet_is_not_quiescent() {
    let params = ServerStatus::default().params();
    assert!(!params.quiescent);
    assert_eq!(params.health, Health::Ok);
    assert_eq!(params.message, None);
}

#[test]
fn a_loading_task_reports_its_message() {
    let params = ServerStatus {
        hashes: loading("Updating hashtables"),
        meta: TaskStatus::Ready,
    }
    .params();

    assert!(!params.quiescent);
    assert_eq!(params.health, Health::Ok);
    assert_eq!(params.message.as_deref(), Some("Updating hashtables"));
}

#[test]
fn concurrent_loading_tasks_are_listed_together() {
    let params = ServerStatus {
        hashes: loading("Updating hashtables"),
        meta: loading("Downloading meta dump"),
    }
    .params();

    assert_eq!(
        params.message.as_deref(),
        Some("Downloading meta dump, Updating hashtables")
    );
}

#[test]
fn everything_ready_is_quiescent_and_silent() {
    let params = ServerStatus {
        hashes: TaskStatus::Ready,
        meta: TaskStatus::Ready,
    }
    .params();

    assert!(params.quiescent);
    assert_eq!(params.health, Health::Ok);
    assert_eq!(params.message, None);
}

#[test]
fn a_failure_degrades_health_but_still_settles() {
    let params = ServerStatus {
        hashes: failed("No hashtable directory"),
        meta: TaskStatus::Ready,
    }
    .params();

    assert!(params.quiescent, "a failed task is finished, not pending");
    assert_eq!(params.health, Health::Warning);
    assert_eq!(params.message.as_deref(), Some("No hashtable directory"));
}

#[test]
fn work_in_progress_outranks_a_past_failure_in_the_message() {
    let params = ServerStatus {
        hashes: failed("No hashtable directory"),
        meta: loading("Downloading meta dump"),
    }
    .params();

    assert!(!params.quiescent);
    assert_eq!(params.health, Health::Warning);
    assert_eq!(params.message.as_deref(), Some("Downloading meta dump"));
}

#[test]
fn every_failure_is_named() {
    let params = ServerStatus {
        hashes: failed("No hashtable directory"),
        meta: failed("Meta fetch failed"),
    }
    .params();

    assert_eq!(
        params.message.as_deref(),
        Some("Meta fetch failed, No hashtable directory")
    );
}
