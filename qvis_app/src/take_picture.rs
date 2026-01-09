use leptos::{
    prelude::*,
    server_fn::codec::{GetUrl, Json},
};
use puzzle_theory::permutations::Permutation;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
mod ssr_imports {
    pub use std::sync::Mutex;
    pub use leptos::logging::log;
    pub use leptos_ws::ChannelSignal;
}

pub const TAKE_PICTURE_CHANNEL: &str = "take_picture_channel";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TakePictureMessage {
    TakePicture,
    PictureResult(Result<Permutation, ServerFnError>),
}

#[server(
  endpoint = "take_picture",
  input = GetUrl,
  output = Json
)]
pub async fn take_picture() -> Result<Permutation, ServerFnError> {
    use ssr_imports::*;
    
    let channel = ChannelSignal::new(TAKE_PICTURE_CHANNEL).map_err(ServerFnError::new)?;

    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));

    channel
        .on_server(move |message: &TakePictureMessage| {
            log!("Recieved message {message:#?}");
            if let TakePictureMessage::PictureResult(result) = message
                && let Some(sender) = tx.lock().unwrap().take()
            {
                sender.send(result.clone()).unwrap();
            }
        })
        .map_err(ServerFnError::new)?;

    channel
        .send_message(TakePictureMessage::TakePicture)
        .map_err(ServerFnError::new)?;

    rx.await.map_err(ServerFnError::new)?
}
