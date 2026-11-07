#[allow(dead_code)]
mod baseline {
    include!(env!("XML_BASELINE"));
}

#[allow(dead_code)]
mod candidate {
    include!(env!("XML_CANDIDATE"));

    pub fn collision_and_stale_entry() {
        let source = "<r><a:x n='1'/><a:y/></r>";
        let other = "<r><a:y n='2'/><a:x/></r>";
        assert_eq!(source.len(), other.len());
        let mut cache = ChildCache::default();
        let hash = cache.hasher.hash_one(source);
        cache.entries.push((other.to_owned(), Vec::new()));
        cache.ranges.insert(hash, vec![0]);
        cache.recent[0] = Some(0);
        let actual: Vec<_> = cache_ranges(&mut cache, source).unwrap().iter()
            .map(|r| (r.start, r.end, r.name_start, r.name_end)).collect();
        let expected: Vec<_> = uncached_child_ranges(source).iter().map(|r| {
            let start = r.tag_name.as_ptr() as usize - source.as_ptr() as usize;
            (r.start, r.end, start, start + r.tag_name.len())
        }).collect();
        assert_eq!(actual, expected);
        assert_eq!(cache.ranges[&hash].len(), 2);
        cache.ranges.clear();
        cache.entries.clear();
        assert_eq!(cache_ranges(&mut cache, source).unwrap().len(), 2);
    }

    pub fn recent_order_and_collisions() {
        let sources = ["<r><x a='1'/></r>", "<r><y a='2'/></r>",
            "<r><z a='3'/></r>", "<r><w a='4'/></r>", "<r><v a='5'/></r>"];
        let mut cache = ChildCache::default();
        for source in sources { cache_ranges(&mut cache, source).unwrap(); }
        for index in [1, 3, 2, 4, 1, 2, 3, 4, 0] {
            let source = sources[index];
            assert_eq!(cache_ranges(&mut cache, source).unwrap().len(), 1);
            let slot = cache.recent[0].unwrap();
            assert_eq!(cache.entries[slot].0, source);
            let populated = cache.recent.iter().flatten().collect::<Vec<_>>();
            assert!(populated.iter().enumerate().all(|(i, entry)| !populated[..i].contains(entry)));
        }
        let source = sources[0];
        let hash = cache.hasher.hash_one(source);
        cache.entries.clear();
        cache.entries.push((sources[1].to_owned(), Vec::new()));
        cache.ranges.insert(hash, vec![8, 0]);
        cache.recent = [Some(8), Some(0), None, None];
        assert_eq!(cache_ranges(&mut cache, source).unwrap().len(), 1);
        assert_eq!(cache.ranges[&hash].len(), 3);
    }
}

type Snapshot = (Vec<(usize, usize, String)>, Vec<Option<String>>);

fn old(source: &str) -> Snapshot {
    (baseline::extract_direct_child_tag_ranges(source).into_iter()
        .map(|r| (r.start, r.end, r.tag_name.to_owned())).collect(),
     ["x", "y", "node", "empty", "missing"].iter()
        .map(|name| baseline::direct_child_local(source, name).map(str::to_owned)).collect())
}

fn current(source: &str) -> Snapshot {
    (candidate::extract_direct_child_tag_ranges(source).into_iter()
        .map(|r| (r.start, r.end, r.tag_name.to_owned())).collect(),
     ["x", "y", "node", "empty", "missing"].iter()
        .map(|name| candidate::direct_child_local(source, name).map(str::to_owned)).collect())
}

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let mut checked = 0;
    candidate::collision_and_stale_entry();
    candidate::recent_order_and_collisions();
    baseline::with_cached_xml(|| candidate::with_cached_xml(|| {
        for source in ["", "<r/>", "<r>", "</r>", "<r><x/></r>",
            "<r><!-- <x/> --><a:y q='>'/><x><![CDATA[<y/>]]></x></r>",
            "<?p x?><r><empty/><x v='&amp;'/>αβ<y/></r>",
            "<r><x><y/></x><y/><x/></r>"] {
            for _ in 0..5 {
                assert_eq!(current(source), old(source));
                checked += 1;
            }
        }
        let mut buffer = String::with_capacity(100);
        buffer.push_str("<r><a:x n='1'/><a:y/></r>");
        let address = buffer.as_ptr();
        for _ in 0..100 {
            assert_eq!(current(&buffer), old(&buffer));
            buffer.replace_range(.., "<r><a:y n='2'/><a:x/></r>");
            assert_eq!(buffer.as_ptr(), address);
            assert_eq!(current(&buffer), old(&buffer));
            buffer.replace_range(.., "<r><a:x n='1'/><a:y/></r>");
            assert_eq!(buffer.as_ptr(), address);
            checked += 2;
        }
        for index in 0..320 {
            let source = format!("<r><node n='{index}'>{}</node><x/><y/></r>", "α".repeat(8192));
            assert_eq!(current(&source), old(&source));
            assert_eq!(current(&source), old(&source));
            checked += 2;
        }
        let huge = format!("<r><node>{}</node><x/></r>", "x".repeat(5 * 1024 * 1024));
        assert_eq!(current(&huge), old(&huge));
        checked += 1;
        let outer = "<r><x/><y/></r>";
        let expected = old(outer);
        assert_eq!(current(outer), expected);
        candidate::with_cached_xml(|| {
            assert_eq!(current("<r><y/><x/></r>"), old("<r><y/><x/></r>"));
        });
        assert_eq!(current(outer), expected);
        assert!(std::panic::catch_unwind(|| candidate::with_cached_xml(|| {
            let _ = current("<r><node/></r>");
            panic!("scope exit");
        })).is_err());
        assert_eq!(current(outer), expected);
        checked += 4;
    }));
    assert_eq!(current("<r><x/></r>"), old("<r><x/></r>"));
    checked += 1;
    println!("{{\"passed\":true,\"snapshot_comparisons\":{checked},\"recent_order_and_collisions\":true,\"collision_and_stale_entry\":true,\"same_address_mutation\":true,\"eviction\":true,\"oversized_bypass\":true,\"nested_and_unwind_restore\":true}}");
}
