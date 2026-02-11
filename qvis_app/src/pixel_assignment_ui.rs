use bytes::Bytes;
use internment::ArcIntern;
use opencv::{
    core::{BORDER_CONSTANT, CV_8UC1, CV_8UC3, Point, Rect, Scalar, Size, Vec3b},
    highgui::{self, EVENT_LBUTTONUP},
    imgcodecs::{self, IMREAD_COLOR},
    imgproc::{self, FILLED, FLOODFILL_FIXED_RANGE, FLOODFILL_MASK_ONLY, LINE_8, MORPH_ELLIPSE},
    prelude::*,
};
use puzzle_theory::puzzle_geometry::{Face, PuzzleGeometry};
use qvis::Pixel;
use rand::{SeedableRng, rngs::SmallRng, seq::SliceRandom};
use std::{
    cmp::Ordering,
    f64::consts::PI,
    sync::{Arc, Mutex},
};

const WINDOW_NAME: &str = "Qvis Sticker Assignment";
const EROSION_SIZE_TRACKBAR_NAME: &str = "Erosion size";
const EROSION_SIZE_TRACKBAR_MINDEFMAX: [i32; 3] = [2, 4, 20];
const UPPER_DIFF_TRACKBAR_NAME: &str = "Upper diff";
const UPPER_DIFF_TRACKBAR_MINDEFMAX: [i32; 3] = [0, 2, 5];
const GUI_SCALE_TRACKBAR_NAME: &str = "GUI Scale";
const GUI_SCALE_TRACKBAR_MINDEFMAX: [i32; 3] = [6, 11, 18];
const SUBMIT_BUTTON_NAME: &str = "Assign sticker";
const BACK_BUTTON_NAME: &str = "Back";
const EROSION_KERNEL_MORPH_SHAPE: i32 = MORPH_ELLIPSE;
const DEF_ANCHOR: Point = Point::new(-1, -1);
const RECTANGLE_DEF_SHIFT: i32 = 0;
const MAX_PIXEL_VALUE: i32 = 255;
const ERODE_UNTIL_PERCENT: (i32, i32) = (1, 3);
const MIN_SAMPLES: i32 = 30;
const NUM_QVIS_PIXELS: usize = 20;

enum UIState {
    OpenCVError(opencv::Error),
    Assigning,
    Finished,
}

#[derive(Debug)]
enum CropState {
    NoCrop,
    SelectingCrop(Rect),
    SelectedCrop(Rect),
    Crop((Rect, Mat)),
}

struct State {
    img: Mat,
    tmp_mask: Mat,
    grayscale_mask: Mat,
    samples: Vec<usize>,
    cleaned_grayscale_mask: Mat,
    eroded_grayscale_mask: Mat,
    erosion_kernel: Mat,
    erosion_kernel_times_two: Mat,
    displayed_img: Mat,
    pixel_assignment: Box<[Pixel]>,
    pixel_assignment_mask: Mat,
    stickers_to_assign: Vec<(Face, Vec<ArcIntern<str>>)>,
    white_balances_to_assign: Vec<Face>,
    assigning_sticker_idx: usize,
    assigning_white_balance_idx: usize,
    gui_scale: f64,
    upper_flood_fill_diff: i32,
    maybe_drag_origin: Option<(i32, i32)>,
    maybe_drag_xy: Option<(i32, i32)>,
    maybe_xy: Option<(i32, i32)>,
    dragging: bool,
    crop: CropState,
    ui: UIState,
}

impl State {
    #[allow(clippy::cast_possible_truncation)]
    fn xy_circle_radius(&self) -> i32 {
        (5.0 * self.gui_scale).round() as i32
    }

    #[allow(clippy::cast_possible_truncation)]
    fn xy_line_thickness(&self) -> i32 {
        (3.0 * self.gui_scale).round() as i32
    }
}

fn c(x: i32, n: i32) -> i32 {
    (x + n) / 6
}

fn perm6_from_number(mut n: u16) -> [i32; 6] {
    const FACT: [u16; 7] = [1, 1, 2, 6, 24, 120, 720];
    n %= FACT[6];

    let mut elems = vec![0, 1, 2, 3, 4, 5];
    let mut result = [0; 6];

    for i in 0..6 {
        let f = FACT[5 - i];
        let idx = (n / f) as usize;
        n %= f;

        result[i] = elems.remove(idx);
    }

    result
}

#[allow(clippy::cast_sign_loss)]
fn outer_index_to_inner_index(outer: &Mat, inner: &Rect, outer_index: usize) -> Option<usize> {
    let outer_cols = outer.cols() as usize;
    let inner_x = inner.x as usize;
    let inner_y = inner.y as usize;
    let inner_rows = inner.height as usize;
    let inner_cols = inner.width as usize;

    let outer_row = outer_index / outer_cols;
    let outer_col = outer_index % outer_cols;
    let inner_row = outer_row.checked_sub(inner_y)?;
    let inner_col = outer_col.checked_sub(inner_x)?;

    if inner_row >= inner_rows || inner_col >= inner_cols {
        return None;
    }

    let ret = inner_row * inner_cols + inner_col;
    assert!(ret < inner_cols * inner_rows);
    Some(ret)
}

#[allow(clippy::cast_sign_loss)]
fn inner_index_to_outer_index(outer: &Mat, inner: &Rect, inner_index: usize) -> Option<usize> {
    let outer_rows = outer.rows() as usize;
    let outer_cols = outer.cols() as usize;
    let inner_x = inner.x as usize;
    let inner_y = inner.y as usize;
    let inner_cols = inner.width as usize;
    let inner_rows = inner.height as usize;

    if inner_index >= inner_cols * inner_rows {
        return None;
    }

    let inner_row = inner_index / inner_cols;
    let inner_col = inner_index % inner_cols;

    let outer_row = inner_y + inner_row;
    let outer_col = inner_x + inner_col;

    let outer_index = outer_row * outer_cols + outer_col;
    assert!(outer_index < outer_rows * outer_cols);
    Some(outer_index)
}

fn update_floodfill_display(state: &mut State) -> opencv::Result<()> {
    let maybe_cropped_img = match &mut state.crop {
        CropState::SelectedCrop(_) | CropState::SelectingCrop(_) | CropState::NoCrop => {
            &mut state.img
        }
        CropState::Crop((_, cropped_img)) => cropped_img,
    };
    let mask_roi = Rect::new(1, 1, maybe_cropped_img.cols(), maybe_cropped_img.rows());
    maybe_cropped_img.copy_to(&mut state.displayed_img)?;
    let ran;
    let mut nonzeroes: Vec<usize>;
    if let Some((drag_origin_x, drag_origin_y)) = state.maybe_drag_origin
        && let Some((drag_x, drag_y)) = state.maybe_drag_xy
    {
        ran = true;
        #[allow(clippy::cast_possible_truncation)]
        let distance = (f64::from(drag_x - drag_origin_x)
            .hypot(f64::from(drag_y - drag_origin_y))
            .powf(1.5)
            / 15.0) as i32;
        // angle is between [-pi, pi]; add pi and multiply by 360/pi to get a range
        // of [0, 720] throughout the full circle which is 6!
        //
        // multiply it again by 20 to increase the periodicity
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let angle = (f64::from(drag_y - drag_origin_y).atan2(f64::from(drag_x - drag_origin_x))
            + PI * 360.0 / PI * 20.0) as u16;
        let perm6 = perm6_from_number(angle);

        Mat::roi_mut(&mut state.grayscale_mask, mask_roi)?.set_to_def(&Scalar::all(0.0))?;
        imgproc::flood_fill_mask(
            maybe_cropped_img,
            &mut state.grayscale_mask,
            Point::new(drag_origin_x, drag_origin_y),
            Scalar::default(), // ignored
            &mut Rect::default(),
            Scalar::from((
                c(distance, perm6[0]),
                c(distance, perm6[1]),
                c(distance, perm6[2]),
            )),
            Scalar::from((
                c(
                    distance,
                    perm6[3]
                        + state.upper_flood_fill_diff * MAX_PIXEL_VALUE
                            / UPPER_DIFF_TRACKBAR_MINDEFMAX[2],
                ),
                c(
                    distance,
                    perm6[4]
                        + state.upper_flood_fill_diff * MAX_PIXEL_VALUE
                            / UPPER_DIFF_TRACKBAR_MINDEFMAX[2],
                ),
                c(
                    distance,
                    perm6[5]
                        + state.upper_flood_fill_diff * MAX_PIXEL_VALUE
                            / UPPER_DIFF_TRACKBAR_MINDEFMAX[2],
                ),
            )),
            4 | FLOODFILL_FIXED_RANGE | FLOODFILL_MASK_ONLY | (MAX_PIXEL_VALUE << 8),
        )?;

        imgproc::erode(
            &state.grayscale_mask,
            &mut state.cleaned_grayscale_mask,
            &state.erosion_kernel,
            DEF_ANCHOR,
            2,
            BORDER_CONSTANT,
            imgproc::morphology_default_border_value()?,
        )?;
        if opencv::core::has_non_zero(&Mat::roi(&state.cleaned_grayscale_mask, mask_roi)?)? {
            *state
                .cleaned_grayscale_mask
                .at_2d_mut::<u8>(drag_origin_y + 1, drag_origin_x + 1)? =
                MAX_PIXEL_VALUE.try_into().unwrap();

            Mat::roi_mut(&mut state.tmp_mask, mask_roi)?.set_to_def(&Scalar::all(0.0))?;
            let mut cleaned_grayscale_mask_cropped_mut =
                Mat::roi_mut(&mut state.cleaned_grayscale_mask, mask_roi)?;
            imgproc::flood_fill_mask(
                &mut cleaned_grayscale_mask_cropped_mut,
                &mut state.tmp_mask,
                Point::new(drag_origin_x, drag_origin_y),
                Scalar::default(), // ignored
                &mut Rect::default(),
                Scalar::all(0.0),
                Scalar::all(0.0),
                4 | FLOODFILL_FIXED_RANGE | FLOODFILL_MASK_ONLY | (MAX_PIXEL_VALUE << 8),
            )?;
            std::mem::swap(&mut state.cleaned_grayscale_mask, &mut state.tmp_mask);

            state
                .cleaned_grayscale_mask
                .roi_mut(Rect::new(0, 0, state.cleaned_grayscale_mask.cols(), 1))?
                .set_to_def(&Scalar::all(0.0))?;
            state
                .cleaned_grayscale_mask
                .roi_mut(Rect::new(
                    0,
                    state.cleaned_grayscale_mask.rows() - 1,
                    state.cleaned_grayscale_mask.cols(),
                    1,
                ))?
                .set_to_def(&Scalar::all(0.0))?;
            state
                .cleaned_grayscale_mask
                .roi_mut(Rect::new(0, 1, 1, state.cleaned_grayscale_mask.rows() - 2))?
                .set_to_def(&Scalar::all(0.0))?;
            state
                .cleaned_grayscale_mask
                .roi_mut(Rect::new(
                    state.cleaned_grayscale_mask.cols() - 1,
                    1,
                    1,
                    state.cleaned_grayscale_mask.rows() - 2,
                ))?
                .set_to_def(&Scalar::all(0.0))?;
            // For some reason dilation doesn't work on ROIs
            imgproc::dilate(
                &state.cleaned_grayscale_mask,
                &mut state.tmp_mask,
                &state.erosion_kernel_times_two,
                DEF_ANCHOR,
                1,
                BORDER_CONSTANT,
                imgproc::morphology_default_border_value()?,
            )?;
            std::mem::swap(&mut state.cleaned_grayscale_mask, &mut state.tmp_mask);
        } else {
            std::mem::swap(&mut state.cleaned_grayscale_mask, &mut state.grayscale_mask);
        }

        let og_num_pixels = opencv::core::count_non_zero(&state.cleaned_grayscale_mask)?;
        let mut erosion_count = 0;
        loop {
            let has_eroded_enough = |to_check| -> Result<bool, opencv::Error> {
                let current_num_pixels = opencv::core::count_non_zero(to_check)?;
                Ok(current_num_pixels
                    <= og_num_pixels * ERODE_UNTIL_PERCENT.0 / ERODE_UNTIL_PERCENT.1
                    || current_num_pixels <= MIN_SAMPLES)
            };
            let to_erode = if erosion_count == 0 {
                if has_eroded_enough(&state.cleaned_grayscale_mask)? {
                    state
                        .cleaned_grayscale_mask
                        .copy_to(&mut state.eroded_grayscale_mask)?;
                    break;
                }
                &state.cleaned_grayscale_mask
            } else {
                if has_eroded_enough(&state.eroded_grayscale_mask)? || erosion_count == 5 {
                    if erosion_count == 1 {
                        state
                            .cleaned_grayscale_mask
                            .copy_to(&mut state.eroded_grayscale_mask)?;
                    } else {
                        std::mem::swap(&mut state.eroded_grayscale_mask, &mut state.tmp_mask);
                    }
                    break;
                }
                &state.eroded_grayscale_mask
            };

            imgproc::erode(
                to_erode,
                &mut state.tmp_mask,
                &state.erosion_kernel,
                DEF_ANCHOR,
                2,
                BORDER_CONSTANT,
                imgproc::morphology_default_border_value()?,
            )?;

            std::mem::swap(&mut state.eroded_grayscale_mask, &mut state.tmp_mask);
            erosion_count += 1;
        }

        let mut seed = [0; 32];
        seed[0..4].copy_from_slice(&drag_origin_x.to_be_bytes());
        seed[4..8].copy_from_slice(&drag_origin_y.to_be_bytes());
        let mut rng = SmallRng::from_seed(seed);
        nonzeroes = state
            .eroded_grayscale_mask
            .data_bytes()?
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(i, value)| {
                if value == u8::try_from(MAX_PIXEL_VALUE).unwrap() {
                    outer_index_to_inner_index(&state.eroded_grayscale_mask, &mask_roi, i)
                } else {
                    None
                }
            })
            .collect();
        state.samples = nonzeroes
            .partial_shuffle(&mut rng, NUM_QVIS_PIXELS)
            .0
            .to_vec();

        let xy_line_thickness = state.xy_line_thickness();
        imgproc::line(
            &mut state.displayed_img,
            Point::new(drag_origin_x, drag_origin_y),
            Point::new(drag_x, drag_y),
            Scalar::all(f64::from(MAX_PIXEL_VALUE)),
            xy_line_thickness,
            LINE_8,
            0,
        )?;
        let xy_circle_radius = state.xy_circle_radius();
        imgproc::circle(
            &mut state.displayed_img,
            Point::new(drag_x, drag_y),
            xy_circle_radius,
            Scalar::all(f64::from(MAX_PIXEL_VALUE)),
            FILLED,
            LINE_8,
            0,
        )?;
        imgproc::circle(
            &mut state.displayed_img,
            Point::new(drag_x, drag_y),
            xy_circle_radius - 3,
            Scalar::from((0, 0, MAX_PIXEL_VALUE)),
            FILLED,
            LINE_8,
            0,
        )?;
    } else {
        ran = false;
        state.samples.clear();
    }
    if let Some(white_balance_face) = state
        .white_balances_to_assign
        .get(state.assigning_white_balance_idx)
    {
        let text = if let Some((assigning_face, assigning_sticker)) =
            state.stickers_to_assign.get(state.assigning_sticker_idx)
        {
            format!(
                "Choose {} on {}",
                assigning_sticker
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<String>(),
                assigning_face.color
            )
        } else {
            format!("Choose white balance on {}", white_balance_face.color)
        };
        let mut display_instructions = |first: bool| -> Result<(), opencv::Error> {
            imgproc::put_text(
                &mut state.displayed_img,
                &text,
                Point::new(10, 40),
                imgproc::FONT_HERSHEY_SIMPLEX,
                state.gui_scale,
                if first {
                    Scalar::all(0.0)
                } else {
                    Scalar::all(f64::from(MAX_PIXEL_VALUE))
                },
                if first { 5 } else { 2 },
                imgproc::LINE_8,
                false,
            )
        };
        display_instructions(true)?;
        display_instructions(false)?;
    }
    if ran {
        let cleaned_grayscale_mask_cropped = Mat::roi(&state.cleaned_grayscale_mask, mask_roi)?;
        state.displayed_img.set_to(
            &Scalar::from((MAX_PIXEL_VALUE, 0, MAX_PIXEL_VALUE)),
            &cleaned_grayscale_mask_cropped,
        )?;

        let eroded_grayscale_mask_cropped = Mat::roi(&state.eroded_grayscale_mask, mask_roi)?;
        state.displayed_img.set_to(
            &Scalar::from((MAX_PIXEL_VALUE * 3 / 4, 0, MAX_PIXEL_VALUE * 3 / 4)),
            &eroded_grayscale_mask_cropped,
        )?;

        let displayed_image_data_bytes_mut: &mut [Vec3b] = state.displayed_img.data_typed_mut()?;
        for i in state.samples.iter().copied() {
            displayed_image_data_bytes_mut[i] = Vec3b::from_array([
                u8::try_from(MAX_PIXEL_VALUE).unwrap() / 2,
                0,
                u8::try_from(MAX_PIXEL_VALUE).unwrap() / 2,
            ]);
        }
    } else {
        let pixel_assignment_mask_cropped = match state.crop {
            CropState::NoCrop | CropState::SelectedCrop(_) | CropState::SelectingCrop(_) => {
                Mat::copy(&state.pixel_assignment_mask)?
            }
            CropState::Crop((rect, _)) => Mat::roi(&state.pixel_assignment_mask, rect)?,
        };
        state.displayed_img.set_to(
            &Scalar::from((MAX_PIXEL_VALUE, 0, MAX_PIXEL_VALUE)),
            &pixel_assignment_mask_cropped,
        )?;
    }
    highgui::imshow(WINDOW_NAME, &state.displayed_img)?;
    Ok(())
}

fn mouse_callback(state: &mut State, event: i32, x: i32, y: i32) -> opencv::Result<()> {
    if event == highgui::EVENT_MOUSEMOVE {
        state.maybe_xy = Some((x, y));
        if let CropState::SelectingCrop(rect) = &mut state.crop {
            state.img.copy_to(&mut state.displayed_img)?;
            match x.cmp(&rect.x) {
                Ordering::Less => {
                    rect.width -= x - rect.x;
                    rect.x = x;
                }
                Ordering::Greater => {
                    rect.width = x - rect.x;
                }
                Ordering::Equal => {
                    rect.width = 1;
                }
            }
            match y.cmp(&rect.y) {
                Ordering::Less => {
                    rect.height -= y - rect.y;
                    rect.y = y;
                }
                Ordering::Greater => {
                    rect.height = y - rect.y;
                }
                Ordering::Equal => {
                    rect.height = 1;
                }
            }
            imgproc::rectangle(
                &mut state.displayed_img,
                *rect,
                Scalar::from((MAX_PIXEL_VALUE, MAX_PIXEL_VALUE, 0)),
                2,
                LINE_8,
                RECTANGLE_DEF_SHIFT,
            )?;
            highgui::imshow(WINDOW_NAME, &state.displayed_img)?;
        } else if let CropState::SelectedCrop(rect) = &state.crop {
            state.img.copy_to(&mut state.displayed_img)?;
            imgproc::rectangle(
                &mut state.displayed_img,
                *rect,
                Scalar::from((MAX_PIXEL_VALUE, MAX_PIXEL_VALUE, 0)),
                2,
                LINE_8,
                RECTANGLE_DEF_SHIFT,
            )?;
            highgui::imshow(WINDOW_NAME, &state.displayed_img)?;
        } else if state.dragging {
            state.maybe_drag_xy = Some((x, y));
            update_floodfill_display(state)?;
        }
    } else if event == EVENT_LBUTTONUP {
    }

    Ok(())
}

fn erosion_kernel_trackbar_callback(state: &mut State, pos: i32) -> opencv::Result<()> {
    state.erosion_kernel =
        imgproc::get_structuring_element_def(EROSION_KERNEL_MORPH_SHAPE, Size::new(pos, pos))?;
    state.erosion_kernel_times_two = imgproc::get_structuring_element_def(
        EROSION_KERNEL_MORPH_SHAPE,
        Size::new(pos * 2, pos * 2),
    )?;
    update_floodfill_display(state)?;
    Ok(())
}

fn light_tolerance_trackbar_callback(state: &mut State, pos: i32) -> opencv::Result<()> {
    state.upper_flood_fill_diff = pos;
    update_floodfill_display(state)?;
    Ok(())
}

fn gui_scale_trackbar_callback(state: &mut State, pos: i32) -> opencv::Result<()> {
    state.gui_scale = f64::from(pos) / 10.0;
    update_floodfill_display(state)?;
    Ok(())
}

fn submit_button_callback(state: &mut State) -> opencv::Result<()> {
    let mut count = 0;
    for &(mut i) in &state.samples {
        if let CropState::Crop((rect, _)) = &state.crop {
            i = inner_index_to_outer_index(&state.img, rect, i).unwrap();
        }
        count += 1;
        state.pixel_assignment[i] = if state.assigning_sticker_idx == state.stickers_to_assign.len()
        {
            let face = &state.white_balances_to_assign[state.assigning_white_balance_idx];
            Pixel::WhiteBalance(face.color.clone())
        } else {
            Pixel::Sticker(state.assigning_sticker_idx)
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let row = i as i32 / state.img.cols();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let col = i as i32 % state.img.cols();
        *state.pixel_assignment_mask.at_2d_mut::<u8>(row, col)? = 255;
    }

    leptos::logging::log!("Assigned {count} pixels");

    if state.assigning_sticker_idx == state.stickers_to_assign.len() {
        state.assigning_white_balance_idx += 1;
        if state.assigning_white_balance_idx == state.white_balances_to_assign.len() {
            state.ui = UIState::Finished;
            return Ok(());
        }
    } else {
        state.assigning_sticker_idx += 1;
    }
    state.maybe_drag_origin = None;
    update_floodfill_display(state)?;

    Ok(())
}

fn back_button_callback(state: &mut State) -> opencv::Result<()> {
    if state.assigning_white_balance_idx != 0 {
        state.assigning_white_balance_idx -= 1;
    } else if state.assigning_sticker_idx != 0 {
        state.assigning_sticker_idx -= 1;
    } else {
        return Ok(());
    }

    let mut count = 0;
    for i in 0..state.pixel_assignment.len() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let row = i as i32 / state.img.cols();
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let col = i as i32 % state.img.cols();
        if state.assigning_sticker_idx == state.stickers_to_assign.len() {
            let face = &state.white_balances_to_assign[state.assigning_white_balance_idx];
            if matches!(&state.pixel_assignment[i], Pixel::WhiteBalance(c) if c == &face.color) {
                count += 1;
                state.pixel_assignment[i] = Pixel::Unassigned;
                *state.pixel_assignment_mask.at_2d_mut::<u8>(row, col)? = 0;
            }
        } else if matches!(state.pixel_assignment[i], Pixel::Sticker(j) if j == state.assigning_sticker_idx)
        {
            count += 1;
            state.pixel_assignment[i] = Pixel::Unassigned;
            *state.pixel_assignment_mask.at_2d_mut::<u8>(row, col)? = 0;
        }
    }

    leptos::logging::log!("Went back {count} pixels");

    state.maybe_drag_origin = None;
    update_floodfill_display(state)?;

    Ok(())
}

fn toggle_dragging(state: &mut State) {
    if state.dragging {
        state.dragging = false;
    } else if let Some((x, y)) = state.maybe_xy {
        if let Some((drag_x, drag_y)) = state.maybe_drag_xy {
            let distance = f64::from(drag_x - x).hypot(f64::from(drag_y - y));
            if distance > f64::from(state.xy_circle_radius()) {
                state.maybe_drag_origin = Some((x, y));
            }
        } else {
            state.maybe_drag_origin = Some((x, y));
        }
        state.maybe_drag_xy = Some((x, y));
        state.dragging = true;
    }
}

fn crop_action(state: &mut State) -> opencv::Result<()> {
    match state.crop {
        CropState::NoCrop => {
            if let Some((x, y)) = state.maybe_xy {
                state.crop = CropState::SelectingCrop(Rect::new(x, y, 0, 0));
            } else {
                return Ok(());
            }
        }
        CropState::SelectingCrop(rect) => {
            state.crop = CropState::SelectedCrop(rect);
        }
        CropState::SelectedCrop(rect) => {
            let cropped_image = Mat::roi(&state.img, rect)?;
            if cropped_image.rows() < 3 || cropped_image.cols() < 3 {
                return Ok(());
            }
            let cropped_image = cropped_image.clone_pointee();
            state.displayed_img =
                Mat::zeros(cropped_image.rows(), cropped_image.cols(), CV_8UC3)?.to_mat()?;
            state.grayscale_mask =
                Mat::zeros(cropped_image.rows() + 2, cropped_image.cols() + 2, CV_8UC1)?
                    .to_mat()?;
            state.cleaned_grayscale_mask = state.grayscale_mask.clone();
            state.eroded_grayscale_mask = state.grayscale_mask.clone();
            state.tmp_mask = state.grayscale_mask.clone();
            if let Some(drag_origin_mut) = state.maybe_drag_origin.as_mut() {
                if drag_origin_mut.0 >= rect.x
                    && drag_origin_mut.0 < rect.x + rect.width
                    && drag_origin_mut.1 >= rect.y
                    && drag_origin_mut.1 < rect.y + rect.height
                {
                    drag_origin_mut.0 -= rect.x;
                    drag_origin_mut.1 -= rect.y;
                } else {
                    state.maybe_drag_origin = None;
                }
            }
            if let Some(drag_xy_mut) = state.maybe_drag_xy.as_mut() {
                if drag_xy_mut.0 >= rect.x
                    && drag_xy_mut.0 < rect.x + rect.width
                    && drag_xy_mut.1 >= rect.y
                    && drag_xy_mut.1 < rect.y + rect.height
                {
                    drag_xy_mut.0 -= rect.x;
                    drag_xy_mut.1 -= rect.y;
                } else {
                    state.maybe_drag_xy = None;
                }
            }
            if let Some(xy_mut) = state.maybe_xy.as_mut() {
                if xy_mut.0 >= rect.x
                    && xy_mut.0 < rect.x + rect.width
                    && xy_mut.1 >= rect.y
                    && xy_mut.1 < rect.y + rect.height
                {
                    xy_mut.0 -= rect.x;
                    xy_mut.1 -= rect.y;
                } else {
                    state.maybe_xy = None;
                }
            }

            state.crop = CropState::Crop((rect, cropped_image));
        }
        CropState::Crop((rect, _)) => {
            state.displayed_img =
                Mat::zeros(state.img.rows(), state.img.cols(), CV_8UC3)?.to_mat()?;
            state.grayscale_mask =
                Mat::zeros(state.img.rows() + 2, state.img.cols() + 2, CV_8UC1)?.to_mat()?;
            state.cleaned_grayscale_mask = state.grayscale_mask.clone();
            state.eroded_grayscale_mask = state.grayscale_mask.clone();
            state.tmp_mask = state.grayscale_mask.clone();
            if let Some(drag_origin_mut) = state.maybe_drag_origin.as_mut() {
                if drag_origin_mut.0 >= 0
                    && drag_origin_mut.0 < rect.width
                    && drag_origin_mut.1 >= 0
                    && drag_origin_mut.1 < rect.height
                {
                    drag_origin_mut.0 += rect.x;
                    drag_origin_mut.1 += rect.y;
                } else {
                    state.maybe_drag_origin = None;
                }
            }
            if let Some(drag_xy_mut) = state.maybe_drag_xy.as_mut() {
                if drag_xy_mut.0 >= 0
                    && drag_xy_mut.0 < rect.width
                    && drag_xy_mut.1 >= 0
                    && drag_xy_mut.1 < rect.height
                {
                    drag_xy_mut.0 += rect.x;
                    drag_xy_mut.1 += rect.y;
                } else {
                    state.maybe_drag_xy = None;
                }
            }
            if let Some(xy_mut) = state.maybe_xy.as_mut() {
                if xy_mut.0 >= 0 && xy_mut.0 < rect.width && xy_mut.1 >= 0 && xy_mut.1 < rect.height
                {
                    xy_mut.0 += rect.x;
                    xy_mut.1 += rect.y;
                } else {
                    state.maybe_xy = None;
                }
            }
            state.crop = CropState::NoCrop;
        }
    }
    update_floodfill_display(state)?;
    Ok(())
}

/// Displays a UI for assignment the stickers of a `PuzzleGeometry`
///
/// # Errors
///
/// This function will return an `OpenCV` error.
pub fn pixel_assignment_ui(
    puzzle_geometry: &PuzzleGeometry,
    // image: &DynamicImage,
    bytes: &Bytes,
) -> Result<Box<[Pixel]>, opencv::Error> {
    let img = imgcodecs::imdecode(&&**bytes, IMREAD_COLOR)?;

    highgui::named_window(
        WINDOW_NAME,
        highgui::WINDOW_NORMAL | highgui::WINDOW_KEEPRATIO | highgui::WINDOW_GUI_EXPANDED,
    )?;

    let w = img.cols();
    let h = img.rows();
    leptos::logging::log!("Image dimensions: w={w} h={h}");
    let pixel_count = w * h;

    let displayed_img = Mat::zeros(img.rows(), img.cols(), CV_8UC3)?.to_mat()?;
    let grayscale_mask = Mat::zeros(img.rows() + 2, img.cols() + 2, CV_8UC1)?.to_mat()?;
    let cleaned_grayscale_mask = grayscale_mask.clone();
    let eroded_grayscale_mask = grayscale_mask.clone();
    let tmp_mask = grayscale_mask.clone();
    let erosion_kernel = Mat::default();
    let erosion_kernel_times_two = Mat::default();

    let pixel_assignment = vec![
        Pixel::Unassigned;
        pixel_count.try_into().map_err(|e| opencv::Error::new(
            opencv::core::StsError,
            format!("Too many pixels: {e}"),
        ))?
    ]
    .into_boxed_slice();
    let pixel_assignment_mask_cropped = Mat::zeros(img.rows(), img.cols(), CV_8UC1)?.to_mat()?;

    let stickers_to_assign = puzzle_geometry.non_fixed_stickers().to_vec();
    let mut white_balances_to_assign: Vec<_> = stickers_to_assign
        .iter()
        .map(|(face, _)| face.clone())
        .collect();
    white_balances_to_assign.dedup_by_key(|face| face.color.clone());

    let state = Arc::new(Mutex::new(State {
        img,
        tmp_mask,
        grayscale_mask,
        cleaned_grayscale_mask,
        samples: Vec::with_capacity(NUM_QVIS_PIXELS),
        eroded_grayscale_mask,
        erosion_kernel,
        erosion_kernel_times_two,
        gui_scale: 0.0,
        displayed_img,
        pixel_assignment,
        pixel_assignment_mask: pixel_assignment_mask_cropped,
        stickers_to_assign,
        white_balances_to_assign,
        assigning_white_balance_idx: 0,
        assigning_sticker_idx: 0,
        upper_flood_fill_diff: 0,
        maybe_drag_origin: None,
        maybe_drag_xy: None,
        maybe_xy: None,
        dragging: false,
        crop: CropState::NoCrop,
        ui: UIState::Assigning,
    }));

    {
        let state = Arc::clone(&state);
        highgui::set_mouse_callback(
            WINDOW_NAME,
            Some(Box::new(move |event, x, y, _flags| {
                #[allow(clippy::missing_panics_doc)]
                let mut state = state.lock().unwrap();
                if let Err(e) = mouse_callback(&mut state, event, x, y) {
                    state.ui = UIState::OpenCVError(e);
                }
            })),
        )?;
    }
    {
        let state = Arc::clone(&state);
        highgui::create_trackbar(
            EROSION_SIZE_TRACKBAR_NAME,
            WINDOW_NAME,
            None,
            EROSION_SIZE_TRACKBAR_MINDEFMAX[2],
            Some(Box::new(move |pos| {
                #[allow(clippy::missing_panics_doc)]
                let mut state = state.lock().unwrap();
                if let Err(e) = erosion_kernel_trackbar_callback(&mut state, pos) {
                    state.ui = UIState::OpenCVError(e);
                }
            })),
        )?;
        highgui::set_trackbar_pos(
            EROSION_SIZE_TRACKBAR_NAME,
            WINDOW_NAME,
            EROSION_SIZE_TRACKBAR_MINDEFMAX[1],
        )?;
        highgui::set_trackbar_min(
            EROSION_SIZE_TRACKBAR_NAME,
            WINDOW_NAME,
            EROSION_SIZE_TRACKBAR_MINDEFMAX[0],
        )?;
    }
    {
        let state = Arc::clone(&state);
        highgui::create_trackbar(
            UPPER_DIFF_TRACKBAR_NAME,
            WINDOW_NAME,
            None,
            UPPER_DIFF_TRACKBAR_MINDEFMAX[2],
            Some(Box::new(move |pos| {
                #[allow(clippy::missing_panics_doc)]
                let mut state = state.lock().unwrap();
                if let Err(e) = light_tolerance_trackbar_callback(&mut state, pos) {
                    state.ui = UIState::OpenCVError(e);
                }
            })),
        )?;
        highgui::set_trackbar_pos(
            UPPER_DIFF_TRACKBAR_NAME,
            WINDOW_NAME,
            UPPER_DIFF_TRACKBAR_MINDEFMAX[1],
        )?;
        highgui::set_trackbar_min(
            UPPER_DIFF_TRACKBAR_NAME,
            WINDOW_NAME,
            UPPER_DIFF_TRACKBAR_MINDEFMAX[0],
        )?;
    }
    {
        let state = Arc::clone(&state);
        highgui::create_trackbar(
            GUI_SCALE_TRACKBAR_NAME,
            WINDOW_NAME,
            None,
            GUI_SCALE_TRACKBAR_MINDEFMAX[2],
            Some(Box::new(move |pos| {
                #[allow(clippy::missing_panics_doc)]
                let mut state = state.lock().unwrap();
                if let Err(e) = gui_scale_trackbar_callback(&mut state, pos) {
                    state.ui = UIState::OpenCVError(e);
                }
            })),
        )?;
        highgui::set_trackbar_pos(
            GUI_SCALE_TRACKBAR_NAME,
            WINDOW_NAME,
            GUI_SCALE_TRACKBAR_MINDEFMAX[1],
        )?;
        highgui::set_trackbar_min(
            GUI_SCALE_TRACKBAR_NAME,
            WINDOW_NAME,
            GUI_SCALE_TRACKBAR_MINDEFMAX[0],
        )?;
    }
    {
        let state = Arc::clone(&state);
        highgui::create_button_def(
            SUBMIT_BUTTON_NAME,
            Some(Box::new(move |_state| {
                #[allow(clippy::missing_panics_doc)]
                let mut state = state.lock().unwrap();
                if let Err(e) = submit_button_callback(&mut state) {
                    state.ui = UIState::OpenCVError(e);
                }
            })),
        )?;
    }
    {
        let state = Arc::clone(&state);
        highgui::create_button_def(
            BACK_BUTTON_NAME,
            Some(Box::new(move |_state| {
                #[allow(clippy::missing_panics_doc)]
                let mut state = state.lock().unwrap();
                if let Err(e) = back_button_callback(&mut state) {
                    state.ui = UIState::OpenCVError(e);
                }
            })),
        )?;
    }

    {
        #[allow(clippy::missing_panics_doc)]
        let mut state = state.lock().unwrap();
        update_floodfill_display(&mut state)?;
    }

    let mut holding_f = false;
    let mut holding_c = false;
    loop {
        const B: i32 = 98;
        const C: i32 = 99;
        const N: i32 = 110;
        const F: i32 = 102;

        {
            #[allow(clippy::missing_panics_doc)]
            let state = state.lock().unwrap();
            if matches!(&state.ui, UIState::Finished | UIState::OpenCVError(_)) {
                // https://stackoverflow.com/questions/6116564/destroywindow-does-not-close-window-on-mac-using-python-and-opencv
                highgui::destroy_all_windows()?;
                highgui::wait_key(1)?;
            }
            match &state.ui {
                UIState::Finished => {
                    leptos::logging::log!("Finished pixel assignment UI");
                    break Ok(state.pixel_assignment.clone());
                }
                UIState::OpenCVError(e) => {
                    break Err(opencv::Error::new(
                        e.code,
                        format!("OpenCV error during pixel assignment: {}", e.message),
                    ));
                }
                // UIState::Assigning
                //     if (highgui::get_window_property(WINDOW_NAME, highgui::WND_PROP_VISIBLE)?
                //         + 1.0)
                //         .abs()
                //         < 0.1 =>
                // {
                //     break Err(opencv::Error::new(
                //         opencv::core::StsError,
                //         "Pixel assignment window was closed by user".to_string(),
                //     ));
                // }
                UIState::Assigning => {}
            }
        }

        let key = highgui::wait_key(1000 / 30)?;
        {
            #[allow(clippy::missing_panics_doc)]
            let mut state = state.lock().unwrap();
            match key {
                C => {
                    holding_f = false;
                    if !holding_c {
                        crop_action(&mut state)?;
                        holding_c = true;
                    }
                }
                N => {
                    holding_f = false;
                    holding_c = false;
                    submit_button_callback(&mut state)?;
                }
                B => {
                    holding_f = false;
                    holding_c = false;
                    back_button_callback(&mut state)?;
                }
                F => {
                    if !holding_f {
                        toggle_dragging(&mut state);
                        holding_f = true;
                    }
                    holding_c = false;
                }
                _ => {
                    holding_f = false;
                    holding_c = false;
                }
            }
        }
    }
}
