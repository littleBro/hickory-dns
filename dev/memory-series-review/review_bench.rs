//! Crude timing probes used during review. Run in release mode:
//! cargo test --release -p hickory-proto --test review_bench -- --ignored --nocapture --test-threads=1

use std::hint::black_box;
use std::str::FromStr;
use std::time::Instant;

use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::rdata::{A, CNAME, NS, SOA};
use hickory_proto::rr::{Name, Record, RecordData, RecordType};

fn time<F: FnMut()>(label: &str, iters: u32, mut f: F) {
    // warm up
    for _ in 0..iters / 10 + 1 {
        f();
    }
    let mut best = f64::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        let ns = start.elapsed().as_nanos() as f64 / f64::from(iters);
        best = best.min(ns);
    }
    println!("BENCH {label}: {best:.1} ns");
}

fn n(s: &str) -> Name {
    Name::from_str(s).unwrap()
}

fn cname_answer() -> Message {
    let mut msg = Message::new(1234, MessageType::Response, OpCode::Query);
    msg.add_query(Query::new(n("www.example.com."), RecordType::A));
    msg.add_answer(Record::from_rdata(
        n("www.example.com."),
        300,
        CNAME(n("www.example.com.cdn.cloudprovider.net.")).into_rdata(),
    ));
    msg.add_answer(Record::from_rdata(
        n("www.example.com.cdn.cloudprovider.net."),
        60,
        A::new(93, 184, 216, 34).into_rdata(),
    ));
    msg
}

fn nxdomain_answer() -> Message {
    let mut msg = Message::new(1234, MessageType::Response, OpCode::Query);
    msg.add_query(Query::new(n("nope.example.com."), RecordType::A));
    msg.add_authority(Record::from_rdata(
        n("example.com."),
        3600,
        SOA::new(
            n("ns1.example.com."),
            n("hostmaster.example.com."),
            2024010101,
            7200,
            3600,
            1209600,
            3600,
        )
        .into_rdata(),
    ));
    msg
}

fn referral() -> Message {
    let mut msg = Message::new(1234, MessageType::Response, OpCode::Query);
    msg.add_query(Query::new(n("www.example.com."), RecordType::A));
    for i in 0..4 {
        let ns = n(&format!("ns{i}.example.com."));
        msg.add_authority(Record::from_rdata(
            n("example.com."),
            172800,
            NS(ns.clone()).into_rdata(),
        ));
        msg.add_additional(Record::from_rdata(ns, 172800, A::new(192, 0, 2, i).into_rdata()));
    }
    msg
}

fn plain_a_answer() -> Message {
    let mut msg = Message::new(1234, MessageType::Response, OpCode::Query);
    msg.add_query(Query::new(n("www.example.com."), RecordType::A));
    for i in 0..4 {
        msg.add_answer(Record::from_rdata(
            n("www.example.com."),
            60,
            A::new(93, 184, 216, i).into_rdata(),
        ));
    }
    msg
}

#[test]
#[ignore]
fn bench_message_clone() {
    for (label, msg) in [
        ("clone A answer (4 A)", plain_a_answer()),
        ("clone CNAME answer (CNAME + A)", cname_answer()),
        ("clone NXDOMAIN (SOA in authority)", nxdomain_answer()),
        ("clone referral (4 NS + 4 A)", referral()),
    ] {
        time(label, 200_000, || {
            black_box(black_box(&msg).clone());
        });
    }
}

#[test]
#[ignore]
fn bench_message_parse_emit() {
    for (label, msg) in [
        ("A answer", plain_a_answer()),
        ("CNAME answer", cname_answer()),
        ("NXDOMAIN", nxdomain_answer()),
        ("referral", referral()),
    ] {
        let bytes = msg.to_vec().unwrap();
        time(&format!("parse {label} ({} bytes)", bytes.len()), 200_000, || {
            black_box(Message::from_vec(black_box(&bytes)).unwrap());
        });
        time(&format!("emit {label}"), 200_000, || {
            black_box(black_box(&msg).to_vec().unwrap());
        });
    }
    // a large response: 40 records under a shared suffix, exercising compression pointers
    let mut msg = Message::new(1, MessageType::Response, OpCode::Query);
    msg.add_query(Query::new(n("big.example.com."), RecordType::PTR));
    for i in 0..40 {
        msg.add_answer(Record::from_rdata(
            n(&format!("host{i}.site{}.big.example.com.", i % 5)),
            60,
            CNAME(n(&format!("target{i}.site{}.big.example.com.", i % 3))).into_rdata(),
        ));
    }
    let bytes = msg.to_vec().unwrap();
    time(&format!("parse 40-CNAME response ({} bytes)", bytes.len()), 50_000, || {
        black_box(Message::from_vec(black_box(&bytes)).unwrap());
    });
}

#[test]
#[ignore]
fn bench_name_ops() {
    let names: Vec<Name> = [
        "www.example.com.",
        "mail.google.com.",
        "cdn-static-assets.production.eu-west-1.example-company.net.",
        "a.b.c.d.e.f.g.h.example.",
        "4.3.2.1.in-addr.arpa.",
        "b.a.9.8.7.6.5.4.3.2.1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.ip6.arpa.",
    ]
    .iter()
    .map(|s| n(s))
    .collect();
    let upper: Vec<Name> = names.iter().map(|x| n(&x.to_ascii().to_ascii_uppercase())).collect();
    let strings: Vec<String> = names.iter().map(|x| x.to_ascii()).collect();

    time("Name::from_ascii (6 typical names)", 100_000, || {
        for s in &strings {
            black_box(Name::from_ascii(black_box(s)).unwrap());
        }
    });
    time("Name eq case-variant (6 names)", 200_000, || {
        for (a, b) in names.iter().zip(&upper) {
            black_box(black_box(a) == black_box(b));
        }
    });
    time("Name cmp case-variant (6 names)", 200_000, || {
        for (a, b) in names.iter().zip(&upper) {
            black_box(black_box(a).cmp(black_box(b)));
        }
    });
    time("Name hash (6 names)", 200_000, || {
        use std::hash::{Hash, Hasher};
        for a in &names {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            black_box(a).hash(&mut h);
            black_box(h.finish());
        }
    });
    time("Name to_lowercase (6 names)", 200_000, || {
        for a in &names {
            black_box(black_box(a).to_lowercase());
        }
    });
    time("iter().rev() full walk (6 names)", 200_000, || {
        for a in &names {
            for l in black_box(a).iter().rev() {
                black_box(l);
            }
        }
    });
    let ip6 = n("b.a.9.8.7.6.5.4.3.2.1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.ip6.arpa.");
    time("parse_arpa_name ip6.arpa (34 labels)", 200_000, || {
        black_box(black_box(&ip6).parse_arpa_name().unwrap());
    });
    let long = Name::from_labels((0..127).map(|_| &b"a"[..])).unwrap();
    time("iter().rev() full walk, 127 labels", 50_000, || {
        for l in black_box(&long).iter().rev() {
            black_box(l);
        }
    });
    time("iter() full walk, 127 labels", 50_000, || {
        for l in black_box(&long).iter() {
            black_box(l);
        }
    });
    time("Name clone (6 names)", 200_000, || {
        for a in &names {
            black_box(black_box(a).clone());
        }
    });
}

#[test]
#[ignore]
fn bench_cmp_by_shape() {
    use hickory_proto::rr::LowerName;
    use std::cmp::Ordering;
    let ip6 = "b.a.9.8.7.6.5.4.3.2.1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.ip6.arpa.";
    let shapes: [(&str, &str); 4] = [
        ("3 labels", "www.example.com."),
        ("5 labels", "a.b.c.example.com."),
        ("8 labels", "a.b.c.d.e.f.g.h.example."),
        ("34 labels ip6.arpa", ip6),
    ];
    for (label, s) in shapes {
        let a = n(s);
        let upper = n(&s.to_ascii_uppercase());
        let tld_differs = n(&s.replace("arpa.", "arpb.").replace("com.", "con.").replace("example.", "examplf."));
        let first_differs = n(&format!("z{}", &s[1..]));
        time(&format!("cmp equal, case-variant, {label}"), 1_000_000, || {
            black_box(black_box(&a).cmp(black_box(&upper)));
        });
        time(&format!("cmp differs at root-most label, {label}"), 1_000_000, || {
            black_box(black_box(&a).cmp(black_box(&tld_differs)));
        });
        time(&format!("cmp differs at leaf label, {label}"), 1_000_000, || {
            black_box(black_box(&a).cmp(black_box(&first_differs)));
        });
        assert_ne!(a.cmp(&tld_differs), Ordering::Equal);
        assert_ne!(a.cmp(&first_differs), Ordering::Equal);
    }
    // authoritative-store shaped lookup: BTreeMap keyed by LowerName
    let mut map = std::collections::BTreeMap::new();
    let mut keys = Vec::new();
    for i in 0..20_000u32 {
        let name = LowerName::from(n(&format!("host{i}.zone{}.example.com.", i % 97)));
        keys.push(name.clone());
        map.insert(name, i);
    }
    let probes: Vec<&LowerName> = (0..1000).map(|i| &keys[(i * 7919) % keys.len()]).collect();
    time("BTreeMap<LowerName> lookup, 20k entries (per lookup)", 1000, || {
        for k in &probes {
            black_box(map.get(black_box(*k)));
        }
    });
    let mut hmap = std::collections::HashMap::new();
    for (k, v) in &map {
        hmap.insert(k.clone(), *v);
    }
    time("HashMap<LowerName> lookup, 20k entries (per lookup)", 1000, || {
        for k in &probes {
            black_box(hmap.get(black_box(*k)));
        }
    });
}
