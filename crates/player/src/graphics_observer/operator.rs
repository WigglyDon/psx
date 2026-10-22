//! Pure contract used by the bounded physical-test operator. No device access.
use serde_json::Value;

pub fn unstable_log(log: &str) -> bool {
    let log = log.to_ascii_lowercase();
    [
        "retiring the runtime before graphics recovery",
        "failed to submit",
        "submission failed",
        "gpu fault",
        "fatal signal",
        "context lost",
        "out of memory",
    ]
    .iter()
    .any(|v| log.contains(v))
        || log.lines().any(|line| {
            (line.contains("ioctl_kgsl_gpu_command") && line.contains("failed: errno"))
                || ((line.contains("kgsl") || line.contains("adreno"))
                    && (line.contains("waittimestamp")
                        || line.contains("wait timestamp")
                        || line.contains("wait for"))
                    && (line.contains("failed")
                        || line.contains("timeout")
                        || line.contains("deadlock")))
        })
}
pub fn export_present(code: i32, text: &str) -> Result<bool, &'static str> {
    match (code, text.trim()) {
        (0, "PRESENT") => Ok(true),
        (0, "ABSENT") => Ok(false),
        _ => Err("Export presence not established"),
    }
}
pub fn decode_snapshot(bytes: &[u8]) -> Result<Value, &'static str> {
    if bytes.len() > super::EXPORT_LIMIT {
        return Err("Export exceeded bound");
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "Malformed export")?;
    if value["version"] != 2
        || value["capacity"] != 256
        || value["tail"].as_array().is_none_or(|v| v.len() > 256)
    {
        return Err("Invalid export schema");
    }
    Ok(value)
}
pub fn decode_stop(bytes: &[u8], mode: &str) -> Result<Value, &'static str> {
    if bytes.len() > 4096 {
        return Err("Stop indication exceeded bound");
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "Malformed stop indication")?;
    if value["version"] != 2
        || value["mode"] != mode
        || !value["reason"]
            .as_u64()
            .is_some_and(|r| (1..=8).contains(&r))
    {
        return Err("Invalid stop indication");
    }
    Ok(value)
}
/// A malformed late export cannot replace or invalidate prior accepted files.
/// Callers persist each returned value before attempting the next optional file.
pub fn accept_snapshot(accepted: &mut Vec<Value>, bytes: &[u8]) -> Result<(), &'static str> {
    let value = decode_snapshot(bytes)?;
    if accepted.len() == super::MAX_EXPORTS {
        return Err("Export count exceeded bound");
    }
    accepted.push(value);
    Ok(())
}
/// Same-lease operator request is consumed by the observer writer and wakes the
/// actual application owner. Only the previously verified scratch is writable.
pub fn stop_shell() -> String {
    format!(
        "test -f files/{}/prepared-v1 && : > files/{}/operator-stop",
        super::SCRATCH,
        super::SCRATCH
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submission_and_wait_warning_stop_but_initial_info_does_not() {
        assert!(unstable_log(
            "(IOCTL_KGSL_GPU_COMMAND) failed: errno 35 Resource deadlock would occur"
        ));
        assert!(unstable_log("Adreno KGSL waittimestamp failed errno 35"));
        assert!(!unstable_log(
            "gpu_model file not found; GL renderer Adreno"
        ));
    }
    #[test]
    fn absent_file_and_late_collection_failure_preserve_accepted_exports() {
        assert_eq!(export_present(0, "ABSENT\n"), Ok(false));
        assert!(export_present(0, "cat: missing file").is_err());
        assert!(export_present(1, "ABSENT").is_err());
        let mut accepted = vec![];
        let bytes = br#"{"version":2,"capacity":256,"tail":[]}"#;
        accept_snapshot(&mut accepted, bytes).unwrap();
        accept_snapshot(&mut accepted, bytes).unwrap();
        assert!(accept_snapshot(&mut accepted, b"cat: missing file").is_err());
        assert_eq!(accepted.len(), 2);
        assert!(decode_stop(br#"{"version":2,"mode":"normal","reason":1}"#, "normal").is_ok());
        assert!(decode_stop(br#"{"version":2,"mode":"normal","reason":0}"#, "normal").is_err());
    }
}
