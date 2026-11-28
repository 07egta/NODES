use nodes::data_tree::{ItemMatches, MatchMode};
use nodes::model::Value;
use std::hint::black_box;
use std::time::Instant;

#[test]
fn reusable_buffer_preserves_order_and_bits() {
    let a = [Value::Number(-0.0), Value::Number(2.0)];
    let b = [Value::Number(3.0), Value::Number(4.0), Value::Number(5.0)];
    let lists: [&[Value]; 2] = [&a, &b];
    for (mode, expected) in [
        (MatchMode::Longest, vec![(0, 0), (1, 1), (1, 2)]),
        (MatchMode::Shortest, vec![(0, 0), (1, 1)]),
        (MatchMode::Cross, vec![(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2)]),
    ] {
        let mut matches = ItemMatches::new(&lists, mode, 6).unwrap();
        let mut args = Vec::with_capacity(2);
        let ptr = args.as_ptr();
        assert_eq!(matches.cardinality(), expected.len());
        for (i, j) in expected {
            assert!(matches.next_into(&mut args));
            assert_eq!(args.len(), 2);
            assert!(std::ptr::eq(args[0], &a[i]));
            assert!(std::ptr::eq(args[1], &b[j]));
            assert_eq!(args.as_ptr(), ptr);
        }
        assert!(!matches.next_into(&mut args));
        assert!(args.is_empty());
    }
    assert!(ItemMatches::new(&lists, MatchMode::Cross, 5).is_err());
    let empty: [&[Value]; 2] = [&a, &[]];
    let mut matches = ItemMatches::new(&empty, MatchMode::Longest, 6).unwrap();
    assert!(!matches.next_into(&mut vec![&a[0]]));
}

#[test]
#[ignore = "manual release-mode benchmark"]
fn bench_item_matching_buffers() {
    for size in [8, 128, 4096] {
        let data = vec![Value::Number(1.0); size];
        let lists: [&[Value]; 2] = [&data, &data];
        for repeat in 0..9 {
            for reuse in [repeat % 2 == 0, repeat % 2 != 0] {
                let start = Instant::now();
                for _ in 0..100 {
                    let mut matches = ItemMatches::new(&lists, MatchMode::Longest, size).unwrap();
                    if reuse {
                        let mut args = Vec::with_capacity(2);
                        while matches.next_into(&mut args) {
                            black_box(&args);
                        }
                    } else {
                        for args in matches {
                            black_box(&args);
                        }
                    }
                }
                println!("size={size} repeat={repeat} reuse={reuse} ns={}", start.elapsed().as_nanos());
            }
        }
    }
}
