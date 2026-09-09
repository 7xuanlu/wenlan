// SPDX-License-Identifier: AGPL-3.0-only

pub fn run_review() {
    tauri::Builder::default()
        .setup(|_app| {
            #[cfg(target_os = "macos")]
            {
                use tauri::{Manager, WindowEvent};

                if let Some(main_window) = _app.get_webview_window("main") {
                    crate::schedule_main_window_traffic_lights_alignment(&main_window);
                    let window_for_events = main_window.clone();
                    main_window.on_window_event(move |event| {
                        if matches!(
                            event,
                            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
                        ) {
                            crate::schedule_main_window_traffic_lights_alignment(
                                &window_for_events,
                            );
                        }
                    });
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Wenlan Review");
}
