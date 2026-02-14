use leptos::{ev::Targeted, html, prelude::*};
use leptos_use::{UseUserMediaReturn, use_event_listener};
use log::{info, warn};
use qvis::CVProcessor;
use send_wrapper::SendWrapper;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Notify, watch::Receiver};
use wasm_bindgen::{Clamped, JsCast, JsValue, prelude::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::js_sys;

const WIDTH: u32 = 850;

#[derive(Default)]
pub struct OnceBarrier {
    ready: AtomicBool,
    notify: Notify,
}

impl OnceBarrier {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    fn set_ready(&self) {
        self.ready.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    fn set_unready(&self) {
        self.ready.store(false, Ordering::Release);
    }

    async fn wait(&self) {
        if self.ready.load(Ordering::Acquire) {
            return;
        }
        self.notify.notified().await;
    }
}

async fn draw_video_on_canvas(
    canvas_ref: &web_sys::HtmlCanvasElement,
    video_ref: &web_sys::HtmlVideoElement,
    video_enabled: Signal<bool>,
    set_video_enabled: WriteSignal<bool>,
    playing_barrier: &OnceBarrier,
) -> web_sys::CanvasRenderingContext2d {
    let opts = js_sys::Object::new();
    js_sys::Reflect::set(&opts, &"willReadFrequently".into(), &true.into()).unwrap();
    js_sys::Reflect::set(&opts, &"alpha".into(), &false.into()).unwrap();
    let ctx = canvas_ref
        .get_context_with_context_options("2d", &opts)
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();
    if !video_enabled.get_untracked() {
        set_video_enabled.set(true);
    }
    playing_barrier.wait().await;
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

pub(crate) async fn take_picture_command(
    video_ref: &web_sys::HtmlVideoElement,
    canvas_ref: &web_sys::HtmlCanvasElement,
    video_enabled: Signal<bool>,
    set_video_enabled: WriteSignal<bool>,
    playing_barrier: &OnceBarrier,
) -> Box<[(f64, f64, f64)]> {
    let ctx = draw_video_on_canvas(
        canvas_ref,
        video_ref,
        video_enabled,
        set_video_enabled,
        playing_barrier,
    )
    .await;

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
    video_ref: &web_sys::HtmlVideoElement,
    canvas_ref: &web_sys::HtmlCanvasElement,
    video_enabled: Signal<bool>,
    set_video_enabled: WriteSignal<bool>,
    playing_barrier: &OnceBarrier,
) -> web_sys::Blob {
    draw_video_on_canvas(
        canvas_ref,
        video_ref,
        video_enabled,
        set_video_enabled,
        playing_barrier,
    )
    .await;

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve = resolve.clone();
        let closure = Closure::once(move |blob: Option<web_sys::Blob>| match blob {
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
                &JsValue::from_f64(1.0),
            )
            .unwrap();
        closure.forget();
    });
    let blob = JsFuture::from(promise).await.unwrap();
    blob.dyn_into::<web_sys::Blob>().unwrap()
}

async fn all_camera_devices() -> Result<Vec<SendWrapper<web_sys::MediaDeviceInfo>>, JsValue> {
    let media_devices = web_sys::window()
        .ok_or_else(|| JsValue::from_str("Failed to access window"))?
        .navigator()
        .media_devices()?;

    let devices_promise = media_devices.enumerate_devices()?;
    let devices_js = JsFuture::from(devices_promise).await?;
    let devices_array = js_sys::Array::from(&devices_js);

    Ok(devices_array
        .iter()
        .filter_map(|device_js| {
            let device_info: web_sys::MediaDeviceInfo = device_js.dyn_into().ok()?;
            if device_info.kind() == web_sys::MediaDeviceKind::Videoinput {
                Some(SendWrapper::new(device_info))
            } else {
                None
            }
        })
        .collect())
}

#[component]
pub fn Video(
    video_ref: NodeRef<html::Video>,
    canvas_ref: NodeRef<html::Canvas>,
    cv_overlay_ref: NodeRef<html::Canvas>,
    use_user_media_return: UseUserMediaReturn<
        impl Fn() + Clone + Send + Sync,
        impl Fn() + Clone + Send + Sync,
    >,
    playing_barrier: Arc<OnceBarrier>,
    mut cv_available_rx: Receiver<Option<CVProcessor>>,
) -> impl IntoView {
    let UseUserMediaReturn {
        stream,
        set_enabled,
        ..
    } = use_user_media_return;
    drop(use_user_media_return);

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
                set_enabled.set(false);
                None
            }
            None => {
                info!("Video is currently disabled");
                None
            }
        };

        video_ref.set_src_object(maybe_stream);
    });

    let toggle_enabled = {
        let playing_barrier = playing_barrier.clone();
        move |_| {
            set_enabled.update(|e| {
                if *e {
                    playing_barrier.set_unready();
                    *e = false;
                } else {
                    *e = true;
                }
            });
        }
    };

    let a = Arc::new(Notify::new());
    {
        let a = Arc::clone(&a);
        let _ = use_event_listener(video_ref, leptos::ev::loadedmetadata, move |_| {
            let video_ref = video_ref.get().unwrap();
            let canvas_ref = canvas_ref.get().unwrap();
            let cv_overlay_ref = cv_overlay_ref.get().unwrap();

            let video_width = f64::from(video_ref.video_width());
            let video_height = f64::from(video_ref.video_height());

            let aspect = video_height / video_width;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let height = (f64::from(WIDTH) * aspect).round() as u32;

            canvas_ref
                .set_attribute("width", &WIDTH.to_string())
                .unwrap();
            canvas_ref
                .set_attribute("height", &height.to_string())
                .unwrap();
            video_ref
                .set_attribute("width", &WIDTH.to_string())
                .unwrap();
            video_ref
                .set_attribute("height", &height.to_string())
                .unwrap();
            cv_overlay_ref
                .set_attribute("width", &WIDTH.to_string())
                .unwrap();
            cv_overlay_ref
                .set_attribute("height", &height.to_string())
                .unwrap();
            a.notify_one();
        });
    }

    let _ = use_event_listener(video_ref, leptos::ev::playing, move |_| {
        let playing_barrier = Arc::clone(&playing_barrier);
        spawn_local(async move {
            // let the camera exposure stabilize
            gloo_timers::future::TimeoutFuture::new(1000).await;
            playing_barrier.set_ready();
        });
    });

    // let camera_devices =
    //     LocalResource::new(move || async move { all_camera_devices().await.unwrap() });
    // let camera_device =
    //     LocalResource::new(move || async move { camera_devices.await.first().cloned() });

    // let select_camera_device = move |ev: Targeted<web_sys::Event, web_sys::HtmlSelectElement>| {
    //     let v = ev.target().value();
    //     let selected_camera_device = camera_devices
    //         .get()
    //         .unwrap()
    //         .iter()
    //         .find(|d| d.device_id() == v)
    //         .cloned();
    //     *camera_device.write() = Some(selected_camera_device);

    //     let a = web_sys::MediaTrackConstraints::default();

    //     let b = web_sys::ConstrainDomStringParameters::default();
    //     b.set_ideal(&JsValue::from_str("environment"));
    //     a.set_facing_mode(&b);

    //     let b = web_sys::ConstrainDomStringParameters::default();
    //     b.set_exact(&JsValue::from_str(&v));
    //     a.set_device_id(&b);

    //     let c = web_sys::MediaStreamConstraints::default();
    //     c.set_video(&a);

    //     spawn_local(async move {
    //         if let Some(device) = stream.get_untracked() {
    //             for track in device.unwrap().get_tracks() {
    //                 wasm_bindgen_futures::JsFuture::from(
    //                     track
    //                         .unchecked_ref::<web_sys::MediaStreamTrack>()
    //                         .apply_constraints_with_constraints(c.unchecked_ref())
    //                         .unwrap(),
    //                 )
    //                 .await
    //                 .unwrap();
    //             }
    //         }
    //     });
    // };

    #[cfg(feature = "hydrate")]
    {
        let a = Arc::clone(&a);
        spawn_local(async move {
            a.notified().await;
            loop {
                info!("1");
                if cv_available_rx.changed().await.is_err() {
                    break;
                }
                info!("2");
                let Some(cv_processor) = &*cv_available_rx.borrow_and_update() else {
                    continue;
                };
                info!("3");
                let pixel_assignment = cv_processor.pixel_assignment_locations();
                let mut overlay_data = vec![0u8; 4 * pixel_assignment.len()];
                let mut assigned_pixels_count = 0;
                for overlay_pixel_mut in overlay_data
                    .chunks_exact_mut(4)
                    .zip(pixel_assignment.iter())
                    .filter_map(|(overlay_pixel_mut, &assigned_pixel)| {
                        if assigned_pixel {
                            Some(overlay_pixel_mut)
                        } else {
                            None
                        }
                    })
                {
                    assigned_pixels_count += 1;
                    overlay_pixel_mut[0] = 255;
                    overlay_pixel_mut[1] = 0;
                    overlay_pixel_mut[2] = 255;
                    overlay_pixel_mut[3] = 255;
                }
                info!(
                    "Assigned {}/{} pixels",
                    assigned_pixels_count,
                    pixel_assignment.len()
                );
                let cv_overlay_ref = cv_overlay_ref.get_untracked().unwrap();
                let overlay_height = cv_overlay_ref.height();
                let overlay_width = cv_overlay_ref.width();
                assert_eq!(
                    overlay_height as usize * overlay_width as usize,
                    pixel_assignment.len()
                );

                let opts = js_sys::Object::new();
                js_sys::Reflect::set(&opts, &"willReadFrequently".into(), &true.into()).unwrap();
                js_sys::Reflect::set(&opts, &"alpha".into(), &true.into()).unwrap();
                let overlay_image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
                    Clamped(&overlay_data),
                    overlay_width,
                    overlay_height,
                )
                .unwrap();
                let ctx = cv_overlay_ref
                    .get_context_with_context_options("2d", &opts)
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                ctx.put_image_data(&overlay_image_data, 0.0, 0.0).unwrap();
            }
        });
    }

    view! {
      <div class="flex gap-4 justify-around">
        <div class="relative flex-1 min-w-0 border-2">
          <video
            node_ref=video_ref
            on:click=toggle_enabled
            controls=false
            autoplay=true
            muted=true
            class="w-full h-full border-white"
          />
          <canvas
            node_ref=cv_overlay_ref
            class="absolute top-0 left-0 w-full h-full pointer-events-none [image-rendering:pixelated]"
          />
        </div>
        <canvas node_ref=canvas_ref class="flex-1 min-w-0 border-2 border-amber-300" />
      </div>
    }
    // <select
    //   on:change:target=select_camera_device
    //   prop:value=move || camera_device.get().flatten().map(|d| d.device_id()).unwrap_or_default()
    //   class="cursor-pointer"
    // >
    //   <Suspense fallback=move || {
    //     view! { <option>"Loading..."</option> }
    //   }>
    //     {move || Suspend::new(async move {
    //       view! {
    //         {camera_devices
    //           .await
    //           .iter()
    //           .map(|device| {
    //             view! {
    //               <option value=device
    //                 .device_id()>
    //                 {if device.label().is_empty() {
    //                   format!("Unidentified: {}", device.device_id())
    //                 } else {
    //                   device.label()
    //                 }}
    //               </option>
    //             }
    //           })
    //           .collect::<Vec<_>>()}
    //       }
    //     })}
    //   </Suspense>
    // </select>
}
