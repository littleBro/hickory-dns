//! Independent property checks for `Name`, written during review of the
//! memory-footprint series. A reference model (Vec of labels + fqdn flag)
//! encodes the semantics of `main`; every public operation is compared
//! against it. Run on both `main` and the merged series.

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::rdata::{A, CNAME};
use hickory_proto::rr::{LowerName, Name, RData, Record, RecordData, RecordType};
use hickory_proto::serialize::binary::{BinDecodable, BinDecoder, BinEncodable, BinEncoder};

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
    fn encoded_len(&self) -> usize {
        self.labels.iter().map(|l| l.len() + 1).sum::<usize>() + 1
    }
    fn to_name(&self) -> Name {
        let mut n = Name::from_labels(self.labels.iter().map(|l| l.as_slice()))
            .unwrap_or_else(|e| panic!("model should be valid: {e} {self:?}"));
        n.set_fqdn(self.fqdn);
        n
    }
    fn label_refs(&self) -> Vec<&[u8]> {
        self.labels.iter().map(|l| l.as_slice()).collect()
    }
}

const SAFE: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";
const SMALL: &[u8] = b"abAB";

/// `Label::from_ascii` rejects a leading hyphen, so keep the first byte alphanumeric.
fn fix_first(mut l: Vec<u8>) -> Vec<u8> {
    if l.first() == Some(&b'-') {
        l[0] = b'x';
    }
    l
}

fn gen_label(rng: &mut Rng, max_len: usize) -> Vec<u8> {
    let len = match rng.below(10) {
        0 => max_len,
        1..=6 => 1 + rng.below(4.min(max_len)),
        _ => 1 + rng.below(max_len),
    };
    let alphabet = if rng.chance(1, 2) { SMALL } else { SAFE };
    fix_first((0..len).map(|_| alphabet[rng.below(alphabet.len())]).collect())
}

fn gen_fixed(rng: &mut Rng, len: usize) -> Vec<u8> {
    fix_first((0..len).map(|_| SAFE[rng.below(SAFE.len())]).collect())
}

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
        let m = Model {
            labels,
            fqdn: rng.chance(3, 4),
        };
        if m.encoded_len() <= 255 {
            return m;
        }
    }
}

/// A second model related to the first: equal, case-variant, prefix/suffix, etc.
fn variant(rng: &mut Rng, a: &Model) -> Model {
    let mut b = a.clone();
    match rng.below(10) {
        0 => {}
        1 => {
            for l in &mut b.labels {
                for c in l.iter_mut() {
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
            if let Some(l) = b.labels.last_mut() {
                if l.len() > 1 && (rng.chance(1, 2) || l.len() >= 63) {
                    l.pop();
                } else {
                    l.push(b'a');
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

fn fold(c: u8, ci: bool) -> u8 {
    if ci { c.to_ascii_lowercase() } else { c }
}

/// Transcription of `main`'s `cmp_with_f` + `cmp_labels`.
fn ref_cmp(a: &Model, b: &Model, ci: bool) -> Ordering {
    match (a.fqdn, b.fqdn) {
        (false, true) => return Ordering::Less,
        (true, false) => return Ordering::Greater,
        _ => {}
    }
    for (l, r) in a.labels.iter().rev().zip(b.labels.iter().rev()) {
        for (&x, &y) in l.iter().zip(r.iter()) {
            match fold(x, ci).cmp(&fold(y, ci)) {
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

/// Transcription of `main`'s `zone_of_with`.
fn ref_zone_of(a: &Model, b: &Model, ci: bool) -> bool {
    let (al, bl) = (a.labels.len(), b.labels.len());
    if al == 0 {
        return true;
    }
    if bl == 0 || al > bl {
        return false;
    }
    a.labels
        .iter()
        .rev()
        .zip(b.labels.iter().rev())
        .all(|(x, y)| x.len() == y.len() && x.iter().zip(y).all(|(p, q)| fold(*p, ci) == fold(*q, ci)))
}

fn hash_of<T: Hash>(t: &T) -> u64 {
    let mut h = DefaultHasher::new();
    t.hash(&mut h);
    h.finish()
}

fn wire(n: &Name, canonical: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut enc = BinEncoder::new(&mut buf);
    let name = if canonical { n.to_lowercase() } else { n.clone() };
    name.emit(&mut enc).expect("emit");
    buf
}

fn read_at(buf: &[u8], off: usize) -> (Result<Name, hickory_proto::ProtoError>, usize) {
    let mut d = BinDecoder::new(buf);
    if off > 0 {
        d.read_slice(off).expect("offset within buffer");
    }
    let r = Name::read(&mut d).map_err(Into::into);
    (r, d.index())
}

#[test]
fn print_sizes() {
    println!(
        "SIZES: Name={} LowerName={} RData={} Record={} Message={}",
        size_of::<Name>(),
        size_of::<LowerName>(),
        size_of::<RData>(),
        size_of::<Record>(),
        size_of::<Message>()
    );
}

#[test]
fn name_semantics_match_reference_model() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut equal_pairs = 0usize;
    for _ in 0..30_000 {
        let am = gen_model(&mut rng);
        let bm = variant(&mut rng, &am);
        let a = am.to_name();
        let b = bm.to_name();

        // shape
        assert_eq!(a.is_fqdn(), am.fqdn);
        assert_eq!(a.iter().collect::<Vec<_>>(), am.label_refs());
        let mut rev = am.label_refs();
        rev.reverse();
        assert_eq!(a.iter().rev().collect::<Vec<_>>(), rev);
        assert_eq!(a.iter().len(), am.labels.len());
        assert_eq!(a.is_root(), am.labels.is_empty() && am.fqdn);
        let expected_len = if am.labels.is_empty() {
            1
        } else {
            am.labels.iter().map(|l| l.len()).sum::<usize>() + am.labels.len()
        };
        assert_eq!(a.len(), expected_len);
        let wildcard = am.labels.first().map(|l| l.as_slice() == b"*").unwrap_or(false);
        assert_eq!(usize::from(a.num_labels()), am.labels.len() - usize::from(wildcard));

        // mixed forward/backward iteration against a deque model
        let mut it = a.iter();
        let mut dq: VecDeque<&[u8]> = am.label_refs().into_iter().collect();
        loop {
            assert_eq!(it.len(), dq.len());
            if rng.chance(1, 2) {
                assert_eq!(it.next(), dq.pop_front());
            } else {
                assert_eq!(it.next_back(), dq.pop_back());
            }
            if dq.is_empty() {
                assert_eq!(it.next(), None);
                assert_eq!(it.next_back(), None);
                break;
            }
        }

        // ordering and equality
        assert_eq!(a.cmp(&b), ref_cmp(&am, &bm, true), "cmp {a:?} vs {b:?}");
        assert_eq!(a.cmp_case(&b), ref_cmp(&am, &bm, false), "cmp_case {a:?} vs {b:?}");
        assert_eq!(a == b, ref_cmp(&am, &bm, true) == Ordering::Equal, "eq {a:?} vs {b:?}");
        assert_eq!(a.eq_case(&b), ref_cmp(&am, &bm, false) == Ordering::Equal);
        assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
        assert_eq!(a == b, a.cmp(&b) == Ordering::Equal);
        assert_eq!(a.zone_of(&b), ref_zone_of(&am, &bm, true), "zone_of {a:?} {b:?}");
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
            a.cmp(&b).then(Ordering::Equal)
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

        // base_name / trim_to / into_wildcard
        let base = a.base_name();
        assert_eq!(
            base.iter().collect::<Vec<_>>(),
            am.labels.iter().skip(1).map(|l| l.as_slice()).collect::<Vec<_>>()
        );
        let k = rng.below(am.labels.len() + 2);
        let trimmed = a.trim_to(k);
        let expected_trim: Vec<&[u8]> = if k > am.labels.len() {
            am.label_refs()
        } else {
            am.labels[am.labels.len() - k..].iter().map(|l| l.as_slice()).collect()
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
        let s = a.to_ascii();
        let back = Name::from_ascii(&s).unwrap_or_else(|e| panic!("from_ascii({s:?}): {e}"));
        assert!(back.eq_case(&a), "text round trip {s:?}");
        assert_eq!(back.is_fqdn(), a.is_fqdn());
        // the utf8 path runs IDNA/UTS46, which lowercases and rejects STD3-invalid `_`
        if !am.labels.iter().any(|l| l.contains(&b'_') || l.windows(2).any(|w| w == b"--")) {
            let back_utf8 =
                Name::from_utf8(&s).unwrap_or_else(|e| panic!("from_utf8({s:?}): {e}"));
            assert_eq!(back_utf8, a, "utf8 round trip {s:?}");
        }

        // wire round trip
        let w = wire(&a, false);
        assert_eq!(w.len(), am.encoded_len());
        let (r, idx) = read_at(&w, 0);
        let r = r.unwrap();
        assert_eq!(idx, w.len());
        assert!(r.is_fqdn());
        assert_eq!(r.iter().collect::<Vec<_>>(), am.label_refs());
        let wc = wire(&a, true);
        let (rc, _) = read_at(&wc, 0);
        let rc = rc.unwrap();
        assert!(rc.eq_case(&lower_model.to_name().to_lowercase()) || !am.fqdn || rc == a);
        assert_eq!(rc, r);
    }
    assert!(equal_pairs > 1000, "generator produced too few equal pairs: {equal_pairs}");
}

#[test]
fn text_parsing_escapes_and_limits() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for _ in 0..20_000 {
        let count = 1 + rng.below(4);
        let labels: Vec<Vec<u8>> = (0..count)
            .map(|_| {
                let len = 1 + rng.below(20);
                const ALPHABET: &[u8] =
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.";
                fix_first((0..len).map(|_| ALPHABET[rng.below(ALPHABET.len())]).collect())
            })
            .collect();
        let m = Model { labels, fqdn: true };
        let n = m.to_name();
        let s = n.to_ascii();
        let back = Name::from_ascii(&s).unwrap_or_else(|e| panic!("from_ascii({s:?}): {e}"));
        assert!(back.eq_case(&n), "escape round trip {s:?}: {back:?} vs {n:?}");
    }

    // label length boundaries around the 63/64 byte edge of the parser's buffer
    for len in [1usize, 62, 63, 64, 65, 100, 300] {
        let label = "a".repeat(len);
        let res = Name::from_ascii(format!("{label}.example."));
        assert_eq!(res.is_ok(), len <= 63, "label length {len}");
        let res = Name::from_utf8(format!("{label}.example."));
        assert_eq!(res.is_ok(), len <= 63, "label length {len} utf8");
    }
    // multi-byte characters through the utf8 path, plus octal escapes
    let n = Name::from_utf8("bücher.ünicode.example.").unwrap();
    assert_eq!(n.num_labels(), 3);
    let n = Name::from_ascii("a\\.b.c-d.\\065\\066.").unwrap();
    assert_eq!(n.iter().collect::<Vec<_>>(), vec![&b"a.b"[..], b"c-d", b"56"]);
    // name total length limit through the text path
    let long = (0..128).map(|_| "a").collect::<Vec<_>>().join(".");
    assert!(Name::from_ascii(format!("{long}.")).is_err());
    let ok = (0..127).map(|_| "a").collect::<Vec<_>>().join(".");
    assert_eq!(Name::from_ascii(format!("{ok}.")).unwrap().num_labels(), 127);
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
    for _ in 0..30_000 {
        let m1 = gen_small_fqdn(&mut rng);
        let mut buf: Vec<u8> = (0..rng.below(6)).map(|_| rng.next_u64() as u8).collect();
        let off1 = buf.len();
        let mut label_offsets = Vec::new();
        for l in &m1.labels {
            label_offsets.push(buf.len());
            buf.push(l.len() as u8);
            buf.extend_from_slice(l);
        }
        label_offsets.push(buf.len());
        buf.push(0);
        let end1 = buf.len();
        for _ in 0..rng.below(4) {
            buf.push(rng.next_u64() as u8);
        }
        let off2 = buf.len();
        let j = rng.below(4);
        let prefix: Vec<Vec<u8>> = (0..j).map(|_| gen_label(&mut rng, 12)).collect();
        for l in &prefix {
            buf.push(l.len() as u8);
            buf.extend_from_slice(l);
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

        let (n1, idx1) = read_at(&buf, off1);
        let n1 = n1.unwrap();
        assert_eq!(idx1, end1);
        assert_eq!(n1.iter().collect::<Vec<_>>(), m1.label_refs());

        let (n2, idx2) = read_at(&buf, off2);
        let n2 = n2.unwrap_or_else(|e| panic!("{e} buf={buf:?} off2={off2} ptr={ptr}"));
        assert_eq!(idx2, end2);
        assert!(n2.is_fqdn());
        assert_eq!(n2.iter().collect::<Vec<_>>(), expected, "buf={buf:?} off2={off2} ptr={ptr}");

        // forward and self pointers must be rejected, never panic
        let mut bad = buf[..end2 - 2].to_vec();
        let fwd = end2 + 2;
        bad.push(0xC0 | (fwd >> 8) as u8);
        bad.push(fwd as u8);
        bad.extend_from_slice(&[1, b'x', 0, 0, 0, 0]);
        assert!(read_at(&bad, off2).0.is_err(), "forward pointer accepted: {bad:?}");
        let mut selfp = buf[..end2 - 2].to_vec();
        let here = selfp.len();
        selfp.push(0xC0 | (here >> 8) as u8);
        selfp.push(here as u8);
        assert!(read_at(&selfp, off2).0.is_err(), "self pointer accepted: {selfp:?}");
    }
}

#[test]
fn message_round_trip_with_compression() {
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for _ in 0..3_000 {
        let base = gen_small_fqdn(&mut rng);
        let mut msg = Message::new(rng.next_u64() as u16, MessageType::Response, OpCode::Query);
        msg.add_query(Query::new(base.to_name(), RecordType::A));
        let mut expected: Vec<(Name, Option<Name>)> = Vec::new();
        for _ in 0..1 + rng.below(12) {
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
                msg.add_answer(Record::from_rdata(
                    owner_name.clone(),
                    60,
                    A::new(10, 0, 0, rng.below(256) as u8).into_rdata(),
                ));
                expected.push((owner_name, None));
            } else {
                let target_name = target.to_name();
                msg.add_answer(Record::from_rdata(
                    owner_name.clone(),
                    60,
                    CNAME(target_name.clone()).into_rdata(),
                ));
                expected.push((owner_name, Some(target_name)));
            }
        }
        let bytes = msg.to_vec().expect("encode");
        let back = Message::from_vec(&bytes).expect("decode");
        assert_eq!(back.answers.len(), expected.len());
        for (rec, (owner, target)) in back.answers.iter().zip(&expected) {
            assert_eq!((&rec.name), owner);
            assert!((&rec.name).eq_case(owner));
            if let Some(t) = target {
                let c = CNAME::try_borrow(&rec.data).expect("cname");
                let tn: &Name = c;
                assert_eq!(tn, t);
                assert!(tn.eq_case(t));
            }
        }
        // re-encoding a decoded message reproduces the bytes exactly
        assert_eq!(back.to_vec().expect("re-encode"), bytes);
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
    let mut ok = 0usize;
    for _ in 0..400_000 {
        let len = match rng.below(4) {
            0 => rng.below(8),
            1 => rng.below(32),
            2 => rng.below(128),
            _ => rng.below(600),
        };
        let buf: Vec<u8> = (0..len)
            .map(|_| match rng.below(10) {
                0..=3 => rng.below(9) as u8,
                4..=5 => 0xC0 | rng.below(4) as u8,
                6 => [0x40u8, 0x80][rng.below(2)],
                7 => 63,
                _ => rng.next_u64() as u8,
            })
            .collect();
        let off = if buf.is_empty() { 0 } else { rng.below(buf.len()) };
        let (r, idx) = read_at(&buf, off);
        assert!(idx <= buf.len());
        if let Ok(n) = r {
            ok += 1;
            let w = wire(&n, false);
            assert!(w.len() <= 255, "decoded name re-encodes to {} bytes", w.len());
            let (back, _) = read_at(&w, 0);
            assert!(back.unwrap().eq_case(&n));
        }
        if buf.len() >= 12 {
            let _ = Message::from_vec(&buf);
        }
    }
    println!("garbage decode successes: {ok}");
}

/// Transcription of `main`'s `read_inner` state machine on top of the public decoder API.
/// Returns the labels and the *outer* decoder position on success, or the `Debug` form of
/// the `DecodeError` the original would have produced.
fn ref_read(buf: &[u8], off: usize) -> Result<(Vec<Vec<u8>>, usize), String> {
    use hickory_proto::serialize::binary::DecodeError;
    enum St {
        LenOrPtr,
        Label,
        Pointer,
        Root,
    }
    let mut outer = BinDecoder::new(buf);
    if off > 0 {
        outer.read_slice(off).expect("offset within buffer");
    }
    let mut tmp: Option<BinDecoder<'_>> = None;
    let mut labels: Vec<Vec<u8>> = Vec::new();
    let mut encoded_len = 1usize;
    let mut ptr_max_idx: Option<usize> = None;
    let mut name_start = off;
    let mut state = St::LenOrPtr;
    loop {
        let decoder = match tmp.as_mut() {
            Some(t) => t,
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
            St::LenOrPtr => match decoder.peek().map(|r| r.unverified()) {
                Some(0) => St::Root,
                None => return Err(format!("{:?}", DecodeError::InsufficientBytes)),
                Some(b) if b & 0b1100_0000 == 0b1100_0000 => St::Pointer,
                Some(b) if b & 0b1100_0000 == 0 => St::Label,
                Some(b) => return Err(format!("{:?}", DecodeError::UnrecognizedLabelCode(b))),
            },
            St::Label => {
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
                St::LenOrPtr
            }
            St::Pointer => {
                let pointer_location = decoder.index();
                let raw = decoder.read_u16().map_err(|e| format!("{e:?}"))?.unverified() & 0x3FFF;
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
                tmp = Some(next);
                St::LenOrPtr
            }
            St::Root => {
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
    let mut ok = 0usize;
    let mut errs = std::collections::BTreeMap::<String, usize>::new();
    for _ in 0..300_000 {
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
                    let t = if !label_starts.is_empty() && rng.chance(2, 3) {
                        label_starts[rng.below(label_starts.len())]
                    } else {
                        rng.below(buf.len() + 1)
                    };
                    buf.push(0xC0 | (t >> 8) as u8);
                    buf.push(t as u8);
                }
                8 => {
                    let t = buf.len() + rng.below(24);
                    buf.push(0xC0 | (t >> 8) as u8);
                    buf.push(t as u8);
                }
                9 => buf.push([0x40u8, 0x80, 0x7F, 0xBF][rng.below(4)] | rng.below(8) as u8),
                10 => {
                    // label whose declared length runs past what follows
                    let len = 1 + rng.below(63);
                    buf.push(len as u8);
                    for _ in 0..rng.below(len) {
                        buf.push(b'q');
                    }
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
            let mut d = BinDecoder::new(&buf);
            if off > 0 {
                d.read_slice(off).expect("offset within buffer");
            }
            let actual = Name::read(&mut d).map_err(|e| format!("{e:?}"));
            match (expected, actual) {
                (Ok((labels, idx)), Ok(name)) => {
                    ok += 1;
                    assert!(name.is_fqdn());
                    assert_eq!(
                        name.iter().map(|l| l.to_vec()).collect::<Vec<_>>(),
                        labels,
                        "labels differ: buf={buf:?} off={off}"
                    );
                    assert_eq!(d.index(), idx, "decoder position differs: buf={buf:?} off={off}");
                }
                (Err(e), Err(a)) => {
                    assert_eq!(e, a, "error differs: buf={buf:?} off={off}");
                    let key = e.split(['(', ' ', '{']).next().unwrap_or("").to_string();
                    *errs.entry(key).or_default() += 1;
                }
                (e, a) => panic!("outcome differs: ref={e:?} actual={a:?} buf={buf:?} off={off}"),
            }
        }
    }
    println!("reference differential: ok={ok} errors={errs:?}");
    assert!(ok > 10_000);
    assert!(errs.len() >= 4, "generator did not reach enough error classes: {errs:?}");
}
