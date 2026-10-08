//! Block interleaver (SPEC §8.3): write row by row, read column by column.

/// Permutation table: `out[i] = in[perm[i]]`.
pub fn permutation(rows: usize, cols: usize) -> Vec<usize> {
    let mut p = Vec::with_capacity(rows * cols);
    for c in 0..cols {
        for r in 0..rows {
            p.push(r * cols + c);
        }
    }
    p
}

pub fn interleave<T: Copy>(input: &[T], rows: usize, cols: usize) -> Vec<T> {
    assert_eq!(input.len(), rows * cols);
    permutation(rows, cols).iter().map(|&i| input[i]).collect()
}

pub fn deinterleave<T: Copy + Default>(input: &[T], rows: usize, cols: usize) -> Vec<T> {
    assert_eq!(input.len(), rows * cols);
    let mut out = vec![T::default(); input.len()];
    for (i, &src) in permutation(rows, cols).iter().enumerate() {
        out[src] = input[i];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_spreads_runs() {
        let input: Vec<u16> = (0..512).collect();
        let il = interleave(&input, 16, 32);
        assert_eq!(deinterleave(&il, 16, 32), input);
        // Four consecutive on-air bits (one symbol) come from four different rows.
        assert_eq!(&il[..4], &[0, 32, 64, 96]);
    }
}
