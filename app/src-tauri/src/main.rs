/// Set `BM_SPIKE=graph` to run the graph rendering benchmark on startup.
#[tauri::command]
fn spike_mode() -> Option<String> {
    std::env::var("BM_SPIKE").ok()
}

#[tauri::command]
fn spike_report(app: tauri::AppHandle, report: serde_json::Value) {
    println!("SPIKE_REPORT {report}");
    if spike_mode().is_some() {
        app.exit(0);
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![spike_mode, spike_report])
        .run(tauri::generate_context!())
        .expect("error while running Brainmatter");
}
