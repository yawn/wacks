//! Delta encoding for sorted u32 sequences.
//!
//! Stores differences between consecutive values instead of absolute
//! values, yielding smaller varints when serialized with postcard.

/// A delta-encoded sequence of sorted u32 values.
pub(crate) struct Delta;

impl Delta {
    /// Delta-encode a sorted sequence of absolute u32 values.
    pub(crate) fn encode(values: &[u32]) -> Vec<u32> {
        let mut prev = 0u32;
        values
            .iter()
            .map(|&v| {
                let delta = v - prev;
                prev = v;
                delta
            })
            .collect()
    }

    /// Reconstruct absolute values from a delta-encoded sequence.
    pub(crate) fn decode(deltas: &[u32]) -> Vec<u32> {
        let mut acc = 0u32;
        deltas
            .iter()
            .map(|&d| {
                acc += d;
                acc
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty() {
        assert_eq!(Delta::encode(&[]), Vec::<u32>::new());
    }

    #[test]
    fn decode_empty() {
        assert_eq!(Delta::decode(&[]), Vec::<u32>::new());
    }

    #[test]
    fn roundtrip_single() {
        let values = vec![42];
        assert_eq!(Delta::decode(&Delta::encode(&values)), values);
    }

    #[test]
    fn roundtrip_ascending() {
        let values = vec![100, 200, 300, 1000];
        let encoded = Delta::encode(&values);
        assert_eq!(encoded, vec![100, 100, 100, 700]);
        assert_eq!(Delta::decode(&encoded), values);
    }

    #[test]
    fn roundtrip_duplicates() {
        let values = vec![5, 5, 5, 10];
        let encoded = Delta::encode(&values);
        assert_eq!(encoded, vec![5, 0, 0, 5]);
        assert_eq!(Delta::decode(&encoded), values);
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Generate a sorted Vec<u32> by accumulating small deltas.
        fn sorted_u32s(max_len: usize) -> impl Strategy<Value = Vec<u32>> {
            proptest::collection::vec(0u32..10_000, 0..max_len).prop_map(|deltas| {
                let mut acc = 0u32;
                deltas
                    .into_iter()
                    .map(|d| {
                        acc = acc.saturating_add(d);
                        acc
                    })
                    .collect()
            })
        }

        proptest! {
            #[test]
            fn roundtrip(values in sorted_u32s(500)) {
                let decoded = Delta::decode(&Delta::encode(&values));
                prop_assert_eq!(decoded, values);
            }
        }
    }
}
