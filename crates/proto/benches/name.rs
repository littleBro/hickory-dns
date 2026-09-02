#![cfg(nightly)]
#![feature(test)]

extern crate test;

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use test::Bencher;

use hickory_proto::rr::*;

/// A 34-label name, the shape of every IPv6 reverse-zone owner name.
const IP6_ARPA: &str = "b.a.9.8.7.6.5.4.3.2.1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.ip6.arpa.";

#[bench]
fn name_cmp_short(b: &mut Bencher) {
    let name1 = Name::from_ascii("com").unwrap();
    let name2 = Name::from_ascii("COM").unwrap();

    b.iter(|| {
        assert_eq!(name1.cmp(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_short_not_eq(b: &mut Bencher) {
    let name1 = Name::from_ascii("com").unwrap();
    let name2 = Name::from_ascii("COM").unwrap();

    b.iter(|| {
        assert_ne!(name1.cmp_case(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_short_case(b: &mut Bencher) {
    let name1 = Name::from_ascii("com").unwrap();
    let name2 = Name::from_ascii("com").unwrap();

    b.iter(|| {
        assert_eq!(name1.cmp_case(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_medium(b: &mut Bencher) {
    let name1 = Name::from_ascii("www.example.com").unwrap();
    let name2 = Name::from_ascii("www.EXAMPLE.com").unwrap();

    b.iter(|| {
        assert_eq!(name1.cmp(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_medium_not_eq(b: &mut Bencher) {
    let name1 = Name::from_ascii("www.example.com").unwrap();
    let name2 = Name::from_ascii("www.EXAMPLE.com").unwrap();

    b.iter(|| {
        assert_ne!(name1.cmp_case(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_medium_case(b: &mut Bencher) {
    let name1 = Name::from_ascii("www.example.com").unwrap();
    let name2 = Name::from_ascii("www.example.com").unwrap();

    b.iter(|| {
        assert_eq!(name1.cmp_case(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_long(b: &mut Bencher) {
    let name1 = Name::from_ascii("a.crazy.really.long.example.com").unwrap();
    let name2 = Name::from_ascii("a.crazy.really.long.EXAMPLE.com").unwrap();

    b.iter(|| {
        assert_eq!(name1.cmp(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_long_not_eq(b: &mut Bencher) {
    let name1 = Name::from_ascii("a.crazy.really.long.example.com").unwrap();
    let name2 = Name::from_ascii("a.crazy.really.long.EXAMPLE.com").unwrap();

    b.iter(|| {
        assert_ne!(name1.cmp_case(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_long_case(b: &mut Bencher) {
    let name1 = Name::from_ascii("a.crazy.really.long.example.com").unwrap();
    let name2 = Name::from_ascii("a.crazy.really.long.example.com").unwrap();

    b.iter(|| {
        assert_eq!(name1.cmp_case(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_to_lower_short(b: &mut Bencher) {
    let name1 = Name::from_ascii("COM").unwrap();

    b.iter(|| {
        let lower = name1.to_lowercase();
        assert_eq!(lower.num_labels(), 1);
    });
}

#[bench]
fn name_to_lower_medium(b: &mut Bencher) {
    let name1 = Name::from_ascii("example.COM").unwrap();

    b.iter(|| {
        let lower = name1.to_lowercase();
        assert_eq!(lower.num_labels(), 2);
    });
}

#[bench]
fn name_to_lower_long(b: &mut Bencher) {
    let name1 = Name::from_ascii("www.EXAMPLE.com").unwrap();

    b.iter(|| {
        let lower = name1.to_lowercase();
        assert_eq!(lower.num_labels(), 3);
    });
}

#[bench]
fn name_no_lower_short(b: &mut Bencher) {
    let name1 = Name::from_ascii("com").unwrap();

    b.iter(|| {
        let lower = name1.to_lowercase();
        assert_eq!(lower.num_labels(), 1);
    });
}

#[bench]
fn name_no_lower_medium(b: &mut Bencher) {
    let name1 = Name::from_ascii("example.com").unwrap();

    b.iter(|| {
        let lower = name1.to_lowercase();
        assert_eq!(lower.num_labels(), 2);
    });
}

#[bench]
fn name_no_lower_long(b: &mut Bencher) {
    let name1 = Name::from_ascii("www.example.com").unwrap();

    b.iter(|| {
        let lower = name1.to_lowercase();
        assert_eq!(lower.num_labels(), 3);
    });
}

#[bench]
fn name_cmp_long_not_eq_root(b: &mut Bencher) {
    // differs in the root-most label, so the comparison can stop after one label
    let name1 = Name::from_ascii("a.crazy.really.long.example.com").unwrap();
    let name2 = Name::from_ascii("a.crazy.really.long.example.net").unwrap();

    b.iter(|| {
        assert_ne!(name1.cmp(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_ip6_arpa(b: &mut Bencher) {
    let name1 = Name::from_ascii(IP6_ARPA).unwrap();
    let name2 = Name::from_ascii(IP6_ARPA.to_ascii_uppercase()).unwrap();

    b.iter(|| {
        assert_eq!(name1.cmp(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_cmp_ip6_arpa_not_eq_root(b: &mut Bencher) {
    let name1 = Name::from_ascii(IP6_ARPA).unwrap();
    let name2 = Name::from_ascii(IP6_ARPA.replace("arpa", "arpb")).unwrap();

    b.iter(|| {
        assert_ne!(name1.cmp(&name2), Ordering::Equal);
    });
}

#[bench]
fn name_eq_medium(b: &mut Bencher) {
    let name1 = Name::from_ascii("www.example.com").unwrap();
    let name2 = Name::from_ascii("www.EXAMPLE.com").unwrap();

    b.iter(|| {
        assert!(name1 == name2);
    });
}

#[bench]
fn name_hash_medium(b: &mut Bencher) {
    let name = Name::from_ascii("www.example.com").unwrap();

    b.iter(|| {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        hasher.finish()
    });
}

#[bench]
fn name_iter_rev_medium(b: &mut Bencher) {
    let name = Name::from_ascii("www.example.com").unwrap();

    b.iter(|| {
        assert_eq!(name.iter().rev().count(), 3);
    });
}

#[bench]
fn name_iter_rev_ip6_arpa(b: &mut Bencher) {
    let name = Name::from_ascii(IP6_ARPA).unwrap();

    b.iter(|| {
        assert_eq!(name.iter().rev().count(), 34);
    });
}

#[bench]
fn name_iter_rev_max_labels(b: &mut Bencher) {
    // 127 one-byte labels, the most a 255-byte name can hold
    let text = (0..127).map(|_| "a").collect::<Vec<_>>().join(".");
    let name = Name::from_ascii(format!("{text}.")).unwrap();

    b.iter(|| {
        assert_eq!(name.iter().rev().count(), 127);
    });
}

#[bench]
fn name_parse_arpa_name_ip6(b: &mut Bencher) {
    let name = Name::from_ascii(IP6_ARPA).unwrap();

    b.iter(|| {
        name.parse_arpa_name().unwrap();
    });
}
