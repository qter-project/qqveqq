use bytes::Bytes;
use leptos::{prelude::*, server_fn::codec::GetUrl};
use log::warn;
use puzzle_theory::permutations::Permutation;
use qvis::Pixel;
use serde::{Deserialize, Serialize};
use server_fn::codec::{MultipartData, MultipartFormData};

#[cfg(feature = "ssr")]
mod ssr_imports {
    pub use leptos::logging::log;
    pub use leptos_ws::ChannelSignal;
    pub use qvis::Pixel;
    pub use std::sync::Mutex;
}

pub const TAKE_PICTURE_CHANNEL: &str = "take_picture_channel";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TakePictureMessage {
    TakePicture,
    PermutationResult(Permutation),
}

#[server(
  endpoint = "take_picture",
  input = GetUrl,
)]
pub async fn take_picture() -> Result<Permutation, ServerFnError> {
    use ssr_imports::*;

    let channel = ChannelSignal::new(TAKE_PICTURE_CHANNEL).map_err(ServerFnError::new)?;

    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    let response_tx = Mutex::new(Some(response_tx));

    channel
        .on_server(move |message: &TakePictureMessage| {
            log!("Recieved message {message:#?}");
            match message {
                TakePictureMessage::PermutationResult(permutation) => {
                    response_tx.lock().unwrap().take().expect("Expected to send only one response").send(permutation.clone()).unwrap();
                }
                TakePictureMessage::TakePicture => {
                    warn!("Received TakePictureMessage::TakePicture on server, which should not happen");
                }
            }
        })
        .map_err(ServerFnError::new)?;

    channel
        .send_message(TakePictureMessage::TakePicture)
        .map_err(ServerFnError::new)?;

    response_rx.await.map_err(ServerFnError::new)
}

#[server(
  input = MultipartFormData,
)]
pub async fn pixel_assignment(data: MultipartData) -> Result<Box<[Pixel]>, ServerFnError> {
    use ssr_imports::*;

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

    // let pixel_assignment_ui_tx = pixel_assignment_ui_tx.clone();
    // let response_tx = response_tx
    //     .lock()
    //     .unwrap()
    //     .take()
    //     .expect("Expected to send only one response");

    // tokio::task::spawn(async move {

    //     // std::fs::write(
    //     //     "pixel_assignment.txt",
    //     //     format!("{pixel_assignment:?}"),
    //     // ).unwrap();
    //     response_tx
    //         .send(todo!())
    //         .unwrap();
    // });
}
