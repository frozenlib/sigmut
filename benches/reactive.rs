// #![feature(test)]

// extern crate test;
// use reactive_fn::*;
// use test::Bencher;

// fn re_cell_fold_n(n: usize) {
//     let cell = ReCell::new(0);
//     let fold = cell.to_re().fold(0, |s, x| s + x);

//     for i in 1..n {
//         cell.set_and_update(i);
//     }
//     fold.stop();
// }

// #[bench]
// fn re_cell_fold_1000(b: &mut Bencher) {
//     b.iter(|| re_cell_fold_n(1000));
// }

// fn re_cell_flatten_n(n: usize) -> usize {
//     let s = ReRefCell::new(Re::constant(0));
//     let s1 = Re::constant(1);
//     let s2 = Re::constant(2);
//     let f = s.to_re_borrow().flatten().fold(0, |s, x| s + x);

//     for _ in 0..n {
//         s.set_and_update(s1.clone());
//         s.set_and_update(s2.clone());
//     }
//     f.stop()
// }

// #[bench]
// fn re_cell_flatten_1000(b: &mut Bencher) {
//     b.iter(|| re_cell_flatten_n(1000));
// }

// fn many_source_n(source_count: usize, repeat: usize) -> usize {
//     let mut ss = Vec::new();
//     for _ in 0..source_count {
//         ss.push(ReCell::new(0));
//     }

//     let f = {
//         let ss = ss.clone();
//         Re::new(move |ctx| {
//             let mut sum = 0;
//             for s in &ss {
//                 sum += s.get(ctx)
//             }
//             sum
//         })
//         .fold(0, |s, x| s + x)
//     };

//     for i in 0..repeat {
//         ss[i % source_count].set_and_update(i);
//     }
//     f.stop()
// }
// #[bench]
// fn many_source_100_100(b: &mut Bencher) {
//     b.iter(|| many_source_n(100, 100));
// }

// fn many_sink_n(sink_count: usize, repeat: usize) -> usize {
//     let s = ReCell::new(0);
//     let mut fs = Vec::new();

//     for _ in 0..sink_count {
//         let s = s.to_re();
//         fs.push(s.fold(0, move |s, x| s + x));
//     }
//     for i in 0..repeat {
//         s.set_and_update(i);
//     }

//     let mut sum = 0;
//     for f in fs {
//         sum += f.stop();
//     }
//     sum
// }
// #[bench]
// fn many_sink_100_100(b: &mut Bencher) {
//     b.iter(|| many_sink_n(100, 100));
// }

// fn many_source_sink_n(count: usize, repeat: usize) -> usize {
//     let mut ss = Vec::new();
//     for _ in 0..count {
//         ss.push(ReCell::new(0));
//     }

//     let mut fs = Vec::new();
//     for _ in 0..count {
//         let f = {
//             let ss = ss.clone();
//             Re::new(move |ctx| {
//                 let mut sum = 0;
//                 for s in &ss {
//                     sum += s.get(ctx)
//                 }
//                 sum
//             })
//             .fold(0, |s, x| s + x)
//         };
//         fs.push(f);
//     }

//     for i in 0..repeat {
//         ss[i % count].set_and_update(i);
//     }
//     let mut sum = 0;
//     for f in fs {
//         sum += f.stop();
//     }
//     sum
// }

// #[bench]
// fn many_source_sink_30_30(b: &mut Bencher) {
//     b.iter(|| many_source_sink_n(30, 30));
// }
