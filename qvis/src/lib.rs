// `CVState` should be sealed
#![allow(private_bounds)]

use std::sync::Arc;

use internment::ArcIntern;
use puzzle_theory::{permutations::Permutation, puzzle_geometry::PuzzleGeometry};

use crate::puzzle_matching::Matcher;

mod cv;
pub use cv::{assignment::AssigningPixels, inference::Inference};
pub mod puzzle_matching;

trait CVState {
    fn calibrate(&mut self, image: &[(f64, f64, f64)], state: Permutation);
}

/// Processes images for computer vision
pub struct CVProcessor<S: CVState> {
    puzzle: Arc<PuzzleGeometry>,
    image_size: usize,
    matcher: Matcher,
    state: S,
}

impl CVProcessor<AssigningPixels> {
    /// Create a new `CVProcessor` that recognizes the given puzzle in images. `image_size` specifies the number of pixels in the image. The CV algorithm does not care about rows and columns.
    ///
    /// # Mask
    ///
    /// The mask is the same size as the image.
    ///
    /// Each pixel is configured with a color determining which face of the puzzle it belongs to, where the colors are colors of the faces. This method panics if any colors are not faces of the puzzle. The boolean parameter determines whether the pixel should be treated as white balance for the given face: `false` means that it is not white balance and `true` means that it is white balance.
    ///
    /// White balance points should be selected such that the face is parallel with the face that it is acting as white balance for.
    ///
    /// Pixels marked `None` will not be considered in the CV algorithm.
    pub fn new(puzzle: Arc<PuzzleGeometry>, image_size: usize, mask: Box<[Option<(ArcIntern<str>, bool)>]>) -> CVProcessor<AssigningPixels> {
        CVProcessor {
            puzzle: Arc::clone(&puzzle),
            image_size,
            matcher: Matcher::new(puzzle),
            state: AssigningPixels::new(mask),
        }
    }

    /// Finish assignment of pixels to stickers
    pub fn finish_selection(self) -> CVProcessor<Inference> {
        todo!()
    }
}

impl<S: CVState> CVProcessor<S> {
    /// Calibrate the CV processor with an image of the puzzle in the given state.
    pub fn calibrate(&mut self, image: &[(f64, f64, f64)], state: Permutation) {
        assert_eq!(self.image_size, image.len());

        self.state.calibrate(image, state);
    }
}

impl CVProcessor<Inference> {
    /// Process an image and return the most likely state that the puzzle appears to be in, along with the confidence in the prediction. This is guaranteed to be a valid member of the group.
    pub fn process_image(&self, image: Box<[(f64, f64, f64)]>) -> (Permutation, f64) {
        self.matcher.most_likely(&self.state.infer(&image))
    }
}
