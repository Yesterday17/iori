use std::cmp::Ordering;

use dash_mpd::Representation;

pub fn best_representation(representation: &Representation) -> impl Ord {
    BestRepresentationSelector {
        width: representation.width,
        height: representation.height,
        bandwidth: representation.bandwidth,
    }
}

#[derive(PartialEq, Eq)]
struct BestRepresentationSelector {
    width: Option<u64>,
    height: Option<u64>,
    bandwidth: Option<u64>,
}

impl PartialOrd for BestRepresentationSelector {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BestRepresentationSelector {
    fn cmp(&self, other: &Self) -> Ordering {
        self.width
            .cmp(&other.width)
            .then(self.height.cmp(&other.height))
            .then(self.bandwidth.cmp(&other.bandwidth))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_best_representation() {
        let representations = [
            BestRepresentationSelector {
                width: Some(1920),
                height: Some(1080),
                bandwidth: Some(1000000),
            },
            BestRepresentationSelector {
                width: Some(1280),
                height: Some(720),
                bandwidth: Some(500000),
            },
            BestRepresentationSelector {
                width: Some(640),
                height: Some(360),
                bandwidth: Some(250000),
            },
        ];

        let best = representations.iter().max().unwrap();
        assert_eq!(best.width, Some(1920));
        assert_eq!(best.height, Some(1080));
        assert_eq!(best.bandwidth, Some(1000000));
    }

    #[test]
    fn test_resolution_first() {
        let representations = [
            BestRepresentationSelector {
                width: Some(1920),
                height: Some(1080),
                bandwidth: Some(500000),
            },
            BestRepresentationSelector {
                width: Some(1280),
                height: Some(720),
                bandwidth: Some(1000000),
            },
        ];

        let best = representations.iter().max().unwrap();
        assert_eq!(best.width, Some(1920));
        assert_eq!(best.height, Some(1080));
        assert_eq!(best.bandwidth, Some(500000));
    }
}
