use leptos::{prelude::*, server_fn::codec::GetUrl};
use puzzle_theory::permutations::Permutation;
use qvis::Pixel;
use serde::{Deserialize, Serialize};
use server_fn::codec::{MultipartData, MultipartFormData};

#[cfg(feature = "ssr")]
mod ssr_imports {
    pub use axum::extract::Query;
    pub use bytes::Bytes;
    pub use leptos::logging::log;
    pub use leptos_axum::extract;
    pub use leptos_ws::ChannelSignal;
    pub use log::warn;
    pub use qvis::Pixel;
    pub use std::sync::Mutex;
}

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

#[derive(Deserialize, Debug)]
struct TakePictureQuery {
    calibration_permutation: Option<String>,
}

#[server(
  endpoint = "take_picture",
  input = GetUrl,
)]
pub async fn take_picture() -> Result<Option<Permutation>, ServerFnError> {
    use ssr_imports::*;

    let query: Query<TakePictureQuery> = extract().await?;
    println!("Query: {:?}", &query);
    // if let Some(permutation) = &query.permutation {
    //     dbg!(permutation);
    //     let permutation = permutation
    //         .parse::<Permutation>()
    //         .map_err(ServerFnError::new)?;
    //     dbg!(permutation);
    // } else {
    //     dbg!("none");
    // }

    let channel = ChannelSignal::new(TAKE_PICTURE_CHANNEL).map_err(ServerFnError::new)?;

    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    let response_tx = Mutex::new(Some(response_tx));

    channel
        .on_server(move |message: &TakePictureMessage| {
            log!("Received message {message:#?}");
            let response_tx = response_tx
                .lock()
                .unwrap()
                .take()
                .expect("Expected to send only one response");
            match message {
                TakePictureMessage::PermutationResult(permutation) => {
                    response_tx.send(Some(permutation.clone())).unwrap();
                }
                TakePictureMessage::Calibrated => {
                    response_tx.send(None).unwrap();
                }
                m @ (TakePictureMessage::TakePicture | TakePictureMessage::Calibrate(_)) => {
                    warn!("Received {m:?} on server, which should not happen");
                }
            }
        })
        .map_err(ServerFnError::new)?;

    let message = if let Some(calibration_permutation) = &query.calibration_permutation {
        let permutation = calibration_permutation
            .parse::<Permutation>()
            .map_err(ServerFnError::new)?;
        TakePictureMessage::Calibrate(permutation)
    } else {
        TakePictureMessage::TakePicture
    };

    channel.send_message(message).map_err(ServerFnError::new)?;

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
}
