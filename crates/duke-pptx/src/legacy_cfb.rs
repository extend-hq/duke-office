use std::collections::BTreeSet;

const FREE: u32 = 0xffff_ffff;
const END: u32 = 0xffff_fffe;
const FAT: u32 = 0xffff_fffd;

fn word(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

struct DirectoryReader<'a> {
    bytes: &'a [u8],
    sector_size: usize,
    fat_sectors: Vec<u32>,
    claimed: BTreeSet<u32>,
}

impl DirectoryReader<'_> {
    fn offset(&self, sector: u32) -> Option<usize> {
        let start = usize::try_from(sector)
            .ok()?
            .checked_add(1)?
            .checked_mul(self.sector_size)?;
        self.bytes
            .get(start..start.checked_add(self.sector_size)?)?;
        Some(start)
    }

    fn next(&self, sector: u32) -> Option<u32> {
        let index = usize::try_from(sector).ok()?;
        let words = self.sector_size / 4;
        word(
            self.bytes,
            self.offset(*self.fat_sectors.get(index / words)?)? + (index % words) * 4,
        )
    }

    fn chain(&mut self, mut sector: u32, limit: usize) -> Option<Vec<usize>> {
        let mut offsets = Vec::new();
        while sector != END {
            if offsets.len() >= limit || !self.claimed.insert(sector) {
                return None;
            }
            offsets.push(self.offset(sector)?);
            sector = self.next(sector)?;
        }
        Some(offsets)
    }
}

pub(crate) fn recover_unused_directory(
    bytes: &[u8],
    max_entries: usize,
) -> Option<(Vec<u8>, usize)> {
    if bytes.get(..8)? != b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1" || bytes.get(28..30)? != [0xfe, 0xff]
    {
        return None;
    }
    let sector_size = match (bytes.get(26..28)?, bytes.get(30..32)?) {
        ([3, 0], [9, 0]) => 512,
        ([4, 0], [12, 0]) => 4096,
        _ => return None,
    };
    // Recovery is deliberately limited to FAT tables described entirely by the header.
    if word(bytes, 72)? != 0 || !matches!(word(bytes, 68)?, FREE | END) {
        return None;
    }
    let fat_count = usize::try_from(word(bytes, 44)?).ok()?;
    if fat_count == 0 || fat_count > 109 {
        return None;
    }
    let mut reader = DirectoryReader {
        bytes,
        sector_size,
        fat_sectors: Vec::new(),
        claimed: BTreeSet::new(),
    };
    for index in 0..109 {
        let sector = word(bytes, 76 + index * 4)?;
        if index < fat_count {
            reader.offset(sector)?;
            if !reader.claimed.insert(sector) {
                return None;
            }
            reader.fat_sectors.push(sector);
        } else if sector != FREE {
            return None;
        }
    }
    for &sector in &reader.fat_sectors {
        if reader.next(sector)? != FAT {
            return None;
        }
    }
    let slots = sector_size / 128;
    let directory = reader.chain(word(bytes, 48)?, max_entries / slots + 1)?;
    if directory.is_empty() {
        return None;
    }
    let offsets: Vec<_> = directory
        .iter()
        .flat_map(|&start| (0..slots).map(move |index| start + index * 128))
        .collect();
    let mut reachable = BTreeSet::new();
    let mut pending = vec![0usize];
    while let Some(index) = pending.pop() {
        if reachable.len() >= max_entries || !reachable.insert(index) {
            return None;
        }
        let start = *offsets.get(index)?;
        let object_type = bytes[start + 66];
        if (index == 0 && object_type != 5) || (index != 0 && !matches!(object_type, 1 | 2)) {
            return None;
        }
        for field in [68, 72, 76] {
            let target = word(bytes, start + field)?;
            if target == FREE {
                continue;
            }
            if (index == 0 && field != 76) || (object_type == 2 && field == 76) {
                return None;
            }
            pending.push(usize::try_from(target).ok()?);
        }
    }
    let repairs: Vec<_> = offsets
        .iter()
        .enumerate()
        .filter_map(|(index, &start)| {
            (!reachable.contains(&index)
                && bytes[start..start + 128].iter().all(|&byte| byte == 0xff))
            .then_some(start)
        })
        .collect();
    if repairs.is_empty() {
        return None;
    }
    // Reject sector aliasing before changing any directory bytes.
    let sector_count = bytes.len() / sector_size;
    let mini_fat = reader.chain(word(bytes, 60)?, sector_count)?;
    if mini_fat.len() != usize::try_from(word(bytes, 64)?).ok()? {
        return None;
    }
    for index in reachable {
        let start = offsets[index];
        let object_type = bytes[start + 66];
        if object_type == 1 {
            continue;
        }
        let length = if sector_size == 512 {
            u64::from(word(bytes, start + 120)?)
        } else {
            u64::from_le_bytes(bytes.get(start + 120..start + 128)?.try_into().ok()?)
        };
        if length == 0 || (object_type == 2 && length < 4096) {
            continue;
        }
        let chain = reader.chain(word(bytes, start + 116)?, sector_count)?;
        if length > chain.len().checked_mul(sector_size)? as u64 {
            return None;
        }
    }
    let mut repaired = bytes.to_vec();
    for &start in &repairs {
        repaired[start..start + 128].fill(0);
        repaired[start + 68..start + 80].fill(0xff);
    }
    Some((repaired, repairs.len()))
}
