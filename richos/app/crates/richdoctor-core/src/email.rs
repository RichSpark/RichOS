//! **EMAIL SUPPORT AND THE SAVED PACKAGE** (PRD §7): the reviewed report text with every approved
//! screenshot as a real attachment, in the person's own mail app; or, without one that can take
//! attachments, a saved package to attach from any email account.
//!
//! Above 10 MiB of approved image bytes the package is split into numbered parts of at most 10
//! MiB each, every part carrying the same report text and report ID and every image appearing in
//! exactly one part, unchanged ([`packages`]). The count is computed from the real sizes: images
//! are indivisible, the text repeats in each part and each file carries a packaging allowance
//! (the V6 handover: never an assumed "three packages").
use serde::{Deserialize, Serialize};

/// A part's cap, and the size above which the email is offered as parts (PRD §7).
pub const PART_CAP: u64 = 10 << 20;
/// Bytes allowed per file for its name, its headers and the package's own structure.
pub const PER_FILE_ALLOWANCE: u64 = 4 << 10;

pub const SUPPORT: &str = crate::scrub::SUPPORT_ADDRESS;

/// One part: which images (indexes into the approved list) it carries, and its size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub number: u32,
    pub images: Vec<usize>,
    pub bytes: u64,
}

/// **SPLIT INTO PARTS** of at most [`PART_CAP`]: every image exactly once, in order, the text in
/// each part. One part when everything fits. `Err` only when one image with the text cannot fit
/// at all (it cannot: an approved image is at most 5 MiB), so no image is ever dropped.
pub fn packages(text_bytes: u64, images: &[u64]) -> Result<Vec<Part>, String> {
    let base = text_bytes + PER_FILE_ALLOWANCE;
    let mut parts: Vec<Part> = Vec::new();
    let mut current = Part { number: 1, images: Vec::new(), bytes: base };
    for (i, &size) in images.iter().enumerate() {
        let cost = size + PER_FILE_ALLOWANCE;
        if base + cost > PART_CAP {
            return Err(format!("screenshot {} is too large for one email part", i + 1));
        }
        if current.bytes + cost > PART_CAP && !current.images.is_empty() {
            let next = current.number + 1;
            parts.push(std::mem::replace(&mut current, Part { number: next, images: Vec::new(), bytes: base }));
        }
        current.images.push(i);
        current.bytes += cost;
    }
    parts.push(current);
    Ok(parts)
}

/// Whether the set is large enough to warn and offer parts (PRD §7: "For more than 10 MiB of
/// approved image bytes").
pub fn needs_parts(images: &[u64]) -> bool {
    images.iter().sum::<u64>() > PART_CAP
}

/// The short note at the top of each email or part.
pub fn cover(report_id: &str, part: Option<(u32, u32)>, images: usize) -> String {
    let which = match part {
        Some((n, of)) => format!(" This is part {n} of {of}; send each part as its own email to the same address."),
        None => String::new(),
    };
    format!(
        "Hello RichOS support,\n\nHere is my RichOS problem report, report ID {report_id}. It is the reviewed report text RichDoctor prepared{}.{which}\n",
        if images == 0 { String::new() } else if images == 1 { ", with 1 screenshot attached".into() } else { format!(", with {images} screenshots attached") }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const MIB: u64 = 1 << 20;

    #[test]
    fn everything_small_is_one_part() {
        let p = packages(20_000, &[MIB, 2 * MIB]).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].images, [0, 1]);
        assert!(!needs_parts(&[MIB, 2 * MIB]));
    }

    #[test]
    fn twenty_five_mib_of_indivisible_images_can_need_more_than_three_parts() {
        // Five 5 MiB images (25 MiB): two do not fit in one 10 MiB part beside the text, so each
        // part takes one. Five parts, not three.
        let images = [5 * MIB; 5];
        let p = packages(50_000, &images).unwrap();
        assert_eq!(p.len(), 5);
        let all: Vec<usize> = p.iter().flat_map(|x| x.images.clone()).collect();
        assert_eq!(all, [0, 1, 2, 3, 4], "every image exactly once, in order");
        assert!(p.iter().all(|x| x.bytes <= PART_CAP));
        assert!(needs_parts(&images));
        // Smaller images pack together: three 3 MiB images and the text fit under 10 MiB.
        let p = packages(50_000, &[3 * MIB; 6]).unwrap();
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|x| x.bytes <= PART_CAP && x.images.len() == 3));
    }

    #[test]
    fn the_cover_names_the_report_and_the_part() {
        assert!(cover("RD-7K3M-Q9VX", Some((2, 3)), 1).contains("part 2 of 3"));
        assert!(cover("RD-7K3M-Q9VX", None, 2).contains("2 screenshots attached"));
    }
}
