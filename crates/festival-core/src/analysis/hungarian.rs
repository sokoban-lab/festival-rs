//! Minimum-cost assignment (`hungarian.cpp`).
//!
//! The C++ solver keeps a one-entry cache so that evaluating the children of a
//! node only re-solves when a single row changed. The same incremental path is
//! preserved here because it influences tie-breaking and therefore the search.

use crate::constants::MAX_BOXES;

const HUNGARIAN_INF: i32 = 10_000_000;

/// Row-major `n x n` cost matrix. C++ uses the full `MAX_BOXES` square but
/// only ever reads the top-left `n x n` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CostMatrix {
    n: usize,
    data: Vec<i32>,
}

impl CostMatrix {
    pub fn new(n: usize) -> Self {
        assert!(n <= MAX_BOXES, "hungarian size overflow");
        Self {
            n,
            data: vec![0; n * n],
        }
    }

    pub fn from_rows(rows: &[&[i32]]) -> Self {
        let n = rows.len();
        let mut matrix = Self::new(n);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), n, "cost matrix must be square");
            matrix.data[i * n..(i + 1) * n].copy_from_slice(row);
        }
        matrix
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.n
    }

    #[inline]
    pub fn get(&self, i: usize, j: usize) -> i32 {
        self.data[i * self.n + j]
    }

    #[inline]
    pub fn set(&mut self, i: usize, j: usize, value: i32) {
        self.data[i * self.n + j] = value;
    }

    #[inline]
    pub fn row(&self, i: usize) -> &[i32] {
        &self.data[i * self.n..(i + 1) * self.n]
    }

    #[inline]
    pub fn row_mut(&mut self, i: usize) -> &mut [i32] {
        &mut self.data[i * self.n..(i + 1) * self.n]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HungarianSolution {
    pub v: [i32; MAX_BOXES],
    pub u: [i32; MAX_BOXES],
    pub match_v: [i16; MAX_BOXES],
    pub match_u: [i16; MAX_BOXES],
    pub weight: i32,
}

impl Default for HungarianSolution {
    fn default() -> Self {
        Self {
            v: [0; MAX_BOXES],
            u: [0; MAX_BOXES],
            match_v: [-1; MAX_BOXES],
            match_u: [-1; MAX_BOXES],
            weight: 0,
        }
    }
}

impl HungarianSolution {
    #[inline]
    pub fn match_for_row(&self, i: usize) -> Option<usize> {
        let value = self.match_v[i];
        (value >= 0).then_some(value as usize)
    }
}

/// One-entry cache mirroring `hungarian_cache`.
#[derive(Debug)]
pub struct HungarianCache {
    prev_c: Vec<i32>,
    prev_sol: HungarianSolution,
    prev_n: isize,
}

impl Default for HungarianCache {
    fn default() -> Self {
        Self::new()
    }
}

impl HungarianCache {
    pub fn new() -> Self {
        Self {
            prev_c: Vec::new(),
            prev_sol: HungarianSolution::default(),
            prev_n: -1,
        }
    }
}

struct HungarianData {
    n: usize,
    v: [i32; MAX_BOXES],
    u: [i32; MAX_BOXES],
    matched_v: [i16; MAX_BOXES],
    matched_u: [i16; MAX_BOXES],
    match_size: usize,
    src_of_v: [i16; MAX_BOXES],
    src_of_u: [i16; MAX_BOXES],
    visited_v: [bool; MAX_BOXES],
    visited_u: [bool; MAX_BOXES],
    v_list: [u16; MAX_BOXES],
    v_list_num: usize,
    u_list: [u16; MAX_BOXES],
    u_list_num: usize,
}

impl HungarianData {
    fn new(n: usize) -> Self {
        Self {
            n,
            v: [0; MAX_BOXES],
            u: [0; MAX_BOXES],
            matched_v: [-1; MAX_BOXES],
            matched_u: [-1; MAX_BOXES],
            match_size: 0,
            src_of_v: [-1; MAX_BOXES],
            src_of_u: [-1; MAX_BOXES],
            visited_v: [false; MAX_BOXES],
            visited_u: [false; MAX_BOXES],
            v_list: [0; MAX_BOXES],
            v_list_num: 0,
            u_list: [0; MAX_BOXES],
            u_list_num: 0,
        }
    }
}

fn init_u_v(cost: &CostMatrix, hd: &mut HungarianData) {
    let n = hd.n;
    for i in 0..n {
        let mut min = HUNGARIAN_INF;
        for j in 0..n {
            min = min.min(cost.get(i, j));
        }
        hd.v[i] = min;
    }

    for j in 0..n {
        let mut min = HUNGARIAN_INF;
        for i in 0..n {
            min = min.min(cost.get(i, j) - hd.v[i]);
        }
        hd.u[j] = min;
    }
}

fn init_match(cost: &CostMatrix, hd: &mut HungarianData) {
    let n = hd.n;
    hd.match_size = 0;
    for i in 0..n {
        hd.matched_v[i] = -1;
        hd.matched_u[i] = -1;
    }

    for i in 0..n {
        for j in 0..n {
            if hd.matched_u[j] != -1 {
                continue;
            }
            if cost.get(i, j) == hd.v[i] + hd.u[j] {
                hd.matched_v[i] = j as i16;
                hd.matched_u[j] = i as i16;
                hd.match_size += 1;
                break;
            }
        }
    }
}

fn init_hungarian_bfs(hd: &mut HungarianData) {
    let n = hd.n;
    hd.v_list_num = 0;

    for i in 0..n {
        hd.src_of_v[i] = -1;
        hd.src_of_u[i] = -1;
        hd.visited_v[i] = false;
        hd.visited_u[i] = false;
    }

    for i in 0..n {
        if hd.matched_v[i] == -1 {
            hd.v_list[hd.v_list_num] = i as u16;
            hd.v_list_num += 1;
            hd.visited_v[i] = true;
        }
    }
}

fn bfs_v_side(cost: &CostMatrix, hd: &mut HungarianData) -> bool {
    let mut changed = false;
    hd.u_list_num = 0;

    for t in 0..hd.v_list_num {
        let i = hd.v_list[t] as usize;
        for j in 0..hd.n {
            if hd.visited_u[j] {
                continue;
            }
            if hd.v[i] + hd.u[j] == cost.get(i, j) {
                hd.visited_u[j] = true;
                hd.src_of_u[j] = i as i16;
                hd.u_list[hd.u_list_num] = j as u16;
                hd.u_list_num += 1;
                changed = true;
            }
        }
    }
    changed
}

fn bfs_u_side(hd: &mut HungarianData) -> bool {
    let mut changed = false;
    hd.v_list_num = 0;

    for t in 0..hd.u_list_num {
        let j = hd.u_list[t] as usize;
        if hd.matched_u[j] == -1 {
            continue;
        }
        let i = hd.matched_u[j] as usize;
        if hd.visited_v[i] {
            continue;
        }
        hd.v_list[hd.v_list_num] = i as u16;
        hd.v_list_num += 1;
        hd.visited_v[i] = true;
        hd.src_of_v[i] = j as i16;
        changed = true;
    }
    changed
}

fn hungarian_bfs(cost: &CostMatrix, side: usize, hd: &mut HungarianData) {
    let mut side = side;
    loop {
        let res = if side == 0 {
            assert!(hd.v_list_num != 0, "bad hungarian v side");
            bfs_v_side(cost, hd)
        } else {
            assert!(hd.u_list_num != 0, "bad hungarian u side");
            bfs_u_side(hd)
        };
        side = 1 - side;
        if !res {
            break;
        }
    }
}

fn try_to_increase_match(hd: &mut HungarianData) -> bool {
    let n = hd.n;
    for j in 0..n {
        if hd.matched_u[j] != -1 || !hd.visited_u[j] {
            continue;
        }

        let mut j = j as isize;
        while j != -1 {
            let jj = j as usize;
            let i = hd.src_of_u[jj] as usize;
            hd.matched_u[jj] = i as i16;
            hd.matched_v[i] = jj as i16;
            j = hd.src_of_v[i] as isize;
        }
        return true;
    }
    false
}

fn add_new_edges(cost: &CostMatrix, hd: &mut HungarianData) {
    let mut min = HUNGARIAN_INF;
    let n = hd.n;
    hd.u_list_num = 0;

    for i in 0..n {
        if !hd.visited_v[i] {
            continue;
        }
        for j in 0..n {
            if hd.visited_u[j] {
                continue;
            }
            let delta = cost.get(i, j) - hd.v[i] - hd.u[j];
            if delta < min {
                min = delta;
            }
        }
    }

    let delta = min;
    for i in 0..n {
        if hd.visited_v[i] {
            hd.v[i] += delta;
        }
        if hd.visited_u[i] {
            hd.u[i] -= delta;
        }
    }

    for j in 0..n {
        if hd.visited_u[j] {
            continue;
        }
        for i in 0..n {
            if !hd.visited_v[i] {
                continue;
            }
            if hd.v[i] + hd.u[j] == cost.get(i, j) {
                hd.visited_u[j] = true;
                hd.src_of_u[j] = i as i16;
                break;
            }
        }
        if hd.visited_u[j] {
            hd.u_list[hd.u_list_num] = j as u16;
            hd.u_list_num += 1;
        }
    }
}

fn verify_hungarian_invariant(cost: &CostMatrix, hd: &HungarianData) {
    debug_assert!(invariant_holds(cost, hd));
}

fn invariant_holds(cost: &CostMatrix, hd: &HungarianData) -> bool {
    for i in 0..hd.n {
        for j in 0..hd.n {
            if hd.v[i] + hd.u[j] > cost.get(i, j) {
                return false;
            }
        }
    }
    true
}

fn add_edge_to_match(cost: &CostMatrix, hd: &mut HungarianData) {
    init_hungarian_bfs(hd);
    hungarian_bfs(cost, 0, hd);

    loop {
        if try_to_increase_match(hd) {
            return;
        }
        add_new_edges(cost, hd);
        hungarian_bfs(cost, 1, hd);
    }
}

fn verify_hungarian_solution(cost: &CostMatrix, hd: &HungarianData) {
    verify_hungarian_invariant(cost, hd);
    debug_assert!(solution_holds(cost, hd));
}

fn solution_holds(cost: &CostMatrix, hd: &HungarianData) -> bool {
    for i in 0..hd.n {
        let j = hd.matched_v[i];
        if j == -1 || hd.matched_u[j as usize] != i as i16 {
            return false;
        }
        if hd.v[i] + hd.u[j as usize] != cost.get(i, j as usize) {
            return false;
        }
    }
    true
}

fn is_match_full(hd: &HungarianData) -> bool {
    (0..hd.n).all(|i| hd.matched_v[i] != -1)
}

fn do_hungarian_iterations(cost: &CostMatrix, hd: &mut HungarianData) {
    while !is_match_full(hd) {
        add_edge_to_match(cost, hd);
        verify_hungarian_invariant(cost, hd);
    }
    verify_hungarian_solution(cost, hd);
}

fn set_hungarian_solution(cost: &CostMatrix, sol: &mut HungarianSolution, hd: &HungarianData) {
    sol.weight = 0;
    for i in 0..hd.n {
        sol.match_v[i] = hd.matched_v[i];
        sol.match_u[i] = hd.matched_u[i];
        sol.u[i] = hd.u[i];
        sol.v[i] = hd.v[i];
        sol.weight += cost.get(i, hd.matched_v[i] as usize);
    }
}

enum CacheState {
    Rebuild,
    Same,
    OneRow(usize),
}

fn did_one_row_change(matrix: &CostMatrix, hc: &HungarianCache) -> CacheState {
    let size = matrix.size();
    if size as isize != hc.prev_n {
        return CacheState::Rebuild;
    }

    let mut changed_row = None;
    for i in 0..size {
        let mut changed = false;
        for j in 0..size {
            if matrix.get(i, j) != hc.prev_c[i * size + j] {
                changed = true;
                break;
            }
        }
        if changed {
            if changed_row.is_some() {
                return CacheState::Rebuild;
            }
            changed_row = Some(i);
        }
    }

    match changed_row {
        None => CacheState::Same,
        Some(row) => CacheState::OneRow(row),
    }
}

fn init_from_prev(row: usize, cost: &CostMatrix, hd: &mut HungarianData, hc: &HungarianCache) {
    let n = hc.prev_n as usize;
    hd.n = n;
    for i in 0..n {
        hd.v[i] = hc.prev_sol.v[i];
        hd.u[i] = hc.prev_sol.u[i];
        hd.matched_v[i] = hc.prev_sol.match_v[i];
        hd.matched_u[i] = hc.prev_sol.match_u[i];
    }
    let matched_column = hd.matched_v[row] as usize;
    hd.matched_u[matched_column] = -1;
    hd.matched_v[row] = -1;

    let mut min = HUNGARIAN_INF;
    for i in 0..n {
        min = min.min(cost.get(row, i) - hd.u[i]);
    }
    hd.v[row] = min;

    verify_hungarian_invariant(cost, hd);
}

fn save_prev_data(sol: &HungarianSolution, cost: &CostMatrix, hc: &mut HungarianCache) {
    hc.prev_n = cost.size() as isize;
    hc.prev_c.clear();
    hc.prev_c.extend_from_slice(&cost.data);
    hc.prev_sol = *sol;
}

/// Solves the assignment problem and writes the solution into `sol`.
pub fn solve_hungarian(
    n: usize,
    cost: &CostMatrix,
    sol: &mut HungarianSolution,
    hc: &mut HungarianCache,
) {
    assert_eq!(n, cost.size());
    assert!(n <= MAX_BOXES, "hungarian size overflow");

    let cache_state = did_one_row_change(cost, hc);
    let fresh = matches!(cache_state, CacheState::Rebuild);
    if matches!(cache_state, CacheState::Same) {
        *sol = hc.prev_sol;
        return;
    }

    let mut hd = HungarianData::new(n);
    match cache_state {
        CacheState::Rebuild => {
            init_u_v(cost, &mut hd);
            init_match(cost, &mut hd);
        }
        CacheState::OneRow(row) => init_from_prev(row, cost, &mut hd, hc),
        CacheState::Same => unreachable!(),
    }

    do_hungarian_iterations(cost, &mut hd);
    set_hungarian_solution(cost, sol, &hd);

    // Only a freshly built problem is cached; variations reuse the old entry.
    if fresh {
        save_prev_data(sol, cost, hc);
    }
}

/// Solves one child whose cost matrix equals the cached matrix except in
/// `changed_row`. The caller reuses a scratch matrix instead of cloning the
/// full base matrix for every child.
pub(crate) fn solve_hungarian_one_row(
    n: usize,
    cost: &CostMatrix,
    changed_row: usize,
    sol: &mut HungarianSolution,
    hc: &mut HungarianCache,
) {
    assert_eq!(n, cost.size());
    assert_eq!(hc.prev_n, n as isize);
    assert!(n <= MAX_BOXES, "hungarian size overflow");

    debug_assert!(matches!(
        did_one_row_change(cost, hc),
        CacheState::OneRow(row) if row == changed_row
    ));

    let row = cost.row(changed_row);
    let prev_row = &hc.prev_c[changed_row * n..(changed_row + 1) * n];
    if row == prev_row {
        *sol = hc.prev_sol;
        return;
    }

    let mut hd = HungarianData::new(n);
    init_from_prev(changed_row, cost, &mut hd, hc);
    do_hungarian_iterations(cost, &mut hd);
    set_hungarian_solution(cost, sol, &hd);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solve(rows: &[&[i32]]) -> i32 {
        let matrix = CostMatrix::from_rows(rows);
        let mut sol = HungarianSolution::default();
        let mut cache = HungarianCache::new();
        solve_hungarian(matrix.size(), &matrix, &mut sol, &mut cache);
        sol.weight
    }

    fn brute_force(matrix: &CostMatrix) -> i32 {
        fn rec(matrix: &CostMatrix, row: usize, used: &mut [bool], sum: i32, best: &mut i32) {
            if row == matrix.size() {
                *best = (*best).min(sum);
                return;
            }
            for j in 0..matrix.size() {
                if used[j] {
                    continue;
                }
                used[j] = true;
                rec(matrix, row + 1, used, sum + matrix.get(row, j), best);
                used[j] = false;
            }
        }

        let mut best = i32::MAX;
        let mut used = vec![false; matrix.size()];
        rec(matrix, 0, &mut used, 0, &mut best);
        best
    }

    #[test]
    fn diagonal_is_zero() {
        assert_eq!(solve(&[&[0, 5, 5], &[5, 0, 5], &[5, 5, 0]]), 0);
    }

    #[test]
    fn constant_matrix_matches_size() {
        assert_eq!(
            solve(&[&[1, 1, 1, 1], &[1, 1, 1, 1], &[1, 1, 1, 1], &[1, 1, 1, 1]]),
            4
        );
    }

    #[test]
    fn row_and_column_offsets_are_found() {
        assert_eq!(solve(&[&[4, 1, 3], &[2, 0, 5], &[3, 2, 2]]), 5);
    }

    #[test]
    fn random_matrices_match_brute_force() {
        let mut state = 0x1234_5678_u64;
        for n in 1..=6 {
            for _ in 0..20 {
                let mut matrix = CostMatrix::new(n);
                for i in 0..n {
                    for j in 0..n {
                        state = state
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1_442_695_040_888_963_407);
                        matrix.set(i, j, ((state >> 33) % 21) as i32);
                    }
                }

                let mut sol = HungarianSolution::default();
                let mut cache = HungarianCache::new();
                solve_hungarian(n, &matrix, &mut sol, &mut cache);
                assert_eq!(sol.weight, brute_force(&matrix), "n={n}");

                let mut sum = 0;
                let mut columns = vec![false; n];
                for i in 0..n {
                    let j = sol.match_for_row(i).expect("full match");
                    assert!(!columns[j]);
                    columns[j] = true;
                    sum += matrix.get(i, j);
                }
                assert_eq!(sum, sol.weight);
            }
        }
    }

    #[test]
    fn cache_reuses_previous_solution() {
        let matrix = CostMatrix::from_rows(&[&[1, 4, 3], &[2, 0, 5], &[3, 2, 2]]);
        let mut cache = HungarianCache::new();
        let mut sol = HungarianSolution::default();
        solve_hungarian(3, &matrix, &mut sol, &mut cache);
        let first = sol.weight;

        solve_hungarian(3, &matrix, &mut sol, &mut cache);
        assert_eq!(sol.weight, first);
    }

    #[test]
    fn one_changed_row_matches_brute_force() {
        let mut matrix = CostMatrix::from_rows(&[&[1, 4, 3], &[2, 0, 5], &[3, 2, 2]]);
        let mut cache = HungarianCache::new();
        let mut sol = HungarianSolution::default();
        solve_hungarian(3, &matrix, &mut sol, &mut cache);

        matrix.set(1, 0, 0);
        matrix.set(1, 1, 0);
        matrix.set(1, 2, 0);
        solve_hungarian(3, &matrix, &mut sol, &mut cache);
        assert_eq!(sol.weight, brute_force(&matrix));
    }

    #[test]
    fn one_row_update_matches_full_solve() {
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for n in 1..=8 {
            for _ in 0..20 {
                let mut base = CostMatrix::new(n);
                for i in 0..n {
                    for j in 0..n {
                        state = state
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1_442_695_040_888_963_407);
                        base.set(i, j, ((state >> 33) % 31) as i32);
                    }
                }

                let mut full_cache = HungarianCache::new();
                let mut row_cache = HungarianCache::new();
                let mut full_sol = HungarianSolution::default();
                let mut row_sol = HungarianSolution::default();
                solve_hungarian(n, &base, &mut full_sol, &mut full_cache);
                solve_hungarian(n, &base, &mut row_sol, &mut row_cache);
                assert_eq!(full_sol, row_sol);

                let mut scratch = base.clone();
                for row in 0..n {
                    if row > 0 {
                        scratch.row_mut(row - 1).copy_from_slice(base.row(row - 1));
                    }
                    for j in 0..n {
                        state = state
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1_442_695_040_888_963_407);
                        scratch.set(row, j, ((state >> 33) % 31) as i32);
                    }

                    solve_hungarian(n, &scratch, &mut full_sol, &mut full_cache);
                    solve_hungarian_one_row(n, &scratch, row, &mut row_sol, &mut row_cache);
                    assert_eq!(full_sol, row_sol, "n={n}, row={row}");
                }
            }
        }
    }
}
