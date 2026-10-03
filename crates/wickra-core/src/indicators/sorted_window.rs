//! A sliding window's values kept in [`f64::total_cmp`] order as it slides.
//!
//! Several statistics (quantiles, medians, tails) read the window sorted. Sorting
//! a copy on every update costs `O(n log n)`; keeping the copy sorted costs one
//! binary search and one shift per value entering or leaving, `O(n)`. Equal
//! under `total_cmp` means bit-identical, so a multiset of `f64`s has exactly
//! one sorted order: the maintained vector is, bit for bit, what sorting the
//! window would produce.

/// Insert `value` into `sorted` (ascending by [`f64::total_cmp`]).
pub(crate) fn insert(sorted: &mut Vec<f64>, value: f64) {
    let at = sorted.partition_point(|v| v.total_cmp(&value).is_lt());
    sorted.insert(at, value);
}

/// Remove one occurrence of `value` from `sorted` (ascending by
/// [`f64::total_cmp`]); `value` must be present.
pub(crate) fn remove(sorted: &mut Vec<f64>, value: f64) {
    let at = sorted.partition_point(|v| v.total_cmp(&value).is_lt());
    debug_assert_eq!(sorted[at].to_bits(), value.to_bits(), "value not in window");
    sorted.remove(at);
}

/// `|x - center|` for every `x` of `sorted`, into `out` in ascending
/// [`f64::total_cmp`] order.
///
/// Below `center` the deviations fall as `x` rises and from it on they rise
/// (subtraction rounds monotonically, and distinct values never subtract to
/// zero), so merging the two runs gives, bit for bit, what sorting the
/// deviations would -- in `O(n)` rather than `O(n log n)`.
pub(crate) fn abs_deviations(sorted: &[f64], center: f64, out: &mut Vec<f64>) {
    distances(sorted, center, |x| (x - center).abs(), out);
}

/// `distance(x)` for every `x` of `sorted`, into `out` in ascending
/// [`f64::total_cmp`] order, for a `distance` that never rises as `x` climbs
/// towards `center` and never falls from it on (as [`abs_deviations`]); the
/// two runs are merged rather than the results sorted.
pub(crate) fn distances(
    sorted: &[f64],
    center: f64,
    distance: impl Fn(f64) -> f64,
    out: &mut Vec<f64>,
) {
    out.clear();
    let split = sorted.partition_point(|&x| x < center);
    let (below, above) = sorted.split_at(split);
    let mut left = below.iter().rev().map(|&x| distance(x)).peekable();
    let mut right = above.iter().map(|&x| distance(x)).peekable();
    while let (Some(&l), Some(&r)) = (left.peek(), right.peek()) {
        if l.total_cmp(&r).is_le() {
            out.push(l);
            left.next();
        } else {
            out.push(r);
            right.next();
        }
    }
    out.extend(left);
    out.extend(right);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abs_deviations_are_the_sorted_deviations() {
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        let windows: [&[f64]; 5] = [
            &[1.0, 2.0, 3.0, 10.0, 11.0],
            &[-9.0, -8.5, 0.0, 0.25, 0.5],
            &[-0.0, 0.0, 0.0, 1.0],
            &[5.0],
            &[1.0, 1.0, 1.0, 7.0, 100.0, 100.0],
        ];
        let mut out = Vec::new();
        for window in windows {
            let mut sorted = window.to_vec();
            sorted.sort_by(f64::total_cmp);
            for center in [sorted[sorted.len() / 2], -0.0, 0.3, 1e6, -1e6] {
                abs_deviations(&sorted, center, &mut out);
                let mut want: Vec<f64> = sorted.iter().map(|&x| (x - center).abs()).collect();
                want.sort_by(f64::total_cmp);
                assert_eq!(bits(&out), bits(&want), "{window:?} about {center}");
            }
        }
    }

    #[test]
    fn keeps_the_order_sorting_a_copy_gives() {
        let values = [3.5, -1.0, 0.0, -0.0, 3.5, 1e300, -7.25, 0.5, 2.0, -1.0];
        let mut sorted = Vec::new();
        let mut window = std::collections::VecDeque::new();
        for (i, &value) in values.iter().cycle().take(40).enumerate() {
            let value = value + f64::from(u32::try_from(i % 3).unwrap());
            if window.len() == 5 {
                remove(&mut sorted, window.pop_front().unwrap());
            }
            window.push_back(value);
            insert(&mut sorted, value);
            let mut copy: Vec<f64> = window.iter().copied().collect();
            copy.sort_by(f64::total_cmp);
            let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&sorted), bits(&copy));
        }
    }
}
