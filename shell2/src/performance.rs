//! Native surface diagnostics cross the bridge as plain observations, without driving an app.
pub(crate) fn json(snapshot: Option<&runtime::FramePerformance>) -> serde_json::Value {
    let Some(s) = snapshot else {
        return serde_json::json!({"enabled":false});
    };
    let fps = s.fps();
    serde_json::json!({
        "enabled":true,"mode":s.mode,"fps":fps,"cadence_ms":fps.map(|f|1000.0/f),
        "normal_frames":s.normal_frames,"presented_frames":s.presented_frames,
        "samples":s.samples,"sample_capacity":240,"target_fps":60,"budget_ms":1000.0/60.0,
        "work_ms":{"last":s.work_last_ms,"p50":s.work_p50_ms,"p95":s.work_p95_ms,"max":s.work_max_ms},
        "over_budget":s.over_budget,"age_ms":s.age_ms(),"gpu_ms":null,
        "viewport":[s.viewport.0,s.viewport.1],"scale":s.scale,"adapter":s.adapter.as_ref(),
        "profile":if cfg!(debug_assertions) {"debug"} else {"release"},
        "overlay":s.label.as_ref(),"overlay_rect":s.overlay_rect(s.viewport),
    })
}
