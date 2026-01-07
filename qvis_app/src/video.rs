use leptos::{
    logging::{error, log},
    prelude::*,
};
use leptos_use::{
    FacingMode, UseUserMediaOptions, UseUserMediaReturn, VideoTrackConstraints,
    use_user_media_with_options,
};

#[component]
pub fn Video(enabled: ReadSignal<bool>, set_enabled: WriteSignal<bool>) -> impl IntoView {
    let video_ref = NodeRef::<leptos::html::Video>::new();
    let UseUserMediaReturn { stream, .. } = use_user_media_with_options(
        UseUserMediaOptions::default()
            .video(VideoTrackConstraints::default().facing_mode(FacingMode::Environment))
            .enabled((enabled, set_enabled).into()),
    );

    Effect::new(move |_| {
        // let media = use_window()
        //     .navigator()
        //     .ok_or_else(|| JsValue::from_str("Failed to access window.navigator"))
        //     .and_then(|n| n.media_devices())
        //     .unwrap();

        match stream.get() {
            Some(Ok(s)) => {
                video_ref.with(|v| {
                    if let Some(v) = v {
                        v.set_src_object(Some(&s));
                    }
                });
                return;
            }
            Some(Err(e)) => error!("Failed to get media stream: {:?}", e),
            None => log!("No stream yet"),
        }

        video_ref.with(|v| {
            if let Some(v) = v {
                v.set_src_object(None);
            }
        });
    });

    view! { <video node_ref=video_ref controls=false autoplay=true muted=true class="w-auto h-96" /> }
}
