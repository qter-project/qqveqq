use std::{
    collections::HashMap,
    sync::Arc,
};

use internment::ArcIntern;
use puzzle_theory::{
    permutations::{Permutation, PermutationGroup, schreier_sims::StabilizerChain},
    puzzle_geometry::{OrbitData, PuzzleGeometry},
};

mod hungarian_algorithm;

pub struct Matcher {
    orbits: Vec<OrbitMatcher>,
    stab_chain: StabilizerChain,
}

impl Matcher {
    pub fn new(puzzle: Arc<PuzzleGeometry>) -> Matcher {
        let data = puzzle.pieces_data();

        let orbits = data
            .orbits()
            .iter()
            .map(|orbit| OrbitMatcher::new(Arc::clone(&puzzle), orbit))
            .collect();

        Matcher {
            orbits,
            stab_chain: StabilizerChain::new(&puzzle.permutation_group()),
        }
    }
}

struct OrbitMatcher {
    stab_chain: StabilizerChain,
    // Maps the observation (sticker orientation idx, color) to all (piece, orientation) that would be consistent with it
    sticker_color_piece: HashMap<(usize, ArcIntern<str>), Vec<(usize, usize)>>,
}

impl OrbitMatcher {
    fn new(puzzle: Arc<PuzzleGeometry>, orbit: &OrbitData) -> OrbitMatcher {
        let pieces_data = puzzle.pieces_data();
        let ori_nums = pieces_data.orientation_numbers();
        let group = puzzle.permutation_group();

        let ori_count = orbit.orientation_count();

        let mut sticker_color_piece =
            HashMap::<(usize, ArcIntern<str>), Vec<(usize, usize)>>::new();

        let mut sticker_in_orbit = vec![false; group.facelet_count()];

        for (i, piece) in orbit.pieces().iter().enumerate() {
            for sticker in piece.stickers() {
                sticker_in_orbit[*sticker] = true;

                let mut current_sticker = *sticker;
                for ori in 0..ori_count {
                    let ori_num = ori_nums[current_sticker];
                    let color = ArcIntern::clone(&group.facelet_colors()[ori_num]);

                    let pieces = sticker_color_piece.entry((ori_num, color)).or_default();
                    pieces.push((i, ori));

                    current_sticker = piece.twist().mapping().get(current_sticker);
                }
            }
        }

        let subgroup = PermutationGroup::new(
            group.facelet_colors().to_owned(),
            group.piece_assignments().to_owned(),
            group
                .generators()
                .map(|(name, perm)| {
                    let new_perm = Permutation::from_mapping(
                        perm.mapping()
                            .minimal()
                            .iter()
                            .enumerate()
                            .map(|(i, v)| if sticker_in_orbit[i] { *v } else { i })
                            .collect(),
                    );

                    (name, new_perm)
                })
                .collect(),
        );

        OrbitMatcher {
            stab_chain: StabilizerChain::new(&Arc::new(subgroup)),
            sticker_color_piece,
        }
    }
}
