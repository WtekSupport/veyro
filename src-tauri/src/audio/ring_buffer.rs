/// Fixed-capacity ring buffer for audio samples.
/// When full, oldest samples are overwritten.
pub struct RingBuffer {
    data: Vec<f32>,
    write_pos: usize,
    len: usize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0.0; capacity],
            write_pos: 0,
            len: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn push_slice(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.data[self.write_pos] = sample;
            self.write_pos = (self.write_pos + 1) % self.data.len();
            if self.len < self.data.len() {
                self.len += 1;
            }
        }
    }

    /// Read the last `count` samples in chronological order.
    pub fn read_last(&self, count: usize) -> Vec<f32> {
        let count = count.min(self.len);
        if count == 0 {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(count);
        let start = (self.write_pos + self.data.len() - count) % self.data.len();
        for i in 0..count {
            out.push(self.data[(start + i) % self.data.len()]);
        }
        out
    }

    pub fn clear(&mut self) {
        self.write_pos = 0;
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overwrites_oldest_when_full() {
        let mut buf = RingBuffer::new(4);
        buf.push_slice(&[1.0, 2.0, 3.0, 4.0]);
        buf.push_slice(&[5.0]);
        assert_eq!(buf.read_last(4), vec![2.0, 3.0, 4.0, 5.0]);
    }

    #[test]
    fn read_last_returns_chronological_order() {
        let mut buf = RingBuffer::new(8);
        buf.push_slice(&[1.0, 2.0, 3.0]);
        assert_eq!(buf.read_last(2), vec![2.0, 3.0]);
    }
}
