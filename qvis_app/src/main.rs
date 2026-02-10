use axum::{
    Router,
    body::Body as AxumBody,
    extract::{FromRef, Path, RawQuery, State},
    http::{HeaderMap, Request},
    response::{IntoResponse, Response as AxumResponse},
    routing::{get, post},
};
use bytes::Bytes;
use leptos::prelude::*;
use leptos_axum::{
    AxumRouteListing, LeptosRoutes, file_and_error_handler_with_context,
    generate_route_list_with_exclusions_and_ssg_and_context, handle_server_fns_with_context,
};
use leptos_ws::{ChannelSignal, WsSignals};
use log::{info, warn};
use puzzle_theory::{permutations::Permutation, puzzle_geometry::parsing::puzzle};
use qvis::Pixel;
use qvis_app::{
    app::{App, TAKE_PICTURE_CHANNEL, TakePictureMessage, shell},
    pixel_assignment_ui,
};
use std::{sync::Mutex, thread};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[derive(Clone, FromRef)]
pub struct AppState {
    server_signals: WsSignals,
    routes: Option<Vec<AxumRouteListing>>,
    options: LeptosOptions,
    pixel_assignment_ui_tx:
        std::sync::mpsc::Sender<(tokio::sync::oneshot::Sender<Box<[Pixel]>>, Bytes)>,
}

async fn server_fn_handler(
    State(state): State<AppState>,
    _path: Path<String>,
    _headers: HeaderMap,
    _query: RawQuery,
    request: Request<AxumBody>,
) -> impl IntoResponse {
    handle_server_fns_with_context(
        move || {
            provide_context(state.options.clone());
            provide_context(state.server_signals.clone());
            provide_context(state.pixel_assignment_ui_tx.clone());
        },
        request,
    )
    .await
}

async fn leptos_routes_handler(state: State<AppState>, req: Request<AxumBody>) -> AxumResponse {
    let state1 = state.0.clone();
    let options1 = state.0.options.clone();
    let handler = leptos_axum::render_route_with_context(
        state.routes.clone().unwrap(),
        move || {
            provide_context(state1.options.clone());
            provide_context(state1.server_signals.clone());
        },
        move || shell(options1.clone()),
    );
    handler(state, req).await.into_response()
}

#[tokio::main]
async fn server_main(
    pixel_assignment_ui_tx: std::sync::mpsc::Sender<(
        tokio::sync::oneshot::Sender<Box<[Pixel]>>,
        Bytes,
    )>,
) {
    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;

    let mut server_signals = WsSignals::new();
    let server_signals2 = server_signals.clone();
    let server_signals3 = server_signals.clone();
    let (routes, _) = generate_route_list_with_exclusions_and_ssg_and_context(
        || view! { <App /> },
        None,
        move || provide_context(server_signals2.clone()),
    );
    let state = AppState {
        options: leptos_options.clone(),
        routes: Some(routes.clone()),
        server_signals: server_signals.clone(),
        pixel_assignment_ui_tx,
    };

    let app = Router::new()
        .route(
            "/api/{*fn_name}",
            post(server_fn_handler).get(server_fn_handler),
        )
        .leptos_routes_with_handler(routes, get(leptos_routes_handler))
        .fallback(file_and_error_handler_with_context::<AppState, _>(
            move || provide_context(server_signals3.clone()),
            shell,
        ))
        .with_state(state);

    tokio::spawn(async move {
        robot_tui(&mut server_signals).await;
    });

    info!("listening on {addr}");
    let listener = TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

async fn robot_tui(server_signals: &mut WsSignals) {
    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Ok(Some(line)) = stdin.next_line().await {
        if line.starts_with("TAKE_PICTURE") {
            let done_string = take_picture(server_signals, None)
                .await
                .map(|p| p.unwrap().to_string())
                .unwrap_or_else(|e| e.to_string());
            stdout
                .write_all(format!("DONE {done_string}\n").as_bytes())
                .await
                .unwrap();
        } else if line.starts_with("CALIBRATE") {
            let perm_str = line.trim_start_matches("CALIBRATE").trim();
            let done_string = if let Ok(permutation) = perm_str.parse::<Permutation>() {
                take_picture(server_signals, Some(permutation))
                    .await
                    .map(|n| {
                        assert!(n.is_none());
                        String::new()
                    })
                    .unwrap_or_else(|e| e.to_string())
            } else {
                format!("Invalid permutation string: {perm_str}")
            };

            stdout
                .write_all(format!("DONE {done_string}\n").as_bytes())
                .await
                .unwrap();
        } else {
            leptos::logging::log!("WARNING: Unknown command: {}", line);
        }
    }
}

async fn take_picture(
    server_signals: &mut WsSignals,
    calibration_permutation: Option<Permutation>,
) -> Result<Option<Permutation>, ServerFnError> {
    let channel = ChannelSignal::new_with_context(server_signals, TAKE_PICTURE_CHANNEL)
        .map_err(ServerFnError::new)?;

    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    let response_tx = Mutex::new(Some(response_tx));

    channel
        .on_server(move |message: &TakePictureMessage| {
            info!("Received message {message:#?}");
            if let Some(response_tx) = response_tx.lock().unwrap().take() {
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
            } else {
                warn!("Received message {message:#?} but response channel was already used. This task will likely hang now.");
            }
        })
        .map_err(ServerFnError::new)?;

    let message = if let Some(calibration_permutation) = calibration_permutation {
        TakePictureMessage::Calibrate(calibration_permutation)
    } else {
        TakePictureMessage::TakePicture
    };

    channel.send_message(message).map_err(ServerFnError::new)?;

    response_rx.await.map_err(ServerFnError::new)
}

fn main() {
    let (pixel_assignment_ui_tx, pixel_assignment_ui_rx) =
        std::sync::mpsc::channel::<(tokio::sync::oneshot::Sender<Box<[Pixel]>>, Bytes)>();

    thread::spawn(move || server_main(pixel_assignment_ui_tx));

    // For some reason highgui doesn't work unless it's on the main thread
    let puzzle_geometry = puzzle("3x3");
    while let Ok((pixel_assignment_done_tx, image)) = pixel_assignment_ui_rx.recv() {
        let assignment =
            pixel_assignment_ui::pixel_assignment_ui(&puzzle_geometry, &image).unwrap();
        pixel_assignment_done_tx.send(assignment).unwrap();
    }
}
