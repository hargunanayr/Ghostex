//! The pixel creatures that stand for a project without an icon of its own in the two-column layout.
//!
//! CDXC:Icons 2026-10-27 DECISION:
//! User, showing pixel space-invader icons beside each project: "I want my project icons to be cute like this, by default following the group's colour but with a random icon". In the two-column layout a project without an icon the user set gets one of these creatures instead of its first letter (and instead of a favicon found in its folder): which creature is picked from the project's id (moved on to the next free one when a project earlier in the same card has it), so it never changes between draws, and it takes its project group's colour (a colour picked from the same id for a project outside any group). A user-made session group keeps its folder (CDXC:Icons 2026-10-24 in project_header.rs); Classic keeps the letter tile (CDXC:Icons 2026-09-24).

use gpui::{AnyElement, Bounds, Hsla, IntoElement, Styled, canvas, point, px, size};

/// The left half of each creature, centre column included; the right half mirrors it, so every
/// creature is nine pixels wide and eight tall.
const SPRITES: [[&str; 8]; 12] = [
    // Crab.
    [
        "..#..", "...#.", "..###", ".##.#", "#####", "#.###", "#.#..", "...##",
    ],
    // Squid.
    [
        "....#", "...##", "..###", ".##.#", ".####", "...#.", "..#.#", ".#.#.",
    ],
    // Octopus.
    [
        "...##", ".####", "#####", "##..#", "#####", "..##.", ".##.#", "##...",
    ],
    // Ghost.
    [
        "..###", ".####", "#..##", "#..##", "#####", "#####", "#####", "#.#.#",
    ],
    // Robot.
    [
        "....#", ".####", ".#..#", ".####", "..###", "#####", "#.###", "..#..",
    ],
    // Cat.
    [
        "#....", "##...", "#####", "#.###", "#####", "###.#", ".####", ".#...",
    ],
    // Skull.
    [
        "..###", ".####", "##..#", "##..#", "#####", ".##.#", "..###", "..#.#",
    ],
    // Bug.
    [
        ".#...", "..#..", "..###", ".#.##", "#####", ".####", "..#.#", ".#...",
    ],
    // Frog.
    [
        ".##..", "#..##", "#####", "#####", ".####", "#.#..", "##...", ".....",
    ],
    // Mushroom.
    [
        "..###", ".##.#", "#####", "#.###", "#####", "...#.", "..###", "..###",
    ],
    // Bunny.
    [
        ".#...", ".#...", ".####", "##.##", "#####", "###.#", ".####", "..#..",
    ],
    // Jelly.
    [
        "...##", "..###", ".#.##", "#####", "#.#.#", "#.#.#", ".#.#.", ".....",
    ],
];

/// How many creatures there are to pick from.
pub(super) const SPRITE_COUNT: usize = SPRITES.len();

const WIDTH: usize = 9;
const HEIGHT: usize = 8;

/// Colours for projects outside any project group.
const PALETTE: [u32; 7] = [
    0x6b9cf2, 0xe0a050, 0x5fb87a, 0xa585e8, 0xe46fa8, 0x4fbfbf, 0xe8875f,
];

/// FNV-1a, then a finaliser: the same project always lands on the same creature and colour.
///
/// CDXC:Icons 2026-10-27 WHY: FNV-1a alone barely mixes its low bits (a product's low bits depend only on its factors' low bits), so ids that differ late picked the same creature; the finaliser spreads every input bit over the result.
fn stable_hash(id: &str) -> u64 {
    let hash = id.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    let hash = (hash ^ (hash >> 33)).wrapping_mul(0xff51afd7ed558ccd);
    let hash = (hash ^ (hash >> 33)).wrapping_mul(0xc4ceb9fe1a85ec53);
    hash ^ (hash >> 33)
}

/// The colour a project outside any project group gets.
pub(super) fn fallback_color(id: &str) -> Hsla {
    gpui::rgb(PALETTE[(stable_hash(id) >> 8) as usize % PALETTE.len()]).into()
}

/// The creature `id` lands on before its project group's other projects are considered
/// (`TwoColumnFrame` moves it on to a free one when a neighbour in the same card has it).
pub(super) fn preferred_sprite(id: &str) -> usize {
    stable_hash(id) as usize % SPRITE_COUNT
}

/// The filled cells of creature `sprite`, as (column, row).
fn cells(sprite: usize) -> Vec<(usize, usize)> {
    let sprite = &SPRITES[sprite % SPRITE_COUNT];
    let mut cells = Vec::new();
    for (row, half) in sprite.iter().enumerate() {
        let half = half.as_bytes();
        for column in 0..WIDTH {
            let mirrored = if column < half.len() {
                column
            } else {
                WIDTH - 1 - column
            };
            if half[mirrored] == b'#' {
                cells.push((column, row));
            }
        }
    }
    cells
}

/// Creature `sprite` in `color`, `side` pixels square (already zoomed). Its pixels snap to the
/// display's pixel grid so its edges stay crisp at every zoom.
pub(super) fn project_sprite(sprite: usize, color: Hsla, side: f32) -> AnyElement {
    let cells = cells(sprite);
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<gpui::Pixels>, _, window, _| {
            let device = window.scale_factor();
            let available = f32::from(bounds.size.width.min(bounds.size.height)) * device;
            let pixel = (available / WIDTH as f32).floor().max(1.0) / device;
            let left = bounds.origin.x
                + px(
                    ((f32::from(bounds.size.width) - pixel * WIDTH as f32) / 2.0 * device).round()
                        / device,
                );
            let top = bounds.origin.y
                + px(
                    ((f32::from(bounds.size.height) - pixel * HEIGHT as f32) / 2.0 * device)
                        .round()
                        / device,
                );
            for (column, row) in &cells {
                window.paint_quad(gpui::fill(
                    Bounds::new(
                        point(
                            left + px(pixel * *column as f32),
                            top + px(pixel * *row as f32),
                        ),
                        size(px(pixel), px(pixel)),
                    ),
                    color,
                ));
            }
        },
    )
    .size(px(side))
    .flex_shrink_0()
    .into_any_element()
}
