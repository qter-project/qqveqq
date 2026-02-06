use crate::{
    messages_logger::MessagesLogger,
    video::{OnceBarrier, Video, pixel_assignment_command, take_picture_command},
};
use bytes::Bytes;
use leptos::{html, prelude::*, task::spawn_local};
use leptos_use::{
    ConstraintExactIdeal, FacingMode, UseUserMediaOptions, VideoTrackConstraints,
    use_user_media_with_options,
};
use leptos_ws::ChannelSignal;
use log::{LevelFilter, info, warn};
use puzzle_theory::{permutations::Permutation, puzzle_geometry::parsing::puzzle};
use qvis::{CVProcessor, Pixel};
use serde::{Deserialize, Serialize};
use server_fn::codec::{MultipartData, MultipartFormData};
use std::sync::Arc;

pub const TAKE_PICTURE_CHANNEL: &str = "take_picture_channel";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TakePictureMessage {
    // Request
    TakePicture,
    Calibrate(Permutation),
    // Response
    // TODO:
    // QvisAppError,
    PermutationResult(Permutation),
    Calibrated,
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
      <!DOCTYPE html>
      <html lang="en">
        <head>
          <meta charset="utf-8" />
          <title>Cube Vision</title>
          <meta name="viewport" content="width=device-width, initial-scale=1" />
          <link rel="shortcut icon" href="favicon.ico" type="image/x-icon" />
          <link rel="stylesheet" id="leptos" href="/pkg/qvis_app.css" />
          <AutoReload options=options.clone() />
          <HydrationScripts options />
        </head>
        <body class="text-white bg-black">
          <App />
        </body>
      </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    let (messages, set_messages) = signal(Vec::<(u32, String)>::new());
    let logger = Box::leak(Box::new(MessagesLogger::new(set_messages)));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Debug);
    }

    leptos_ws::provide_websocket();
    #[cfg(feature = "hydrate")]
    {
        let context = expect_context::<leptos_ws::ServerSignalWebSocket>();
        context.set_on_connect(move || {
            info!("Established connection with server");
        });
        context.set_on_disconnect(move || {
            warn!("Lost connection with server; trying reconnect");
        });
        context.set_on_reconnect(move || {
            info!("Re-established connection with server");
        });
    }

    let use_user_media_return = use_user_media_with_options(UseUserMediaOptions::default().video(
        VideoTrackConstraints::default().facing_mode(ConstraintExactIdeal::ExactIdeal {
            exact: None,
            ideal: Some(FacingMode::Environment),
        }),
    ));

    let messages_container = NodeRef::<leptos::html::Div>::new();
    let video_ref = NodeRef::<html::Video>::new();
    let canvas_ref = NodeRef::<html::Canvas>::new();
    let (overflowing, set_overflowing) = signal(true);
    let (cv_available_tx, cv_available_rx) = tokio::sync::watch::channel(None::<CVProcessor>);
    let playing_barrier = OnceBarrier::new();
    let puzzle_geometry = puzzle("3x3");

    let take_picture_channel = ChannelSignal::new(TAKE_PICTURE_CHANNEL).unwrap();
    let take_picture_channel2 = take_picture_channel.clone();

    let pixel_assignment_action =
        Action::new_local(|data: &web_sys::FormData| pixel_assignment(data.clone().into()));

    let do_pixel_assignment = {
        let playing_barrier = Arc::clone(&playing_barrier);
        move || {
            let video_ref = video_ref.get_untracked().unwrap();
            let canvas_ref = canvas_ref.get_untracked().unwrap();
            use_user_media_return.set_enabled.set(true);
            let playing_barrier = Arc::clone(&playing_barrier);
            spawn_local(async move {
                let blob = match pixel_assignment_command(&video_ref, &canvas_ref, &playing_barrier)
                    .await
                {
                    Ok(blob) => blob,
                    Err(e) => {
                        warn!("Failed to capture image: {e:?}");
                        return;
                    }
                };
                let form_data = web_sys::FormData::new().unwrap();
                form_data.append_with_blob("qvis_picture", &blob).unwrap();
                pixel_assignment_action.dispatch_local(form_data);
            });
        }
    };
    {
        let cv_available_tx = cv_available_tx.clone();
        let playing_barrier = Arc::clone(&playing_barrier);
        let do_pixel_assignment = do_pixel_assignment.clone();
        take_picture_channel
            .on_client(move |msg: &TakePictureMessage| {
                let video_ref = video_ref.get_untracked().unwrap();
                let canvas_ref = canvas_ref.get_untracked().unwrap();
                info!("Received message {msg:?}");

                let take_picture_channel2 = take_picture_channel2.clone();
                let mut cv_available_rx = cv_available_rx.clone();
                let cv_available_tx = cv_available_tx.clone();
                match msg {
                    TakePictureMessage::TakePicture => {
                        let playing_barrier = Arc::clone(&playing_barrier);
                        let do_pixel_assignment = do_pixel_assignment.clone();
                        spawn_local(async move {
                            let pixels =
                                take_picture_command(&video_ref, &canvas_ref, &playing_barrier)
                                    .await;
                            if cv_available_rx.borrow().is_none() {
                                do_pixel_assignment();
                                cv_available_rx.changed().await.unwrap();
                            }
                            let cv_processor = cv_available_rx.borrow();
                            let cv_processor = cv_processor.as_ref().unwrap();
                            let (permutation, confidence) = cv_processor.process_image(pixels);
                            info!(
                                "Processed {permutation} with confidence {:.1}%",
                                confidence * 100.0
                            );
                            take_picture_channel2
                                .send_message(TakePictureMessage::PermutationResult(permutation))
                                .unwrap();
                        });
                    }
                    TakePictureMessage::Calibrate(permutation) => {
                        let permutation = permutation.clone();
                        let playing_barrier = Arc::clone(&playing_barrier);
                        let do_pixel_assignment = do_pixel_assignment.clone();
                        spawn_local(async move {
                            let pixels =
                                take_picture_command(&video_ref, &canvas_ref, &playing_barrier)
                                    .await;
                            if cv_available_rx.borrow().is_none() {
                                do_pixel_assignment();
                                cv_available_rx.changed().await.unwrap();
                            }
                            cv_available_tx.send_modify(|maybe_cv_processor| {
                                let cv_processor = maybe_cv_processor.as_mut().unwrap();
                                cv_processor.calibrate(&pixels, &permutation);
                            });
                            take_picture_channel2
                                .send_message(TakePictureMessage::Calibrated)
                                .unwrap();
                        });
                    }
                    m @ (TakePictureMessage::PermutationResult(_)
                    | TakePictureMessage::Calibrated) => {
                        warn!("Received {m:?} on client, which should not happen");
                    }
                }
            })
            .unwrap();
    }

    Effect::new(move |_| {
        let pixel_assignment = pixel_assignment_action.value().get();
        let Some(pixel_assignment) = pixel_assignment else {
            return;
        };
        let pixel_assignment = match pixel_assignment {
            Ok(pixels) => pixels,
            Err(err) => {
                warn!("Pixel assignment failed: {err}");
                return;
            }
        };
        let assigned = pixel_assignment
            .iter()
            .filter(|p| !matches!(p, Pixel::Unassigned))
            .count();
        info!("Assigned {}/{} pixels", assigned, pixel_assignment.len());

        let cv_processor = CVProcessor::new(
            Arc::clone(&puzzle_geometry),
            pixel_assignment.len(),
            pixel_assignment,
        );
        cv_available_tx.send_modify(|maybe_cv_processor| {
            *maybe_cv_processor = Some(cv_processor);
        });
    });

    Effect::watch(
        move || messages.get(),
        move |_, _, _| {
            let Some(container) = messages_container.get_untracked() else {
                return;
            };
            let scroll_height = container.scroll_height();
            let client_height = container.client_height();
            set_overflowing.set(scroll_height > client_height);
            container.set_scroll_top(scroll_height);
        },
        false,
    );

    view! {
      <header class="mb-5 font-sans text-4xl font-bold tracking-wider text-center bg-[rgb(47,48,80)] leading-20">
        <button
          on:click=move |_| {
            location().reload().unwrap();
          }
          class="cursor-pointer"
        >
          "QVIS"
        </button>
      </header>
      <main class="flex flex-col gap-4 justify-center mr-4 ml-4 text-center">
        <Video video_ref canvas_ref pixel_assignment_action do_pixel_assignment use_user_media_return playing_barrier />
        "Messages:"
        <div class="relative h-72 font-mono text-left border-2 border-gray-300">
          <div
            class:hidden=move || !overflowing.get()
            class="absolute top-0 left-0 right-3 h-5 from-black to-transparent pointer-events-none bg-linear-to-b"
          />
          <div
            node_ref=messages_container
            class="overflow-y-auto h-full [&::-webkit-scrollbar]:w-3 [&::-webkit-scrollbar-thumb]:bg-white"
          >
            <ul class="pl-4 list-disc list-inside whitespace-pre-wrap">
              <For each=move || messages.get() key=|msg| msg.0 let((_, msg))>
                <li>{msg}</li>
              </For>
            </ul>
          </div>
        </div>
      </main>
    }
}

#[server(
  input = MultipartFormData,
)]
pub async fn pixel_assignment(data: MultipartData) -> Result<Box<[Pixel]>, ServerFnError> {
    let mut data = data.into_inner().unwrap();
    let field = data
        .next_field()
        .await
        .map_err(ServerFnError::new)?
        .unwrap();
    let bytes = field.bytes().await?;

    let pixel_assignment_ui_tx = use_context::<
        std::sync::mpsc::Sender<(tokio::sync::oneshot::Sender<Box<[Pixel]>>, Bytes)>,
    >()
    .unwrap();
    let (pixel_assignment_done_tx, pixel_assignment_done_rx) = tokio::sync::oneshot::channel();
    pixel_assignment_ui_tx
        .send((pixel_assignment_done_tx, bytes))
        .unwrap();
    let pixel_assignment = pixel_assignment_done_rx.await.unwrap();

    Ok(pixel_assignment)
}
