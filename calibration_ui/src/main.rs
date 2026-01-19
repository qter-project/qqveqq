use opencv::{
    core::{Point, Scalar},
    highgui, imgcodecs, imgproc,
    prelude::*,
};
use std::sync::{Arc, Mutex};

const WINDOW_NAME: &str = "Qvis Sticker Calibration";

struct State {
    img: Mat,
    err: Option<opencv::Error>,
}

fn on_mousemove(state: &mut State, event: i32, x: i32, y: i32, flags: i32) -> opencv::Result<()> {
    println!("{:?}", flags);
    if event == highgui::EVENT_FLAG_LBUTTON {
        imgproc::circle(
            &mut state.img,
            Point::new(x, y),
            100,
            Scalar::new(255.0, 0.0, 0.0, 0.0),
            -1,
            imgproc::LINE_8,
            0,
        )?;
    }
    // MouseEventFlags::EVENT_FLAG_LBUTTON

    highgui::imshow(WINDOW_NAME, &state.img)?;
    Ok(())
}

fn main() -> opencv::Result<()> {
    highgui::named_window(WINDOW_NAME, highgui::WINDOW_AUTOSIZE)?;

    let img = imgcodecs::imread("input.png", imgcodecs::IMREAD_COLOR)?;
    highgui::imshow(WINDOW_NAME, &img)?;

    let state = Arc::new(Mutex::new(State { img, err: None }));
    {
        let state = Arc::clone(&state);
        highgui::set_mouse_callback(
            WINDOW_NAME,
            Some(Box::new(move |event, x, y, flags| {
                let mut state = state.lock().unwrap();
                if let Err(e) = on_mousemove(&mut state, event, x, y, flags) {
                    state.err = Some(e);
                }
            })),
        )?;
    }

    loop {
        if let Some(err) = state.lock().unwrap().err.take() {
            return Err(err);
        }
        if highgui::wait_key(1000 / 30)? == 27 {
            break;
        }
    }

    Ok(())
}
