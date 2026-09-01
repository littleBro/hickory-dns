//! Model-based tests for [`Name`].
//!
//! A `Model` is the plain representation of a domain name, a list of labels plus the FQDN flag,
//! and the `ref_*` functions spell out the semantics `Name` must implement: RFC 4034 canonical
//! ordering (labels compared from the root, case-insensitively unless asked otherwise), the
//! `zone_of` suffix relation, and the RFC 1035 wire decoding rules with the restrictions this
//! crate enforces. Every public operation on `Name` is compared against the model on randomly
//! generated names, so the tests hold for any internal representation.
//!
//! The generators are seeded, so runs are reproducible. Set `HICKORY_NAME_PROPS_SCALE=n` to run
//! `n` times as many cases.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, VecDeque};
use std::hash::{Hash, Hasher};

use hickory_proto::ProtoError;
use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::rdata::{A, CNAME};
use hickory_proto::rr::{LowerName, Name, RecordData, RecordType};
use hickory_proto::serialize::binary::{
    BinDecodable, BinDecoder, BinEncodable, BinEncoder, DecodeError,
};

/// Number of iterations for a test, scaled by `HICKORY_NAME_PROPS_SCALE`.
fn iterations(base: usize) -> usize {
    let scale = std::env::var("HICKORY_NAME_PROPS_SCALE")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    base * scale.max(1)
}

/// xorshift64, enough to drive the generators deterministically without a dependency.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        assert!(n > 0);
        (self.next_u64() % n as u64) as usize
    }

    fn chance(&mut self, num: usize, den: usize) -> bool {
        self.below(den) < num
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Model {
    labels: Vec<Vec<u8>>,
    fqdn: bool,
}

impl Model {
    /// Length of the wire form, root byte included.
    fn encoded_len(&self) -> usize {
        self.labels.iter().map(|l| l.len() + 1).sum::<usize>() + 1
    }

    fn to_name(&self) -> Name {
        let mut name = Name::from_labels(self.labels.iter().map(|l| l.as_slice()))
            .unwrap_or_else(|e| panic!("model should be valid: {e} {self:?}"));
        name.set_fqdn(self.fqdn);
        name
    }

    fn label_refs(&self) -> Vec<&[u8]> {
        self.labels.iter().map(|l| l.as_slice()).collect()
    }
}

const SAFE: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
const SMALL: &[u8] = b"abAB";

/// `Label::from_ascii` rejects a leading hyphen, so keep the first byte alphanumeric.
fn fix_first(mut label: Vec<u8>) -> Vec<u8> {
    if label.first() == Some(&b'-') {
        label[0] = b'x';
    }
    label
}

fn gen_label(rng: &mut Rng, max_len: usize) -> Vec<u8> {
    let len = match rng.below(10) {
        0 => max_len,
        1..=6 => 1 + rng.below(4.min(max_len)),
        _ => 1 + rng.below(max_len),
    };
    let alphabet = if rng.chance(1, 2) { SMALL } else { SAFE };
    fix_first(
        (0..len)
            .map(|_| alphabet[rng.below(alphabet.len())])
            .collect(),
    )
}

fn gen_fixed(rng: &mut Rng, len: usize) -> Vec<u8> {
    fix_first((0..len).map(|_| SAFE[rng.below(SAFE.len())]).collect())
}

/// A random valid name: mostly short, sometimes at the 127-label or 255-byte limit.
fn gen_model(rng: &mut Rng) -> Model {
    loop {
        let count = match rng.below(20) {
            0 => 0,
            1 => 127,
            2 => 4,
            3..=10 => 1 + rng.below(3),
            _ => 1 + rng.below(8),
        };
        let labels: Vec<Vec<u8>> = match count {
            127 => (0..127).map(|_| vec![SMALL[rng.below(4)]]).collect(),
            4 if rng.chance(1, 2) => vec![
                gen_fixed(rng, 63),
                gen_fixed(rng, 63),
                gen_fixed(rng, 63),
                gen_fixed(rng, 61),
            ],
            _ => (0..count).map(|_| gen_label(rng, 63)).collect(),
        };
        let model = Model {
            labels,
            fqdn: rng.chance(3, 4),
        };
        if model.encoded_len() <= 255 {
            return model;
        }
    }
}

/// A second name related to the first: equal, a case variant, a prefix or suffix, a wildcard...
fn variant(rng: &mut Rng, a: &Model) -> Model {
    let mut b = a.clone();
    match rng.below(10) {
        0 => {}
        1 => {
            for label in &mut b.labels {
                for c in label.iter_mut() {
                    if c.is_ascii_alphabetic() && rng.chance(1, 2) {
                        *c ^= 0x20;
                    }
                }
            }
        }
        2 => {
            if !b.labels.is_empty() {
                let i = rng.below(b.labels.len());
                b.labels[i] = gen_label(rng, 63);
            }
        }
        3 => b.labels.insert(0, gen_label(rng, 63)),
        4 => {
            let k = rng.below(b.labels.len() + 1);
            b.labels.drain(..k);
        }
        5 => b.fqdn = !b.fqdn,
        6 => {
            if let Some(label) = b.labels.last_mut() {
                if label.len() > 1 && (rng.chance(1, 2) || label.len() >= 63) {
                    label.pop();
                } else {
                    label.push(b'a');
                }
            }
        }
        7 => {
            if !b.labels.is_empty() {
                let i = rng.below(b.labels.len());
                if b.labels[i].len() > 1 {
                    b.labels[i].pop();
                } else {
                    b.labels[i].push(b'z');
                }
            }
        }
        8 => {
            if !b.labels.is_empty() {
                b.labels[0] = b"*".to_vec();
            }
        }
        _ => return gen_model(rng),
    }
    if b.encoded_len() > 255 {
        return a.clone();
    }
    b
}

fn fold(c: u8, case_insensitive: bool) -> u8 {
    if case_insensitive {
        c.to_ascii_lowercase()
    } else {
        c
    }
}

/// RFC 4034 section 6.1 canonical ordering, with the FQDN flag as the primary key as `Name`
/// documents it: a relative name sorts before an absolute one.
fn ref_cmp(a: &Model, b: &Model, case_insensitive: bool) -> Ordering {
    match (a.fqdn, b.fqdn) {
        (false, true) => return Ordering::Less,
        (true, false) => return Ordering::Greater,
        _ => {}
    }
    for (l, r) in a.labels.iter().rev().zip(b.labels.iter().rev()) {
        for (&x, &y) in l.iter().zip(r.iter()) {
            match fold(x, case_insensitive).cmp(&fold(y, case_insensitive)) {
                Ordering::Equal => {}
                o => return o,
            }
        }
        match l.len().cmp(&r.len()) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    a.labels.len().cmp(&b.labels.len())
}

/// `a.zone_of(b)`: the root is the zone of everything, nothing is a zone of the root, and
/// otherwise `a`'s labels must be a suffix of `b`'s labels.
fn ref_zone_of(a: &Model, b: &Model, case_insensitive: bool) -> bool {
    let (a_len, b_len) = (a.labels.len(), b.labels.len());
    if a_len == 0 {
        return true;
    }
    if b_len == 0 || a_len > b_len {
        return false;
    }
    a.labels
        .iter()
        .rev()
        .zip(b.labels.iter().rev())
        .all(|(x, y)| {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(p, q)| fold(*p, case_insensitive) == fold(*q, case_insensitive))
        })
}

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    t.hash(&mut hasher);
    hasher.finish()
}

fn wire(name: &Name, lowercase: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut encoder = BinEncoder::new(&mut buf);
    let name = if lowercase {
        name.to_lowercase()
    } else {
        name.clone()
    };
    name.emit(&mut encoder).expect("emit");
    buf
}

/// Decodes a name starting at `off`, returning the result and the decoder position afterwards.
fn read_at(buf: &[u8], off: usize) -> (Result<Name, ProtoError>, usize) {
    let mut decoder = BinDecoder::new(buf);
    if off > 0 {
        decoder.read_slice(off).expect("offset within buffer");
    }
    let result = Name::read(&mut decoder).map_err(Into::into);
    (result, decoder.index())
}

#[test]
fn name_semantics_match_reference_model() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let iterations = iterations(4_000);
    let mut equal_pairs = 0usize;
    for _ in 0..iterations {
        let am = gen_model(&mut rng);
        let bm = variant(&mut rng, &am);
        let a = am.to_name();
        let b = bm.to_name();

        // shape
        assert_eq!(a.is_fqdn(), am.fqdn);
        assert_eq!(a.iter().collect::<Vec<_>>(), am.label_refs());
        let mut reversed = am.label_refs();
        reversed.reverse();
        assert_eq!(a.iter().rev().collect::<Vec<_>>(), reversed);
        assert_eq!(a.iter().len(), am.labels.len());
        assert_eq!(a.is_root(), am.labels.is_empty() && am.fqdn);
        let expected_len = if am.labels.is_empty() {
            1
        } else {
            am.labels.iter().map(|l| l.len()).sum::<usize>() + am.labels.len()
        };
        assert_eq!(a.len(), expected_len);
        let wildcard = am.labels.first().is_some_and(|l| l.as_slice() == b"*");
        assert_eq!(
            usize::from(a.num_labels()),
            am.labels.len() - usize::from(wildcard)
        );

        // mixed forward and backward iteration against a deque
        let mut iter = a.iter();
        let mut deque: VecDeque<&[u8]> = am.label_refs().into_iter().collect();
        loop {
            assert_eq!(iter.len(), deque.len());
            if rng.chance(1, 2) {
                assert_eq!(iter.next(), deque.pop_front());
            } else {
                assert_eq!(iter.next_back(), deque.pop_back());
            }
            if deque.is_empty() {
                assert_eq!(iter.next(), None);
                assert_eq!(iter.next_back(), None);
                break;
            }
        }

        // ordering and equality
        assert_eq!(a.cmp(&b), ref_cmp(&am, &bm, true), "cmp {a:?} vs {b:?}");
        assert_eq!(
            a.cmp_case(&b),
            ref_cmp(&am, &bm, false),
            "cmp_case {a:?} vs {b:?}"
        );
        assert_eq!(
            a == b,
            ref_cmp(&am, &bm, true) == Ordering::Equal,
            "eq {a:?} vs {b:?}"
        );
        assert_eq!(a.eq_case(&b), ref_cmp(&am, &bm, false) == Ordering::Equal);
        assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
        assert_eq!(a == b, a.cmp(&b) == Ordering::Equal);
        assert_eq!(
            a.zone_of(&b),
            ref_zone_of(&am, &bm, true),
            "zone_of {a:?} {b:?}"
        );
        assert_eq!(a.zone_of_case(&b), ref_zone_of(&am, &bm, false));
        assert_eq!(b.zone_of(&a), ref_zone_of(&bm, &am, true));
        if a == b {
            equal_pairs += 1;
            assert_eq!(hash_of(&a), hash_of(&b), "hash {a:?} vs {b:?}");
            assert_eq!(
                hash_of(&LowerName::from(a.clone())),
                hash_of(&LowerName::from(b.clone()))
            );
        }
        assert_eq!(
            LowerName::from(a.clone()).cmp(&LowerName::from(b.clone())),
            a.cmp(&b)
        );

        // lowercase
        let lower = a.to_lowercase();
        let lower_model = Model {
            labels: am.labels.iter().map(|l| l.to_ascii_lowercase()).collect(),
            fqdn: am.fqdn,
        };
        assert!(lower.eq_case(&lower_model.to_name()));
        assert_eq!(lower, a);
        assert_eq!(hash_of(&lower), hash_of(&a));

        // base_name, trim_to, into_wildcard
        let base = a.base_name();
        assert_eq!(
            base.iter().collect::<Vec<_>>(),
            am.labels
                .iter()
                .skip(1)
                .map(|l| l.as_slice())
                .collect::<Vec<_>>()
        );
        let k = rng.below(am.labels.len() + 2);
        let trimmed = a.trim_to(k);
        let expected_trim: Vec<&[u8]> = if k > am.labels.len() {
            am.label_refs()
        } else {
            am.labels[am.labels.len() - k..]
                .iter()
                .map(|l| l.as_slice())
                .collect()
        };
        assert_eq!(trimmed.iter().collect::<Vec<_>>(), expected_trim);
        let wild = a.clone().into_wildcard();
        if am.labels.is_empty() {
            assert!(wild.is_root());
        } else {
            let mut expected_wild = am.label_refs();
            expected_wild[0] = b"*";
            assert_eq!(wild.iter().collect::<Vec<_>>(), expected_wild);
            assert_eq!(wild.is_fqdn(), am.fqdn);
        }

        // text round trip
        let text = a.to_ascii();
        let back = Name::from_ascii(&text).unwrap_or_else(|e| panic!("from_ascii({text:?}): {e}"));
        assert!(back.eq_case(&a), "text round trip {text:?}");
        assert_eq!(back.is_fqdn(), a.is_fqdn());
        // the utf8 path runs IDNA/UTS46, which lowercases and rejects STD3-invalid `_`
        if !am
            .labels
            .iter()
            .any(|l| l.contains(&b'_') || l.windows(2).any(|w| w == b"--"))
        {
            let back_utf8 =
                Name::from_utf8(&text).unwrap_or_else(|e| panic!("from_utf8({text:?}): {e}"));
            assert_eq!(back_utf8, a, "utf8 round trip {text:?}");
        }

        // wire round trip
        let bytes = wire(&a, false);
        assert_eq!(bytes.len(), am.encoded_len());
        let (read, index) = read_at(&bytes, 0);
        let read = read.unwrap();
        assert_eq!(index, bytes.len());
        assert!(read.is_fqdn());
        assert_eq!(read.iter().collect::<Vec<_>>(), am.label_refs());
        let lower_bytes = wire(&a, true);
        let (read_lower, _) = read_at(&lower_bytes, 0);
        let read_lower = read_lower.unwrap();
        assert_eq!(read_lower, read);
        assert!(read_lower.eq_case(&lower_model.to_name()) || !am.fqdn);
    }
    assert!(
        equal_pairs > iterations / 20,
        "generator produced too few equal pairs: {equal_pairs}"
    );
}

#[test]
fn text_parsing_escapes_and_limits() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.";
    for _ in 0..iterations(4_000) {
        let count = 1 + rng.below(4);
        let labels: Vec<Vec<u8>> = (0..count)
            .map(|_| {
                let len = 1 + rng.below(20);
                fix_first(
                    (0..len)
                        .map(|_| ALPHABET[rng.below(ALPHABET.len())])
                        .collect(),
                )
            })
            .collect();
        let model = Model { labels, fqdn: true };
        let name = model.to_name();
        let text = name.to_ascii();
        let back = Name::from_ascii(&text).unwrap_or_else(|e| panic!("from_ascii({text:?}): {e}"));
        assert!(
            back.eq_case(&name),
            "escape round trip {text:?}: {back:?} vs {name:?}"
        );
    }

    // label length boundaries through both entry points
    for len in [1usize, 62, 63, 64, 65, 100, 300] {
        let label = "a".repeat(len);
        assert_eq!(
            Name::from_ascii(format!("{label}.example.")).is_ok(),
            len <= 63,
            "label length {len}"
        );
        assert_eq!(
            Name::from_utf8(format!("{label}.example.")).is_ok(),
            len <= 63,
            "label length {len} utf8"
        );
    }
    // multi-byte characters through the utf8 path, plus escapes
    let name = Name::from_utf8("bücher.ünicode.example.").unwrap();
    assert_eq!(name.num_labels(), 3);
    let name = Name::from_ascii("a\\.b.c-d.\\065\\066.").unwrap();
    assert_eq!(
        name.iter().collect::<Vec<_>>(),
        vec![&b"a.b"[..], b"c-d", b"56"]
    );
    // total length limit through the text path: 127 one-byte labels fit, 128 do not
    let too_long = (0..128).map(|_| "a").collect::<Vec<_>>().join(".");
    assert!(Name::from_ascii(format!("{too_long}.")).is_err());
    let longest = (0..127).map(|_| "a").collect::<Vec<_>>().join(".");
    assert_eq!(
        Name::from_ascii(format!("{longest}."))
            .unwrap()
            .num_labels(),
        127
    );
}

fn gen_small_fqdn(rng: &mut Rng) -> Model {
    let count = 1 + rng.below(6);
    Model {
        labels: (0..count).map(|_| gen_label(rng, 12)).collect(),
        fqdn: true,
    }
}

#[test]
fn compression_pointers_decode_expected_names() {
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    for _ in 0..iterations(8_000) {
        // name1 at off1, then name2 at off2: a few labels followed by a pointer into name1
        let m1 = gen_small_fqdn(&mut rng);
        let mut buf: Vec<u8> = (0..rng.below(6)).map(|_| rng.next_u64() as u8).collect();
        let off1 = buf.len();
        let mut label_offsets = Vec::new();
        for label in &m1.labels {
            label_offsets.push(buf.len());
            buf.push(label.len() as u8);
            buf.extend_from_slice(label);
        }
        label_offsets.push(buf.len());
        buf.push(0);
        let end1 = buf.len();
        for _ in 0..rng.below(4) {
            buf.push(rng.next_u64() as u8);
        }
        let off2 = buf.len();
        let prefix: Vec<Vec<u8>> = (0..rng.below(4)).map(|_| gen_label(&mut rng, 12)).collect();
        for label in &prefix {
            buf.push(label.len() as u8);
            buf.extend_from_slice(label);
        }
        let i = rng.below(label_offsets.len());
        let ptr = label_offsets[i];
        buf.push(0xC0 | (ptr >> 8) as u8);
        buf.push(ptr as u8);
        let end2 = buf.len();
        for _ in 0..rng.below(4) {
            buf.push(rng.next_u64() as u8);
        }

        let expected: Vec<&[u8]> = prefix
            .iter()
            .map(|l| l.as_slice())
            .chain(m1.labels[i..].iter().map(|l| l.as_slice()))
            .collect();

        let (n1, index1) = read_at(&buf, off1);
        let n1 = n1.unwrap();
        assert_eq!(index1, end1);
        assert_eq!(n1.iter().collect::<Vec<_>>(), m1.label_refs());

        let (n2, index2) = read_at(&buf, off2);
        let n2 = n2.unwrap_or_else(|e| panic!("{e} buf={buf:?} off2={off2} ptr={ptr}"));
        assert_eq!(index2, end2);
        assert!(n2.is_fqdn());
        assert_eq!(
            n2.iter().collect::<Vec<_>>(),
            expected,
            "buf={buf:?} off2={off2} ptr={ptr}"
        );

        // forward and self pointers must be rejected, never panic
        let mut forward = buf[..end2 - 2].to_vec();
        let target = end2 + 2;
        forward.push(0xC0 | (target >> 8) as u8);
        forward.push(target as u8);
        forward.extend_from_slice(&[1, b'x', 0, 0, 0, 0]);
        assert!(
            read_at(&forward, off2).0.is_err(),
            "forward pointer accepted: {forward:?}"
        );
        let mut selfref = buf[..end2 - 2].to_vec();
        let here = selfref.len();
        selfref.push(0xC0 | (here >> 8) as u8);
        selfref.push(here as u8);
        assert!(
            read_at(&selfref, off2).0.is_err(),
            "self pointer accepted: {selfref:?}"
        );
    }
}

#[test]
fn message_round_trip_with_compression() {
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for _ in 0..iterations(1_000) {
        let base = gen_small_fqdn(&mut rng);
        let mut message = Message::new(rng.next_u64() as u16, MessageType::Response, OpCode::Query);
        message.add_query(Query::new(base.to_name(), RecordType::A));
        let mut expected: Vec<(Name, Option<Name>)> = Vec::new();
        for _ in 0..1 + rng.below(12) {
            // owners and targets share suffixes with the query name, so the encoder emits pointers
            let mut owner = base.clone();
            owner.labels.drain(..rng.below(owner.labels.len() + 1));
            for _ in 0..rng.below(3) {
                owner.labels.insert(0, gen_label(&mut rng, 12));
            }
            let mut target = base.clone();
            target.labels.drain(..rng.below(target.labels.len() + 1));
            for _ in 0..rng.below(3) {
                target.labels.insert(0, gen_label(&mut rng, 12));
            }
            let owner_name = owner.to_name();
            if rng.chance(1, 2) {
                message.add_answer(hickory_proto::rr::Record::from_rdata(
                    owner_name.clone(),
                    60,
                    A::new(10, 0, 0, rng.below(256) as u8).into_rdata(),
                ));
                expected.push((owner_name, None));
            } else {
                let target_name = target.to_name();
                message.add_answer(hickory_proto::rr::Record::from_rdata(
                    owner_name.clone(),
                    60,
                    CNAME(target_name.clone()).into_rdata(),
                ));
                expected.push((owner_name, Some(target_name)));
            }
        }
        let bytes = message.to_vec().expect("encode");
        let decoded = Message::from_vec(&bytes).expect("decode");
        assert_eq!(decoded.answers.len(), expected.len());
        for (record, (owner, target)) in decoded.answers.iter().zip(&expected) {
            assert_eq!(&record.name, owner);
            assert!(record.name.eq_case(owner));
            if let Some(target) = target {
                let cname = CNAME::try_borrow(&record.data).expect("cname");
                let target_name: &Name = cname;
                assert_eq!(target_name, target);
                assert!(target_name.eq_case(target));
            }
        }
        // re-encoding a decoded message reproduces the bytes exactly
        assert_eq!(decoded.to_vec().expect("re-encode"), bytes);
        // reading names at arbitrary offsets inside a real message never panics
        for _ in 0..8 {
            let off = rng.below(bytes.len());
            let _ = read_at(&bytes, off);
        }
    }
}

#[test]
fn decoder_never_panics_on_garbage() {
    let mut rng = Rng(0xF1E2_D3C4_B5A6_9788);
    for _ in 0..iterations(60_000) {
        let len = match rng.below(4) {
            0 => rng.below(8),
            1 => rng.below(32),
            2 => rng.below(128),
            _ => rng.below(600),
        };
        // biased toward bytes that mean something to the decoder
        let buf: Vec<u8> = (0..len)
            .map(|_| match rng.below(10) {
                0..=3 => rng.below(9) as u8,
                4..=5 => 0xC0 | rng.below(4) as u8,
                6 => [0x40u8, 0x80][rng.below(2)],
                7 => 63,
                _ => rng.next_u64() as u8,
            })
            .collect();
        let off = if buf.is_empty() {
            0
        } else {
            rng.below(buf.len())
        };
        let (result, index) = read_at(&buf, off);
        assert!(index <= buf.len());
        if let Ok(name) = result {
            let bytes = wire(&name, false);
            assert!(
                bytes.len() <= 255,
                "decoded name re-encodes to {} bytes",
                bytes.len()
            );
            let (back, _) = read_at(&bytes, 0);
            assert!(back.unwrap().eq_case(&name));
        }
        if buf.len() >= 12 {
            let _ = Message::from_vec(&buf);
        }
    }
}

/// The RFC 1035 name decoding rules with this crate's restrictions, written as the plain state
/// machine on top of the public decoder API: labels of at most 63 bytes, pointers only to
/// earlier offsets, no label may start inside a region already covered by a pointer, 255 bytes
/// at most. Returns the labels and the *outer* decoder position on success, or the `Debug` form
/// of the [`DecodeError`] the decoder must produce.
fn ref_read(buf: &[u8], off: usize) -> Result<(Vec<Vec<u8>>, usize), String> {
    enum State {
        LengthOrPointer,
        Label,
        Pointer,
        Root,
    }
    let mut outer = BinDecoder::new(buf);
    if off > 0 {
        outer.read_slice(off).expect("offset within buffer");
    }
    let mut chased: Option<BinDecoder<'_>> = None;
    let mut labels: Vec<Vec<u8>> = Vec::new();
    let mut encoded_len = 1usize;
    let mut ptr_max_idx: Option<usize> = None;
    let mut name_start = off;
    let mut state = State::LengthOrPointer;
    loop {
        let decoder = match chased.as_mut() {
            Some(decoder) => decoder,
            None => &mut outer,
        };
        if let Some(max_idx) = ptr_max_idx {
            if decoder.index() >= max_idx {
                return Err(format!(
                    "{:?}",
                    DecodeError::LabelOverlapsWithOther {
                        label: name_start,
                        other: max_idx,
                    }
                ));
            }
        }
        state = match state {
            State::LengthOrPointer => match decoder.peek().map(|r| r.unverified()) {
                Some(0) => State::Root,
                None => return Err(format!("{:?}", DecodeError::InsufficientBytes)),
                Some(b) if b & 0b1100_0000 == 0b1100_0000 => State::Pointer,
                Some(b) if b & 0b1100_0000 == 0 => State::Label,
                Some(b) => {
                    return Err(format!("{:?}", DecodeError::UnrecognizedLabelCode(b)));
                }
            },
            State::Label => {
                let label = decoder
                    .read_character_data()
                    .map_err(|e| format!("{e:?}"))?
                    .unverified();
                if label.len() > 63 {
                    return Err(format!("{:?}", DecodeError::LabelBytesTooLong(label.len())));
                }
                let new_len = encoded_len + label.len() + 1;
                if new_len > 255 {
                    return Err(format!("{:?}", DecodeError::DomainNameTooLong(label.len())));
                }
                encoded_len = new_len;
                labels.push(label.to_vec());
                State::LengthOrPointer
            }
            State::Pointer => {
                let pointer_location = decoder.index();
                let raw = decoder
                    .read_u16()
                    .map_err(|e| format!("{e:?}"))?
                    .unverified()
                    & 0x3FFF;
                if usize::from(raw) >= name_start {
                    return Err(format!(
                        "{:?}",
                        DecodeError::PointerNotPriorToLabel {
                            idx: pointer_location,
                            ptr: raw,
                        }
                    ));
                }
                ptr_max_idx = Some(name_start);
                let next = decoder.clone(raw);
                name_start = next.index();
                chased = Some(next);
                State::LengthOrPointer
            }
            State::Root => {
                decoder.pop().map_err(|e| format!("{e:?}"))?;
                break;
            }
        };
    }
    let len = if labels.is_empty() {
        1
    } else {
        labels.iter().map(|l| l.len()).sum::<usize>() + labels.len()
    };
    if len >= 255 {
        return Err(format!("{:?}", DecodeError::DomainNameTooLong(len)));
    }
    Ok((labels, outer.index()))
}

#[test]
fn decoder_matches_reference_state_machine() {
    let mut rng = Rng(0x1357_9BDF_2468_ACE0);
    let iterations = iterations(50_000);
    let mut ok = 0usize;
    let mut errors = BTreeMap::<String, usize>::new();
    for _ in 0..iterations {
        // a buffer assembled from fragments: labels, root bytes, pointers backward (often to a
        // label start), forward or to itself, reserved label codes, truncated labels, noise
        let mut buf: Vec<u8> = Vec::new();
        let mut label_starts: Vec<usize> = Vec::new();
        for _ in 0..rng.below(14) {
            match rng.below(13) {
                0..=5 => {
                    let len = match rng.below(6) {
                        0 => 63,
                        1 => 1 + rng.below(63),
                        _ => 1 + rng.below(6),
                    };
                    label_starts.push(buf.len());
                    buf.push(len as u8);
                    for _ in 0..len {
                        buf.push(SAFE[rng.below(SAFE.len())]);
                    }
                }
                6 => buf.push(0),
                7 => {
                    let target = if !label_starts.is_empty() && rng.chance(2, 3) {
                        label_starts[rng.below(label_starts.len())]
                    } else {
                        rng.below(buf.len() + 1)
                    };
                    buf.push(0xC0 | (target >> 8) as u8);
                    buf.push(target as u8);
                }
                8 => {
                    let target = buf.len() + rng.below(24);
                    buf.push(0xC0 | (target >> 8) as u8);
                    buf.push(target as u8);
                }
                9 => buf.push([0x40u8, 0x80, 0x7F, 0xBF][rng.below(4)] | rng.below(8) as u8),
                10 => {
                    let len = 1 + rng.below(63);
                    buf.push(len as u8);
                    let present = rng.below(len);
                    buf.resize(buf.len() + present, b'q');
                }
                11 => buf.push(0xC0 | rng.below(64) as u8),
                _ => {
                    for _ in 0..1 + rng.below(4) {
                        buf.push(rng.next_u64() as u8);
                    }
                }
            }
        }
        if rng.chance(1, 5) && !buf.is_empty() {
            buf.truncate(rng.below(buf.len()));
        }
        let mut offsets = vec![0usize];
        offsets.extend(label_starts.iter().copied().filter(|&o| o < buf.len()));
        if !buf.is_empty() {
            offsets.push(rng.below(buf.len()));
        }
        for off in offsets {
            let expected = ref_read(&buf, off);
            let mut decoder = BinDecoder::new(&buf);
            if off > 0 {
                decoder.read_slice(off).expect("offset within buffer");
            }
            let actual = Name::read(&mut decoder).map_err(|e| format!("{e:?}"));
            match (expected, actual) {
                (Ok((labels, index)), Ok(name)) => {
                    ok += 1;
                    assert!(name.is_fqdn());
                    assert_eq!(
                        name.iter().map(|l| l.to_vec()).collect::<Vec<_>>(),
                        labels,
                        "labels differ: buf={buf:?} off={off}"
                    );
                    assert_eq!(
                        decoder.index(),
                        index,
                        "decoder position differs: buf={buf:?} off={off}"
                    );
                }
                (Err(expected), Err(actual)) => {
                    assert_eq!(expected, actual, "error differs: buf={buf:?} off={off}");
                    let variant = expected
                        .split(['(', ' ', '{'])
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    *errors.entry(variant).or_default() += 1;
                }
                (expected, actual) => {
                    panic!(
                        "outcome differs: ref={expected:?} actual={actual:?} buf={buf:?} off={off}"
                    )
                }
            }
        }
    }
    assert!(ok > iterations / 10, "too few successful decodes: {ok}");
    assert!(
        errors.len() >= 4,
        "generator did not reach enough error classes: {errors:?}"
    );
}
