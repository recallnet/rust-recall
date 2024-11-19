use anyhow::anyhow;
use dare::{HEADER_SIZE, MAX_PAYLOAD_SIZE, TAG_SIZE};

#[derive(Clone, Copy)]
pub struct Range {
    start: Option<u64>,
    end: Option<u64>,
}

impl Range {
    pub fn parse(range: &str) -> anyhow::Result<Self> {
        let range: Vec<String> = range
            .replace("bytes=", "")
            .split('-')
            .map(|n| n.to_string())
            .collect();
        if range.len() != 2 {
            return Err(anyhow!("invalid range"));
        }

        let start = if !range[0].is_empty() {
            Some(range[0].parse::<u64>()?)
        } else {
            None
        };

        let end = if !range[1].is_empty() {
            Some(range[1].parse::<u64>()?)
        } else {
            None
        };

        Ok(Range { start, end })
    }

    pub fn get_range(&self, size: u64, is_encrypted: bool) -> String {
        if !is_encrypted {
            let (offset, length) = self.get_offset_length(size);
            return format!("{}-{}", offset, offset + length + 1);
        }

        self.get_range_for_encrypted(size)
    }

    pub fn get_offset_length(&self, size: u64) -> (u64, u64) {
        match (self.start, self.end) {
            (Some(start), Some(end)) if start <= end => {
                if size <= end {
                    return (start, size - 1);
                }
                (start, end - start + 1)
            }
            (Some(start), None) if start < size => (start, size - start),
            (None, Some(end)) if end > 0 => {
                if end <= size {
                    return (size - end, end);
                }

                (0, size)
            }
            _ => unreachable!("this state is not possible"),
        }
    }

    fn get_range_for_encrypted(&self, size: u64) -> String {
        let (offset, length) = self.get_offset_length(size);

        let last_package_index = size / MAX_PAYLOAD_SIZE as u64;
        let start_package_index = offset / MAX_PAYLOAD_SIZE as u64;
        let end_package_index = (offset + length) / MAX_PAYLOAD_SIZE as u64;

        let package_size = (HEADER_SIZE + MAX_PAYLOAD_SIZE + TAG_SIZE) as u64;

        if end_package_index < last_package_index {
            return format!(
                "{}-{}",
                start_package_index * package_size,
                (end_package_index + 1) * package_size - 1
            );
        }

        format!(
            "{}-{}",
            start_package_index * package_size,
            size + (last_package_index + 1) * (HEADER_SIZE + TAG_SIZE) as u64 - 1
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::range::Range;

    #[test]
    fn test_offset_length() {
        let object_size = 10;
        struct RangeSpec {
            spec: &'static str,
            exp_offset: u64,
            exp_length: u64,
        }

        let valid_range_specs = vec![
            RangeSpec {
                spec: "bytes=0-",
                exp_offset: 0,
                exp_length: 10,
            },
            RangeSpec {
                spec: "bytes=1-",
                exp_offset: 1,
                exp_length: 9,
            },
            RangeSpec {
                spec: "bytes=0-9",
                exp_offset: 0,
                exp_length: 10,
            },
            RangeSpec {
                spec: "bytes=1-10",
                exp_offset: 1,
                exp_length: 9,
            },
            RangeSpec {
                spec: "bytes=1-1",
                exp_offset: 1,
                exp_length: 1,
            },
            RangeSpec {
                spec: "bytes=2-5",
                exp_offset: 2,
                exp_length: 4,
            },
            RangeSpec {
                spec: "bytes=-5",
                exp_offset: 5,
                exp_length: 5,
            },
            RangeSpec {
                spec: "bytes=-1",
                exp_offset: 9,
                exp_length: 1,
            },
            RangeSpec {
                spec: "bytes=-1000",
                exp_offset: 0,
                exp_length: 10,
            },
        ];

        for range_spec in valid_range_specs {
            let range = Range::parse(range_spec.spec).expect("range should be valid");

            let (offset, length) = range.get_offset_length(object_size);

            assert_eq!(range_spec.exp_offset, offset);
            assert_eq!(range_spec.exp_length, length);
        }
    }

    #[test]
    fn test_get_range_for_encrypted() {
        // Object bigger than MAX_PAYLOAD_SIZE.
        // It means that the encrypted object has two packages.
        let object_size = 70000;
        struct RangeSpec {
            spec: &'static str,
            exp_encrypted_range: &'static str,
        }

        let specs = vec![
            // range is inside first package
            RangeSpec {
                spec: "bytes=60000-60002",
                exp_encrypted_range: "0-65567",
            },
            // range spreads across two packages
            RangeSpec {
                spec: "bytes=60000-67000",
                exp_encrypted_range: "0-70063",
            },
            // ranges is inside second package
            RangeSpec {
                spec: "bytes=67000-68000",
                exp_encrypted_range: "65568-70063",
            },
        ];

        for range_spec in specs {
            let range = Range::parse(range_spec.spec).expect("range should be valid");

            assert_eq!(
                range_spec.exp_encrypted_range,
                range.get_range_for_encrypted(object_size)
            );
        }
    }
}
