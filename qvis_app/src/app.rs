use crate::{
    messages_logger::MessagesLogger,
    server_fns::{TAKE_PICTURE_CHANNEL, TakePictureMessage, pixel_assignment},
    video::{Video, pixel_assignment_command, take_picture_command},
};
use leptos::{html, prelude::*, task::spawn_local};
use leptos_ws::ChannelSignal;
use log::{LevelFilter, info, warn};
use puzzle_theory::puzzle_geometry::parsing::puzzle;
use qvis::{CVProcessor, Pixel};
use std::sync::Arc;
use web_sys::FormData;

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
            warn!("Lost connection with server");
        });
        context.set_on_reconnect(move || {
            info!("Re-established connection with server");
        });
    }

    let messages_container = NodeRef::<leptos::html::Div>::new();
    let (overflowing, set_overflowing) = signal(true);
    let puzzle_geometry = puzzle("3x3");
    let video_ref = NodeRef::<html::Video>::new();
    let canvas_ref = NodeRef::<html::Canvas>::new();
    let (tx, rx) = tokio::sync::watch::channel(None::<CVProcessor>);

    let take_picture_channel = ChannelSignal::new(TAKE_PICTURE_CHANNEL).unwrap();
    let take_picture_channel2 = take_picture_channel.clone();

    let pixel_assignment_action =
        Action::new_local(|data: &FormData| pixel_assignment(data.clone().into()));

    let do_pixel_assignment = move || {
        let video_ref = video_ref.get_untracked().unwrap();
        let canvas_ref = canvas_ref.get_untracked().unwrap();
        spawn_local(async move {
            let blob = match pixel_assignment_command(&video_ref, &canvas_ref).await {
                Ok(blob) => blob,
                Err(e) => {
                    warn!("Failed to capture image: {e:?}");
                    return;
                }
            };
            let form_data = FormData::new().unwrap();
            form_data.append_with_blob("qvis_picture", &blob).unwrap();
            pixel_assignment_action.dispatch_local(form_data);
        });
    };

    take_picture_channel
        .on_client(move |msg: &TakePictureMessage| {
            let video_ref = video_ref.get_untracked().unwrap();
            let canvas_ref = canvas_ref.get_untracked().unwrap();
            info!("Received message {msg:?}");
            let TakePictureMessage::TakePicture = msg else {
                return;
            };

            let pixels = take_picture_command(&video_ref, &canvas_ref);
            let take_picture_channel2 = take_picture_channel2.clone();
            let mut rx = rx.clone();

            spawn_local(async move {
                if rx.borrow().is_none() {
                    do_pixel_assignment();
                    rx.changed().await.unwrap();
                }
                let cv_processor = rx.borrow();
                let cv_processor = cv_processor.as_ref().unwrap();
                let permutation = cv_processor.process_image(pixels).0;
                take_picture_channel2
                    .send_message(TakePictureMessage::PermutationResult(permutation))
                    .unwrap();
            });
        })
        .unwrap();

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
        tx.send_modify(|maybe_cv_processor| {
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
        <Video video_ref canvas_ref pixel_assignment_action do_pixel_assignment/>
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
            <ul class="pl-4 list-disc list-inside">
              <For each=move || messages.get() key=|msg| msg.0 let((_, msg))>
                <li>{msg}</li>
              </For>
            </ul>
          </div>
        </div>
      </main>
    }
}
