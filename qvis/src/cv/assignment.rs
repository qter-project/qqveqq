use std::collections::HashMap;

use internment::ArcIntern;
use itertools::Itertools;
use puzzle_theory::{permutations::Permutation, puzzle_geometry::PuzzleGeometry};

use crate::CVState;

pub(super) enum Pixel {
    Unmasked,
    Sticker {
        face: ArcIntern<str>,
        history: Vec<(f64, f64, f64)>,
        r_statistic_by_sticker_option: Box<[f64]>,
    },
    WhiteBalance {
        face: ArcIntern<str>,
    },
}

pub struct AssigningPixels {
    pub(super) pixels: Box<[Pixel]>,
    pub(super) stickers_by_face: HashMap<ArcIntern<str>, Vec<usize>>,
    pub(super) perm_history: Vec<Permutation>,
}

impl AssigningPixels {
    pub(crate) fn new(
        mask: Box<[Option<(ArcIntern<str>, bool)>]>,
        puzzle: &PuzzleGeometry,
    ) -> Self {
        let stickers_by_face = puzzle
            .permutation_group()
            .facelet_colors()
            .iter()
            .cloned()
            .enumerate()
            .map(|(a, b)| (b, a))
            .into_group_map();

        Self {
            pixels: mask
                .into_iter()
                .map(|v| match v {
                    None => Pixel::Unmasked,
                    Some((face, true)) => Pixel::WhiteBalance { face },
                    Some((face, false)) => Pixel::Sticker {
                        r_statistic_by_sticker_option: (0..stickers_by_face
                            .get(&face)
                            .unwrap()
                            .len())
                            .map(|_| 0.)
                            .collect(),
                        face,
                        history: Vec::new(),
                    },
                })
                .collect(),
            stickers_by_face,
            perm_history: Vec::new(),
        }
    }
}

impl CVState for AssigningPixels {
    fn calibrate(&mut self, image: &[(f64, f64, f64)], state: Permutation) {
        self.perm_history.push(state);

        for (color, pixel) in image.iter().copied().zip(self.pixels.iter_mut()) {
            let Pixel::Sticker {
                face,
                history,
                r_statistic_by_sticker_option,
            } = pixel
            else {
                continue;
            };

            history.push(color);

            for (sticker_option, f_statistic) in self
                .stickers_by_face
                .get(face)
                .unwrap()
                .iter()
                .copied()
                .zip(r_statistic_by_sticker_option.iter_mut())
            {
                todo!()
            }
        }
    }
}
