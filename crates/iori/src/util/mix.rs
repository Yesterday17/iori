pub fn mix_vec<T>(vecs: Vec<Vec<T>>) -> Vec<T> {
    // Merge vectors by interleaving their elements
    // For example: [[a1, a2, a3], [b1, b2, b3]] -> [a1, b1, a2, b2, a3, b3]
    let max_len = vecs.iter().map(|v| v.len()).max().unwrap_or(0);
    let total_len = vecs.iter().map(|v| v.len()).sum();
    let mut result = Vec::with_capacity(total_len);

    // Consume the input vectors
    let mut vecs = vecs;
    for _ in 0..max_len {
        for vec in &mut vecs {
            if !vec.is_empty() {
                // TODO: avoid array shift
                let item = vec.remove(0);
                result.push(item);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mix_vec() {
        let vecs = vec![vec![1, 3, 5], vec![2, 4, 6]];
        let mixed_vec = mix_vec(vecs);
        assert_eq!(mixed_vec, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_mix_vec_empty() {
        let vecs = vec![vec![], vec![1, 2, 3]];
        let mixed_vec = mix_vec(vecs);
        assert_eq!(mixed_vec, vec![1, 2, 3]);
    }

    #[test]
    fn test_mix_vec_different_length() {
        let vecs = vec![vec![1, 2, 3, 4, 5, 6], vec![7, 8, 9]];
        let mixed_vec = mix_vec(vecs);
        assert_eq!(mixed_vec, vec![1, 7, 2, 8, 3, 9, 4, 5, 6]);
    }
}
