pub trait VecMix {
    type Item;

    fn mix(self) -> Vec<Self::Item>;
}

impl<T> VecMix for Vec<Vec<T>> {
    type Item = T;

    fn mix(self) -> Vec<Self::Item> {
        // Merge vectors by interleaving their elements
        // For example: [[a1, a2, a3], [b1, b2, b3]] -> [a1, b1, a2, b2, a3, b3]
        let total_len = self.iter().map(|v| v.len()).sum();
        let mut result = Vec::with_capacity(total_len);

        let mut iters: Vec<_> = self
            .into_iter()
            .map(|v| v.into_iter())
            .filter(|iter| iter.len() > 0)
            .collect();

        while !iters.is_empty() {
            iters.retain_mut(|iter| {
                if let Some(item) = iter.next() {
                    result.push(item);
                    true
                } else {
                    false
                }
            });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mix_single_vec() {
        let mixed_vec = vec![vec![1, 3, 5]].mix();
        assert_eq!(mixed_vec, vec![1, 3, 5]);
    }

    #[test]
    fn test_mix_vec() {
        let mixed_vec = vec![vec![1, 3, 5], vec![2, 4, 6]].mix();
        assert_eq!(mixed_vec, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_mix_vec_empty() {
        let mixed_vec = vec![vec![], vec![1, 2, 3]].mix();
        assert_eq!(mixed_vec, vec![1, 2, 3]);
    }

    #[test]
    fn test_mix_vec_different_length() {
        let mixed_vec: Vec<i32> = vec![vec![1, 2, 3, 4, 5, 6], vec![7, 8, 9]].mix();
        assert_eq!(mixed_vec, vec![1, 7, 2, 8, 3, 9, 4, 5, 6]);
    }
}
