//! Conservative control-arm gate. A clean exit or old fence-only export is
//! insufficient. UI/copy/swap counters intentionally absent in the reference
//! are not required; real native guest/output and complete retirement are.
use super::{NATIVE_SHA, PLAYER_SOURCE, SETTINGS};
use serde_json::Value;
type Result<T> = std::result::Result<T, String>;
fn require(ok: bool, why: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(why.into())
    }
}
fn count(v: &Value, key: &str) -> Result<u64> {
    v.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("missing/invalid {key}"))
}
fn null(v: &Value, key: &str) -> bool {
    v.get(key).is_some_and(Value::is_null)
}
pub fn reference(result: &Value, snapshot: &Value, harness_source: &str) -> Result<()> {
    require(
        result["version"] == 1
            && result["minimal_host"] == true
            && result["harness_source"] == harness_source,
        "wrong/missing reference binding",
    )?;
    require(
        result["frozen_native"] == NATIVE_SHA && result["frozen_player"] == PLAYER_SOURCE,
        "frozen engine/control differs",
    )?;
    require(null(result, "result"), "reference fault or missing result")?;
    require(
        result["host_page"] == 4096 && result["egl_selection_matches_frozen_query"] == true,
        "page/EGL selection equivalence unqualified",
    )?;
    let window = result["window"]
        .as_array()
        .filter(|v| v.len() == 2)
        .ok_or("window absent")?;
    require(
        window
            .iter()
            .all(|v| v.as_u64().is_some_and(|v| (1..=16384).contains(&v)))
            && result["window"] == result["expected_window"],
        "unmatched window",
    )?;
    let gles = result["gles"]
        .as_array()
        .filter(|v| v.len() == 2)
        .ok_or("missing GLES")?;
    require(
        gles[0]
            .as_u64()
            .zip(gles[1].as_u64())
            .is_some_and(|v| v >= (3, 1)),
        "GLES requirement",
    )?;
    let egl = result["egl_attributes"]
        .as_array()
        .filter(|v| v.len() == 11)
        .ok_or("missing actual EGL attributes")?;
    require(
        egl.iter().all(|v| v.as_i64().is_some())
            && egl[0].as_u64().is_some_and(|v| v > 0)
            && egl[1..5].iter().all(|v| v == 8)
            && egl[10] == 3,
        "EGL configuration/context differs",
    )?;
    require(
        result["elapsed_since_guest_launch"]
            .as_f64()
            .is_some_and(|t| (20.0..=30.0).contains(&t)),
        "bounded comparable interval incomplete",
    )?;
    let e = result
        .get("evidence")
        .filter(|v| v.is_object())
        .ok_or("missing host evidence")?;
    for key in [
        "init",
        "loaded",
        "reset",
        "ready",
        "normal_speed_applied",
        "unloaded",
        "context_destroyed",
        "deinitialized",
        "callback_storage_released",
        "native_signals_restored",
    ] {
        require(e[key] == true, &format!("incomplete {key}"))?;
    }
    require(
        null(e, "first_failure") && null(e, "native_failure") && e["unexpected_software"] == false,
        "host failure/unknown failure fields",
    )?;
    require(
        e["dimensions"] == serde_json::json!([640, 448]),
        "different native output size",
    )?;
    for key in ["hardware_frames", "notifications", "services"] {
        require(
            count(e, key)? >= 480,
            "insufficient actual frontend progress",
        )?;
    }
    require(
        count(e, "software_frames")? == 0
            && count(e, "audio_rejected")? == 0
            && count(e, "audio_frames")? > 0,
        "audio/input/output path unqualified",
    )?;
    require(count(e, "inputs")? > 0, "neutral input never queried")?;
    require(
        e["hardware_request"]
            .as_array()
            .is_some_and(|v| v.len() == 7 && v.iter().all(|v| v.as_u64().is_some())),
        "hardware negotiation absent",
    )?;
    for (key, value) in SETTINGS {
        require(
            e["settings_queries"][key] == value,
            &format!("effective option absent/different: {key}"),
        )?;
    }
    require(
        snapshot["version"] == 2 && snapshot["capacity"] == 256 && null(snapshot, "first_failure"),
        "missing/faulted full observer evidence",
    )?;
    require(
        count(&snapshot["progress"], "producer_frame")? >= 512,
        "insufficient actual GS/output progress",
    )?;
    let p = &snapshot["output_sync"];
    for key in ["first_ownership_failure", "first_invalid_generation"] {
        require(null(p, key), "output ownership/fence failure")?;
    }
    for key in [
        "live",
        "invalid_names_released",
        "context_reclaim_pending",
        "frontend_sync_operations",
        "untracked_creates",
        "cancelled_lease_omissions",
    ] {
        require(count(p, key)? == 0, "incomplete output retirement")?;
    }
    require(
        p["overflow"] == false && p["native_joined"] == true,
        "native ownership join/coverage incomplete",
    )?;
    let created = count(p, "created")?;
    require(
        created >= 512 && count(p, "retired_on_creator")? == created,
        "created outputs not retired on creator",
    )?;
    require(
        count(p, "published")? >= 480
            && count(p, "consumed")? >= 480
            && count(p, "acknowledged")? >= 480,
        "output metadata progress insufficient",
    )?;
    let common = &snapshot["common"];
    for key in [
        "first_nonzero_error",
        "first_runtime_failure",
        "first_dependency_failure",
        "first_fatal",
        "first_failed_request",
    ] {
        require(
            null(common, key),
            "common renderer result not clean/complete",
        )?;
    }
    require(
        common["overflow"] == false
            && common["native_context_destroyed"] == true
            && common["frontend_context_destroyed"] == true,
        "common context retirement incomplete",
    )?;
    for key in ["unknown_releases", "untracked_resources"] {
        require(
            count(common, key)? == 0,
            "common allocation coverage incomplete",
        )?;
    }
    let phases = common["phase_entries"]
        .as_array()
        .filter(|v| v.len() == 10)
        .ok_or("actual native phase coverage missing")?;
    for i in [1, 4, 5, 6] {
        require(
            phases[i].as_u64().is_some_and(|v| v > 0),
            "real renderer/composition/donor overlay/output absent",
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn clean() -> (Value, Value) {
        let mut e = serde_json::to_value(crate::minimal_host::Evidence::default()).unwrap();
        for k in [
            "init",
            "loaded",
            "reset",
            "ready",
            "normal_speed_applied",
            "unloaded",
            "context_destroyed",
            "deinitialized",
            "callback_storage_released",
            "native_signals_restored",
        ] {
            e[k] = json!(true);
        }
        for k in [
            "hardware_frames",
            "notifications",
            "services",
            "inputs",
            "audio_frames",
        ] {
            e[k] = json!(600);
        }
        e["dimensions"] = json!([640, 448]);
        e["hardware_request"] = json!([4, 3, 0, 1, 1, 1, 0]);
        for (k, v) in SETTINGS {
            e["settings_queries"][k] = json!(v);
        }
        let r = json!({"version":1,"minimal_host":true,"harness_source":"fixture","frozen_native":NATIVE_SHA,"frozen_player":PLAYER_SOURCE,"result":null,"host_page":4096,"window":[1080,2280],"expected_window":[1080,2280],"gles":[3,2],"egl_selection_matches_frozen_query":true,"egl_attributes":[1,8,8,8,8,0,0,0,5,64,3],"elapsed_since_guest_launch":22.0,"evidence":e});
        let mut p =
            serde_json::to_value(crate::graphics_observer::output_sync::Progress::default())
                .unwrap();
        for k in [
            "created",
            "retired_on_creator",
            "published",
            "consumed",
            "acknowledged",
        ] {
            p[k] = json!(600);
        }
        p["native_joined"] = json!(true);
        let mut c =
            serde_json::to_value(crate::graphics_observer::common::Progress::default()).unwrap();
        c["native_context_destroyed"] = json!(true);
        c["frontend_context_destroyed"] = json!(true);
        for i in [1, 4, 5, 6] {
            c["phase_entries"][i] = json!(600);
        }
        let s = json!({"version":2,"capacity":256,"first_failure":null,"progress":{"producer_frame":600,"copied_frame":0,"presented_frame":0,"swap_frame":0},"output_sync":p,"common":c});
        (r, s)
    }
    #[test]
    fn clean_exit_and_missing_ui_counters_do_not_replace_full_workload_and_return() {
        let (r, s) = clean();
        assert!(reference(&r, &s, "fixture").is_ok());
        for path in [
            "/evidence/ready",
            "/evidence/normal_speed_applied",
            "/evidence/deinitialized",
            "/evidence/native_signals_restored",
            "/egl_selection_matches_frozen_query",
        ] {
            let mut bad = r.clone();
            *bad.pointer_mut(path).unwrap() = json!(false);
            assert!(reference(&bad, &s, "fixture").is_err(), "{path}");
        }
        for (path, v) in [
            ("/progress/producer_frame", json!(511)),
            ("/output_sync/retired_on_creator", json!(599)),
            ("/output_sync/native_joined", json!(false)),
            ("/first_failure", json!({"stage":16})),
            ("/common/first_fatal", json!({"phase":9})),
            ("/common/phase_entries/5", json!(0)),
        ] {
            let mut bad = s.clone();
            *bad.pointer_mut(path).unwrap() = v;
            assert!(reference(&r, &bad, "fixture").is_err(), "{path}");
        }
        assert!(reference(&json!({"result":null}), &s, "fixture").is_err());
        assert!(reference(&r, &json!({"first_failure":null}), "fixture").is_err());
    }
}
