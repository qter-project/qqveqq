use opencv::{
    core::{CV_8UC1, Point, Rect, Scalar},
    highgui, imgcodecs,
    imgproc::{self, FLOODFILL_FIXED_RANGE, FLOODFILL_MASK_ONLY},
    prelude::*,
};
use std::sync::{Arc, Mutex};

const WINDOW_NAME: &str = "Qvis Sticker Calibration";

struct State {
    img: Mat,
    mask: Mat,
    displayed_img: Mat,
    maybe_drag_xy: Option<(i32, i32)>,
    err: Option<opencv::Error>,
}

fn mouse_callback(state: &mut State, event: i32, x: i32, y: i32) -> opencv::Result<()> {
    match event {
        highgui::EVENT_MOUSEMOVE => {
            let Some((drag_x, drag_y)) = state.maybe_drag_xy else {
                return Ok(());
            };
            let distance = ((x - drag_x).pow(2) + (y - drag_y).pow(2)).isqrt();
            let first = (distance / 2) as f64;
            let second = ((distance + 1) / 2) as f64;
 
            let mut a = Rect::default();
            state.mask.set_to_def(&Scalar::all(0.0))?;
            imgproc::flood_fill_mask(
                &mut state.img,
                &mut state.mask,
                Point::new(drag_x, drag_y),
                Scalar::default(), // ignored
                &mut a,
                Scalar::new(0.2 * first, 0.2 * first, 0.2 * first, 0.0),
                Scalar::new(0.2 * second, 0.2 * second, 0.2 * second, 0.0),
                8 | FLOODFILL_FIXED_RANGE | FLOODFILL_MASK_ONLY | (255 << 8),
            )?;
            let mask_roi = Rect::new(
                1,
                1,
                state.mask.cols() - 2,
                state.mask.rows() - 2,
            );
            let mask_cropped = Mat::roi(&state.mask, mask_roi)?;
            
            let mut red_mask = Mat::zeros(
                state.img.rows(),
                state.img.cols(),
                state.img.typ(),
            )?.to_mat()?;
            
            let mut channels = opencv::core::Vector::<Mat>::new();
            channels.push(Mat::zeros(state.img.rows(), state.img.cols(), CV_8UC1)?.to_mat()?);
            channels.push(Mat::zeros(state.img.rows(), state.img.cols(), CV_8UC1)?.to_mat()?);
            channels.push(mask_cropped.clone_pointee());
            opencv::core::merge(&channels, &mut red_mask)?;
            
            opencv::core::add_weighted_def(
                &state.img,
                1.0,
                &red_mask,
                0.5,
                0.0,
                &mut state.displayed_img,
            )?;
            highgui::imshow(WINDOW_NAME, &state.displayed_img)?;
        }
        highgui::EVENT_LBUTTONDOWN => {
            state.maybe_drag_xy = Some((x, y));
        }
        highgui::EVENT_LBUTTONUP => {
            state.maybe_drag_xy = None;
        }
        _ => (),
    }

    Ok(())
}

fn main() -> opencv::Result<()> {
    highgui::named_window(WINDOW_NAME, highgui::WINDOW_AUTOSIZE)?;

    let img = imgcodecs::imread("input.png", imgcodecs::IMREAD_COLOR)?;
    let displayed_img = Mat::zeros(
        img.rows(),
        img.cols(),
        img.typ(),
    )?.to_mat()?;
    let mask = Mat::zeros(img.rows() + 2, img.cols() + 2, CV_8UC1)?.to_mat()?;
    highgui::imshow(WINDOW_NAME, &img)?;

    let state = Arc::new(Mutex::new(State {
        img,
        mask,
        displayed_img,
        maybe_drag_xy: None,
        err: None,
    }));
    {
        let state = Arc::clone(&state);
        highgui::set_mouse_callback(
            WINDOW_NAME,
            Some(Box::new(move |event, x, y, _flags| {
                let mut state = state.lock().unwrap();
                if let Err(e) = mouse_callback(&mut state, event, x, y) {
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
