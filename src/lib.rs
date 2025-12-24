use std::sync::{Arc, mpsc};

use puzzle_theory::{permutations::Permutation, puzzle_geometry::PuzzleGeometry};

mod hungarian_algorithm;

/// Processes images for computer vision
pub struct CVProcessor {
    puzzle: Arc<PuzzleGeometry>,
    image_size: usize,
    calibration_tx: mpsc::Sender<(Box<[(f64, f64, f64)]>, Permutation)>,
    calibration_rx: Option<mpsc::Receiver<(Box<[(f64, f64, f64)]>, Permutation)>>,
}

impl CVProcessor {
    /// Create a new `CVProcessor` that recognizes the given puzzle in images. `image_size` specifies the number of pixels in the image. The CV algorithm does not care about rows and columns.
    pub fn new(puzzle: Arc<PuzzleGeometry>, image_size: usize) -> CVProcessor {
        let (tx, rx) = mpsc::channel();

        CVProcessor { puzzle, image_size, calibration_tx: tx, calibration_rx: Some(rx) }
    }

    /// Calibrate the CV processor with an image of the puzzle in the given state.
    ///
    /// # Panics
    ///
    /// This method panics if the calibration thread panicked or was otherwise killed before calling this method.
    pub fn calibrate(&self, image: Box<[(f64, f64, f64)]>, state: Permutation) {
        assert_eq!(self.image_size, image.len());
        self.calibration_tx.send((image, state)).expect("The calibration thread should not be dropped");
    }

    /// Return a callback that should be called in a new thread and detached. This thread will continuously process CV data in the background to improve the model. Making this its own thread allows image processing to occur immediately and asynchronously with respect to model improvement.
    ///
    /// It is a logic error to not spawn this thread and image processing will not work properly without it.
    ///
    /// # Panics
    ///
    /// This method will panic if called more than once.
    pub fn calibration_thread(&mut self) -> impl FnOnce() + 'static {
        let rx = self.calibration_rx.take().expect("The calibration thread to only be started once");

        move || {
            while let Ok((img, perm)) = rx.recv() {
                println!("Got a calibration!");
            }
        }
    }

    /// Process an image and return the most likely state that the puzzle appears to be in, along with the confidence in the prediction. This is guaranteed to be a valid member of the group.
    ///
    /// The `calibrate` argument tells whether to allow using this prediction for calibration. Setting this to true allows the model to handle non-stationarity in lighting conditions at the cost of errors potentially weakening the model. We recommend setting this to `false` when validating the CV after calibration, and setting it to `true` during normal use.
    ///
    /// # Panics
    ///
    /// This method panics if
    /// - `mask` has never been called, or
    /// - `calibrate` is `true` and the calibration thread panicked or was otherwise killed before calling this method.
    pub fn process_image(&self, image: Box<[(f64, f64, f64)]>, calibrate: bool) -> (Permutation, f64) {
        (Permutation::from_mapping(Vec::new()), 0.)
    }

    /// Set the mask of the image; the mask is the same size as the image.
    ///
    /// Each pixel is configured with a number determining which face of the puzzle it belongs to, such that pixels with the same number correspond to the same face. The boolean parameter determines whether the pixel should be treated as white balance for the given face: `false` means that it is not white balance and `true` means that it is white balance.
    ///
    /// White balance points should be selected such that the face is parallel with the face that it is acting as white balance for.
    pub fn mask(&self, mask: Box<[Option<(u32, bool)>]>) {}
}
