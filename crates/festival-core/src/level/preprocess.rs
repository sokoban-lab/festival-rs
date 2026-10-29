use crate::board::Board;
use crate::constants::*;
use crate::error::{Result, SolveError};
use crate::io::sok::RawLevel;

#[derive(Clone, Debug)]
pub struct Level {
    /// Board after preprocessing, with the player's reachable cloud expanded.
    pub board: Board,
    /// Board saved before expanding the cloud (C++ `initial_board`).
    pub initial: Board,
    pub title: String,
    pub inner: [bool; MAX_INNER],
    pub index_of: [i16; MAX_INNER],
    pub coords: Vec<(u8, u8)>,
    pub initial_sokoban: (usize, usize),
    pub box_count: usize,
    pub target_holes: Board,
    pub semi_holes: Board,
    pub forbidden_push: Board,
    pub forbidden_pull: Board,
}

impl Level {
    /// Runs the deterministic part of `preprocess_level` from the C++ solver.
    pub fn preprocess(raw: RawLevel) -> Result<Self> {
        if raw.size_error {
            return Err(SolveError::SizeTooBig);
        }
        let mut board = raw.board.ok_or(SolveError::SizeTooBig)?;

        sanity_checks(&board)?;

        if board.sokoban_position().is_none() {
            return Err(SolveError::InvalidBoard);
        }

        board = turn_decorative_boxes_to_walls(&board);
        close_holes_in_board(&mut board);

        let inner = init_inner(&board);
        let (index_of, coords) = init_index_x_y(&inner)?;

        board.keep_boxes_in_inner(&inner);

        let initial = board;
        let initial_sokoban = initial.sokoban_position().ok_or(SolveError::InvalidBoard)?;
        let box_count = initial.boxes_in_level();

        board.expand_sokoban_cloud();

        let (forbidden_push, forbidden_pull) = set_forbidden_tunnel(&initial)?;
        let (target_holes, semi_holes) = crate::analysis::holes::mark_target_holes(&board);

        Ok(Self {
            board,
            initial,
            title: raw.title,
            inner,
            index_of,
            coords,
            initial_sokoban,
            box_count,
            target_holes,
            semi_holes,
            forbidden_push,
            forbidden_pull,
        })
    }
}

pub fn player_can_leave_board(board: &Board) -> bool {
    let mut b = board.without_boxes();
    let height = board.height();
    let width = board.width();
    let mut queue: Vec<(usize, usize)> = Vec::with_capacity(width * height);

    for y in 0..height {
        for x in 0..width {
            if b.cell(y, x) & SOKOBAN != 0 {
                queue.push((y, x));
            }
        }
    }

    let mut pos = 0;
    while pos < queue.len() {
        let (y, x) = queue[pos];
        pos += 1;

        for dir in 0..4 {
            let ny = y as isize + DELTA_Y[dir];
            let nx = x as isize + DELTA_X[dir];

            if ny < 0 || nx < 0 || ny >= height as isize || nx >= width as isize {
                return true;
            }

            let (ny, nx) = (ny as usize, nx as usize);
            let cell = b.cell(ny, nx);
            if cell & SOKOBAN != 0 || cell & OCCUPIED != 0 {
                continue;
            }
            b.set(ny, nx, cell | SOKOBAN);
            queue.push((ny, nx));
        }
    }

    false
}

pub fn sanity_checks(board: &Board) -> Result<()> {
    if player_can_leave_board(board) {
        return Err(SolveError::InvalidBoard);
    }
    if board.boxes_in_level() >= MAX_BOXES {
        return Err(SolveError::TooManyBoxes);
    }
    Ok(())
}

pub fn close_holes_in_board(board: &mut Board) {
    let height = board.height();
    let width = board.width();
    let mut changed = true;
    while changed {
        changed = false;
        for y in 1..height.saturating_sub(1) {
            for x in 1..width.saturating_sub(1) {
                if board.cell(y, x) != 0 {
                    continue;
                }
                let mut walls = 0;
                for dir in 0..4 {
                    if board.at_dir(y, x, dir) == WALL {
                        walls += 1;
                    }
                }
                if walls >= 3 {
                    board.set(y, x, WALL);
                    changed = true;
                }
            }
        }
    }
}

fn remove_boxes_out_of_inner(board: &mut Board) {
    let mut reachable = board.without_boxes();
    reachable.expand_sokoban_cloud();
    for y in 0..board.height() {
        for x in 0..board.width() {
            if reachable.cell(y, x) & SOKOBAN == 0 {
                board.and_not(y, x, BOX | TARGET);
            }
        }
    }
}

pub fn turn_decorative_boxes_to_walls(board: &Board) -> Board {
    let mut b = *board;
    remove_boxes_out_of_inner(&mut b);

    let mut c = b.without_boxes();

    // Boxes in the reachable area may be pullable. Start from targets (which
    // are implicitly boxes in the final position) and eliminate any box that
    // could be pulled or pushed out.
    for y in 0..c.height() {
        for x in 0..c.width() {
            if c.cell(y, x) & TARGET != 0 {
                c.set(y, x, BOX);
            }
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for y in 0..c.height() {
            for x in 0..c.width() {
                if c.cell(y, x) & BOX == 0 {
                    continue;
                }
                for dir in 0..4 {
                    let ny = y as isize + DELTA_Y[dir];
                    let nx = x as isize + DELTA_X[dir];
                    if c.get(ny, nx) & OCCUPIED != 0 {
                        continue;
                    }
                    let ny2 = y as isize + DELTA_Y[dir] * 2;
                    let nx2 = x as isize + DELTA_X[dir] * 2;
                    if c.get(ny2, nx2) & OCCUPIED != 0 {
                        continue;
                    }
                    c.and_not(y, x, BOX);
                    changed = true;
                }
            }
        }
    }

    for y in 0..b.height() {
        for x in 0..b.width() {
            if c.cell(y, x) & BOX != 0 {
                b.set(y, x, WALL);
            }
        }
    }

    // Now eliminate boxes that can be pushed.
    c = b;
    changed = true;
    while changed {
        changed = false;
        for y in 0..c.height() {
            for x in 0..c.width() {
                if c.cell(y, x) & BOX == 0 {
                    continue;
                }
                for dir in 0..4 {
                    let ny = y as isize + DELTA_Y[dir];
                    let nx = x as isize + DELTA_X[dir];
                    if c.get(ny, nx) & OCCUPIED != 0 {
                        continue;
                    }
                    let ny = y as isize - DELTA_Y[dir];
                    let nx = x as isize - DELTA_X[dir];
                    if c.get(ny, nx) & OCCUPIED != 0 {
                        continue;
                    }
                    c.and_not(y, x, BOX);
                    changed = true;
                }
            }
        }
    }

    for y in 0..b.height() {
        for x in 0..b.width() {
            if c.cell(y, x) & BOX != 0 {
                b.set(y, x, WALL);
            }
        }
    }

    b
}

pub fn init_inner(board: &Board) -> [bool; MAX_INNER] {
    let mut reachable = board.without_boxes();
    reachable.expand_sokoban_cloud();
    let mut inner = [false; MAX_INNER];
    for y in 0..board.height() {
        for x in 0..board.width() {
            inner[y * MAX_SIZE + x] = reachable.cell(y, x) & SOKOBAN != 0;
        }
    }
    inner
}

pub type IndexMap = ([i16; MAX_INNER], Vec<(u8, u8)>);

pub fn init_index_x_y(inner: &[bool; MAX_INNER]) -> Result<IndexMap> {
    let mut index_of = [-1i16; MAX_INNER];
    let mut coords = Vec::new();

    for y in 0..MAX_SIZE {
        for x in 0..MAX_SIZE {
            let flat = y * MAX_SIZE + x;
            if !inner[flat] {
                continue;
            }
            let index = coords.len();
            if index >= MAX_INNER {
                return Err(SolveError::SizeTooBig);
            }
            index_of[flat] = index as i16;
            coords.push((y as u8, x as u8));
        }
    }

    Ok((index_of, coords))
}

pub fn set_forbidden_tunnel(initial: &Board) -> Result<(Board, Board)> {
    let width = initial.width();
    let height = initial.height();
    let mut forbidden_push = Board::new(width, height)?;
    let mut forbidden_pull = Board::new(width, height)?;

    let mut b = initial.without_boxes();
    b.clear_sokoban_inplace();

    // horizontal tunnel
    for i in 1..height.saturating_sub(1) {
        for j in 1..width.saturating_sub(1) {
            if b.get(i as isize - 1, j as isize - 1) == WALL
                && b.get(i as isize, j as isize - 1) != WALL
                && b.get(i as isize + 1, j as isize - 1) == WALL
            {
                continue;
            }

            let mut k = 0;
            while k < width {
                if b.get(i as isize - 1, (j + k) as isize) != WALL {
                    break;
                }
                if b.get(i as isize, (j + k) as isize) == WALL {
                    break;
                }
                if b.get(i as isize + 1, (j + k) as isize) != WALL {
                    break;
                }
                k += 1;
            }

            if k >= 2 {
                let n = k;
                let mut base = false;
                let mut target = false;
                for k in 0..n {
                    if initial.cell(i, j + k) & BOX != 0 {
                        base = true;
                    }
                    if b.cell(i, j + k) & TARGET != 0 {
                        target = true;
                    }
                }
                for k in 0..n {
                    forbidden_push.set(i, j + k, 1);
                    forbidden_pull.set(i, j + k, 1);
                }
                if !base {
                    forbidden_pull.set(i, j, 0);
                }
                if !target {
                    forbidden_push.set(i, j, 0);
                }
            }
        }
    }

    // vertical tunnel
    for i in 1..height.saturating_sub(1) {
        for j in 1..width.saturating_sub(1) {
            if b.get(i as isize - 1, j as isize - 1) == WALL
                && b.get(i as isize - 1, j as isize) != WALL
                && b.get(i as isize - 1, j as isize + 1) == WALL
            {
                continue;
            }

            let mut k = 0;
            while k < height {
                if b.get((i + k) as isize, j as isize - 1) != WALL {
                    break;
                }
                if b.get((i + k) as isize, j as isize) == WALL {
                    break;
                }
                if b.get((i + k) as isize, j as isize + 1) != WALL {
                    break;
                }
                k += 1;
            }

            if k >= 2 {
                let n = k;
                let mut base = false;
                let mut target = false;
                for k in 0..n {
                    if initial.cell(i + k, j) & BOX != 0 {
                        base = true;
                    }
                    if b.cell(i + k, j) & TARGET != 0 {
                        target = true;
                    }
                }
                for k in 0..n {
                    forbidden_push.set(i + k, j, 1);
                    forbidden_pull.set(i + k, j, 1);
                }
                if !base {
                    forbidden_pull.set(i, j, 0);
                }
                if !target {
                    forbidden_push.set(i, j, 0);
                }
            }
        }
    }

    Ok((forbidden_push, forbidden_pull))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::sok::parse_levels;

    fn level(text: &str) -> Level {
        let raw = parse_levels(text).remove(0);
        Level::preprocess(raw).unwrap()
    }

    #[test]
    fn preprocesses_simple_level() {
        let level = level("#######\n#  .  #\n#  $  #\n#  @  #\n#######\n");
        assert_eq!(level.box_count, 1);
        assert_eq!(level.initial_sokoban, (3, 3));
        assert!(level.is_inner(3, 3));
        assert!(level.is_inner(1, 1));
        assert_eq!(level.index_num(), level.coords.len());
        assert!(level.index_num() > 0);
    }

    #[test]
    fn rejects_board_with_open_edge() {
        let raw = parse_levels("#####\n#@$.#\n").remove(0);
        assert!(matches!(
            Level::preprocess(raw),
            Err(SolveError::InvalidBoard)
        ));
    }

    #[test]
    fn rejects_too_many_boxes() {
        let mut rows = vec!["#".repeat(MAX_SIZE)];
        let mut middle = String::from("#");
        for _ in 0..MAX_BOXES {
            middle.push('$');
        }
        middle.push('#');
        let width = middle.len();
        let middle = format!("{middle:<width$}");
        rows.push(middle);
        rows.push("#".repeat(width));
        let text = rows.join("\n");
        let raw = parse_levels(&text).remove(0);
        assert!(matches!(
            Level::preprocess(raw),
            Err(SolveError::TooManyBoxes) | Err(SolveError::SizeTooBig)
        ));
    }

    #[test]
    fn inner_is_player_reachable_area() {
        let level = level("######\n#@   #\n#    #\n######\n");
        assert!(level.is_inner(1, 1));
        assert!(level.is_inner(2, 4));
    }

    #[test]
    fn decorative_box_outside_player_area_becomes_wall() {
        let level = level("#######\n#@    #\n#######\n#  $  #\n#######\n");
        assert_eq!(level.box_count, 0);
        assert_eq!(level.initial.cell(3, 3), WALL);
    }
}
