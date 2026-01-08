use leptos::prelude::*;
use leptos_use::{
    FacingMode, UseUserMediaOptions, UseUserMediaReturn, VideoTrackConstraints,
    use_user_media_with_options,
};
use log::{error, info};

#[component]
pub fn Video() -> impl IntoView {
    let video_ref = NodeRef::<leptos::html::Video>::new();
    let UseUserMediaReturn {
        stream,
        set_enabled,
        ..
    } = use_user_media_with_options(
        UseUserMediaOptions::default()
            .video(VideoTrackConstraints::default().facing_mode(FacingMode::Environment)), // .enabled((enabled, set_enabled).into()),
    );

    Effect::new(move |_| {
        // let media = use_window()
        //     .navigator()
        //     .ok_or_else(|| JsValue::from_str("Failed to access window.navigator"))
        //     .and_then(|n| n.media_devices())
        //     .unwrap();

        match stream.get() {
            Some(Ok(s)) => {
                info!("Stream is currently enabled");
                video_ref.with(|v| {
                    if let Some(v) = v {
                        v.set_src_object(Some(&s));
                    }
                });
                return;
            }
            Some(Err(e)) => error!("Failed to get media stream: {:?}", e),
            None => info!("Stream is currently disabled"),
        }

        video_ref.with(|v| {
            if let Some(v) = v {
                v.set_src_object(None);
            }
        });
    });

    let toggle_enabled = move |_| {
        set_enabled.update(|e| *e = !*e);
    };

    view! {
      <video
        node_ref=video_ref
        on:click=toggle_enabled
        controls=false
        autoplay=true
        muted=true
        class="w-auto h-96 border-2 border-white"
      />
      <canvas class="hidden" />
    }
}
