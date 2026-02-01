use leptos::{ev::canplay, html, prelude::*};
use leptos_use::{
    FacingMode, UseEventListenerOptions, UseUserMediaOptions, UseUserMediaReturn,
    VideoTrackConstraints, use_event_listener_with_options, use_user_media_with_options,
};
use log::{info, warn};
use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Blob, CanvasRenderingContext2d, HtmlCanvasElement, HtmlElement, HtmlVideoElement,
    js_sys::{self, Promise},
};

const WIDTH: u32 = 850;

fn draw_video_on_canvas(
    canvas_ref: &HtmlCanvasElement,
    video_ref: &HtmlVideoElement,
) -> CanvasRenderingContext2d {
    let opts = js_sys::Object::new();
    js_sys::Reflect::set(&opts, &"willReadFrequently".into(), &true.into()).unwrap();
    js_sys::Reflect::set(&opts, &"alpha".into(), &false.into()).unwrap();
    let ctx = canvas_ref
        .get_context_with_context_options("2d", &opts)
        .unwrap()
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()
        .unwrap();

    ctx.draw_image_with_html_video_element_and_dw_and_dh(
        video_ref,
        0.0,
        0.0,
        canvas_ref.width().into(),
        canvas_ref.height().into(),
    )
    .unwrap();
    ctx
}

pub(crate) fn take_picture_command(
    video_ref: &HtmlVideoElement,
    canvas_ref: &HtmlCanvasElement,
) -> Box<[(f64, f64, f64)]> {
    let ctx = draw_video_on_canvas(canvas_ref, video_ref);

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

    data.chunks_exact(4)
        .map(|rgba| {
            let [r, g, b, _] = rgba.try_into().unwrap();
            (
                f64::from(r) / 255.0,
                f64::from(g) / 255.0,
                f64::from(b) / 255.0,
            )
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

pub(crate) async fn pixel_assignment_command(
    video_ref: &HtmlVideoElement,
    canvas_ref: &HtmlCanvasElement,
) -> Result<Blob, JsValue> {
    draw_video_on_canvas(canvas_ref, video_ref);

    let promise = Promise::new(&mut |resolve, reject| {
        let resolve = resolve.clone();
        let closure = Closure::once(move |blob: Option<Blob>| match blob {
            Some(blob) => {
                resolve.call1(&JsValue::NULL, &blob).unwrap();
            }
            None => {
                reject
                    .call1(&JsValue::NULL, &JsValue::from_str("canvas toBlob failed"))
                    .unwrap();
            }
        });
        canvas_ref
            .to_blob_with_type_and_encoder_options(
                closure.as_ref().unchecked_ref(),
                "image/webp",
                &JsValue::from_f64(0.8),
            )
            .unwrap();
        closure.forget();
    });
    let blob = JsFuture::from(promise).await?;
    Ok(blob.dyn_into::<Blob>().unwrap())
}

#[component]
pub fn Video(video_ref: NodeRef<html::Video>, canvas_ref: NodeRef<html::Canvas>) -> impl IntoView {
    let UseUserMediaReturn {
        stream,
        enabled,
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
        let video_ref = video_ref.get().unwrap();
        let stream = stream.read();
        let maybe_stream = match stream.as_ref() {
            Some(Ok(s)) => {
                info!("Video is currently enabled");
                Some(s)
            }
            Some(Err(e)) => {
                warn!("Failed to get intialize video: {e:?}");
                None
            }
            None => {
                info!("Video is currently disabled");
                None
            }
        };

        video_ref.set_src_object(maybe_stream);
        let new = maybe_stream.is_some();
        let old = enabled.get_untracked();
        if new != old {
            set_enabled.set(new);
        }
    });

    let toggle_enabled = move |_| {
        set_enabled.update(|e| *e = !*e);
    };

    let _ = use_event_listener_with_options(
        video_ref,
        canplay,
        move |_| {
            let video_ref = video_ref.get().unwrap();
            let canvas_ref = canvas_ref.get().unwrap();
            let height = f64::from(video_ref.video_height())
                / (f64::from(video_ref.video_width()) / f64::from(WIDTH));
            video_ref
                .dyn_ref::<HtmlElement>()
                .unwrap()
                .style()
                .set_property("height", &format!("{height}px"))
                .unwrap();
            video_ref
                .dyn_ref::<HtmlElement>()
                .unwrap()
                .style()
                .set_property("width", &format!("{WIDTH}px"))
                .unwrap();
            canvas_ref
                .dyn_ref::<HtmlElement>()
                .unwrap()
                .style()
                .set_property("height", &format!("{height}px"))
                .unwrap();
            canvas_ref
                .dyn_ref::<HtmlElement>()
                .unwrap()
                .style()
                .set_property("width", &format!("{WIDTH}px"))
                .unwrap();
            // video_ref
            //     .set_attribute("width", WIDTH.to_string().as_str())
            //     .unwrap();
            // video_ref
            //     .set_attribute("height", height.to_string().as_str())
            //     .unwrap();
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
          class="flex-1 min-w-0 border-2 border-white"
        />
        <canvas node_ref=canvas_ref class="flex-1 min-w-0 border-2 border-amber-300" />
      </div>
    }
}
