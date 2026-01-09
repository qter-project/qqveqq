use leptos::{ev::canplay, prelude::*};
use leptos_use::{
    FacingMode, UseEventListenerOptions, UseUserMediaOptions, UseUserMediaReturn,
    VideoTrackConstraints, use_event_listener_with_options, use_user_media_with_options,
};
use log::{error, info};
use wasm_bindgen::JsCast;
use web_sys::CanvasRenderingContext2d;

const WIDTH: u32 = 350;

#[component]
pub fn Video(take_picture_command: ReadSignal<()>) -> impl IntoView {
    let video_ref = NodeRef::<leptos::html::Video>::new();
    let canvas_ref = NodeRef::<leptos::html::Canvas>::new();
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

        let binding = stream.read();
        let maybe_stream = match binding.as_ref() {
            Some(Ok(s)) => {
                info!("Stream is currently enabled");
                Some(s)
            }
            Some(Err(e)) => {
                error!("Failed to get media stream: {e:?}");
                None
            }
            None => {
                info!("Stream is currently disabled");
                None
            }
        };

        if let Some(v) = video_ref.get() {
            v.set_src_object(maybe_stream);
        }
    });

    let toggle_enabled = move |_| {
        set_enabled.update(|e| *e = !*e);
    };

    Effect::watch(
        move || take_picture_command.get(),
        move |(), _, _| {
            let (Some(canvas_ref), Some(video_ref)) =
                (canvas_ref.get_untracked(), video_ref.get_untracked())
            else {
                return;
            };
            let ctx = canvas_ref
                .get_context("2d")
                .unwrap()
                .unwrap()
                .dyn_into::<CanvasRenderingContext2d>()
                .unwrap();

            ctx.draw_image_with_html_video_element_and_dw_and_dh(
                &video_ref,
                0.0,
                0.0,
                f64::from(canvas_ref.width()),
                f64::from(canvas_ref.height()),
            )
            .unwrap();

            let image_data = ctx
                .get_image_data(
                    0.0,
                    0.0,
                    canvas_ref.width().into(),
                    canvas_ref.height().into(),
                )
                .unwrap();
            let data = &*image_data.data();

            info!("Captured image data length: {}", data.len());
            let pixels = data
                .chunks_exact(4)
                .map(|rgba| {
                    let [r, g, b, _] = rgba.try_into().unwrap();
                    (
                        f64::from(r) / 255.0,
                        f64::from(g) / 255.0,
                        f64::from(b) / 255.0,
                    )
                })
                .collect::<Vec<_>>()
                .into_boxed_slice();
            info!("{:?}", pixels[10]);
        },
        false,
    );

    let _ = use_event_listener_with_options(
        video_ref,
        canplay,
        move |_| {
            let video_ref = video_ref.get().unwrap();
            let canvas_ref = canvas_ref.get().unwrap();
            let height = f64::from(video_ref.video_height())
                / (f64::from(video_ref.video_width()) / f64::from(WIDTH));
            video_ref
                .set_attribute("width", WIDTH.to_string().as_str())
                .unwrap();
            video_ref
                .set_attribute("height", height.to_string().as_str())
                .unwrap();
            canvas_ref
                .set_attribute("width", WIDTH.to_string().as_str())
                .unwrap();
            canvas_ref
                .set_attribute("height", height.to_string().as_str())
                .unwrap();
        },
        UseEventListenerOptions::default().once(true),
    );

    view! {
      <div class="flex gap-4 justify-around">
        <video
          node_ref=video_ref
          on:click=toggle_enabled
          controls=false
          autoplay=true
          muted=true
          class="flex-1 min-w-0 border-2 border-white max-w-[400px]"
        />
        <canvas node_ref=canvas_ref class="flex-1 min-w-0 border-2 border-amber-300 max-w-[400px]" />
      </div>
    }
}
