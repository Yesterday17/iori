use std::collections::BTreeMap;
use tokio::sync::mpsc;

pub struct OrderedStream<T> {
    buffer: BTreeMap<u64, T>,
    next_seq: u64,
    rx: mpsc::UnboundedReceiver<(u64, T)>,
}

impl<T> OrderedStream<T> {
    pub fn new(rx: mpsc::UnboundedReceiver<(u64, T)>) -> Self {
        Self {
            buffer: BTreeMap::new(),
            next_seq: 0,
            rx,
        }
    }

    pub async fn next(&mut self) -> Option<T> {
        loop {
            // Check if we have the next item in buffer
            if let Some(item) = self.buffer.remove(&self.next_seq) {
                self.next_seq += 1;
                return Some(item);
            }

            // Receive new item
            match self.rx.recv().await {
                Some((seq, item)) => {
                    if seq == self.next_seq {
                        self.next_seq += 1;
                        return Some(item);
                    }
                    self.buffer.insert(seq, item);
                }
                None => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ordered_stream() {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut ordered = OrderedStream::new(rx);

        // Send items out of order
        tokio::spawn(async move {
            tx.send((2, "c")).unwrap();
            tx.send((0, "a")).unwrap();
            tx.send((1, "b")).unwrap();
            drop(tx);
        });

        // Receive items in order
        assert_eq!(ordered.next().await.unwrap(), "a");
        assert_eq!(ordered.next().await.unwrap(), "b");
        assert_eq!(ordered.next().await.unwrap(), "c");
        assert_eq!(ordered.next().await, None);
    }
}
