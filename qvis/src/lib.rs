use std::sync::Arc;

use internment::ArcIntern;
use puzzle_theory::{permutations::Permutation, puzzle_geometry::PuzzleGeometry};
use serde::{Deserialize, Serialize};

use crate::{inference::Inference, puzzle_matching::Matcher};

mod inference;
pub mod puzzle_matching;

/// Processes images for computer vision
#[derive(Deserialize)]
#[serde(from = "(usize, Arc<PuzzleGeometry>, Inference)")]
pub struct CVProcessor {
    image_size: usize,
    puzzle: Arc<PuzzleGeometry>,
    matcher: Matcher,
    inference: Inference,
}

#[derive(Debug, Clone)]
#[derive(Serialize, Deserialize)]
pub enum Pixel {
    /// The pixel is not assigned to anything
    Unassigned,
    /// The pixel is white balance for the face of the given color
    WhiteBalance(ArcIntern<str>),
    /// The pixel is assigned to a particular sticker
    Sticker(usize),
}

impl CVProcessor {
    /// Create a new `CVProcessor` that recognizes the given puzzle in images. `image_size` specifies the number of pixels in the image. The CV algorithm does not care about rows and columns.
    ///
    /// # Assignment
    ///
    /// The assignment is the same size as the image.
    ///
    /// Each pixel is configured with a number determining which index sticker of the puzzle it belongs to. This method panics if any indices are out of range. The boolean parameter determines whether the pixel should be treated as white balance for the given face: `false` means that it is not white balance and `true` means that it is white balance.
    ///
    /// White balance points should be selected such that the face is parallel with the face that it is acting as white balance for.
    ///
    /// Pixels marked `None` will not be considered in the CV algorithm.
    pub fn new(
        puzzle: Arc<PuzzleGeometry>,
        image_size: usize,
        assignment: Box<[Pixel]>,
    ) -> CVProcessor {
        CVProcessor {
            image_size,
            inference: Inference::new(assignment, &puzzle),
            matcher: Matcher::new(&puzzle),
            puzzle,
        }
    }

    /// Calibrate the CV processor with an image of the puzzle in the given state.
    pub fn calibrate(&mut self, image: &[(f64, f64, f64)], state: &Permutation) {
        assert_eq!(self.image_size, image.len());

        self.inference
            .calibrate(image, state, &self.puzzle.permutation_group());
    }

    /// Process an image and return the most likely state that the puzzle appears to be in, along with the confidence in the prediction. This is guaranteed to be a valid member of the group.
    pub fn process_image(&self, image: Box<[(f64, f64, f64)]>) -> (Permutation, f64) {
        self.matcher.most_likely(
            &self
                .inference
                .infer(&image, &self.puzzle.permutation_group()),
            &self.puzzle,
        )
    }
}

impl Serialize for CVProcessor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        (&self.image_size, &self.puzzle, &self.inference).serialize(serializer)
    }
}

impl From<(usize, Arc<PuzzleGeometry>, Inference)> for CVProcessor {
    fn from((image_size, puzzle, inference): (usize, Arc<PuzzleGeometry>, Inference)) -> Self {
        CVProcessor {
            image_size,
            matcher: Matcher::new(&puzzle),
            puzzle,
            inference,
        }
    }
}
