use std::{collections::HashMap, sync::Arc};

use internment::ArcIntern;
use kiddo::KdTree;
use puzzle_theory::{
    permutations::{Permutation, PermutationGroup},
    puzzle_geometry::PuzzleGeometry,
};

use crate::{AssigningPixels, CVState, cv::assignment};

const CONFIDENCE_PERCENTILE: f64 = 0.8;

struct Pixel {
    idx: usize,
    observations: HashMap<ArcIntern<str>, KdTree<f64, 3>>,
}

pub struct Inference {
    pixels_by_sticker: Box<[Box<[Pixel]>]>,
    group: Arc<PermutationGroup>,
}

impl Inference {
    pub(crate) fn from_assignment(
        assignment: AssigningPixels,
        puzzle: &PuzzleGeometry,
    ) -> Inference {
        let mut pixels_by_sticker: Vec<Vec<Pixel>> = Vec::new();

        let group = puzzle.permutation_group();
        let facelet_colors = group.facelet_colors();

        for _ in 0..group.facelet_count() {
            pixels_by_sticker.push(Vec::new());
        }

        let empty_kdtrees: HashMap<ArcIntern<str>, KdTree<f64, 3>> = assignment
            .stickers_by_face
            .keys()
            .cloned()
            .map(|a| (a, KdTree::<f64, 3>::new()))
            .collect();

        for (idx, pixel) in assignment.pixels.into_iter().enumerate() {
            let assignment::Pixel::Sticker {
                face,
                history,
                r_statistic_by_sticker_option,
            } = pixel
            else {
                continue;
            };

            let face_options = assignment.stickers_by_face.get(&face).unwrap();

            let sticker = *r_statistic_by_sticker_option
                .iter()
                .zip(face_options.iter())
                .max_by(|(a, _), (b, _)| a.total_cmp(b))
                .unwrap()
                .1;

            let mut kdtrees = empty_kdtrees.to_owned();

            for (perm, (r, g, b)) in assignment.perm_history.iter().zip(history.into_iter()) {
                let color = &facelet_colors[perm.comes_from().get(sticker)];
                kdtrees.get_mut(color).unwrap().add(&[r, g, b], 0);
            }

            pixels_by_sticker[sticker].push(Pixel {
                idx,
                observations: kdtrees,
            });
        }

        Inference {
            pixels_by_sticker: pixels_by_sticker.into_iter().map(|v| v.into()).collect(),
            group,
        }
    }

    pub(crate) fn infer(&self, picture: &[(f64, f64, f64)]) -> Box<[HashMap<ArcIntern<str>, f64>]> {
        todo!()
    }
}

impl CVState for Inference {
    fn calibrate(&mut self, image: &[(f64, f64, f64)], state: Permutation) {
        for (sticker, pixels) in self.pixels_by_sticker.iter_mut().enumerate() {
            let color = &self.group.facelet_colors()[state.comes_from().get(sticker)];

            for pixel in pixels {
                let (r, g, b) = image[pixel.idx];
                pixel
                    .observations
                    .get_mut(color)
                    .unwrap()
                    .add(&[r, g, b], 0);
            }
        }
    }
}
