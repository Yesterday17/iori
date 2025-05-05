use criterion::{black_box, criterion_group, criterion_main, Criterion};
use iori_ssa::decrypt;
use std::fs::File;
use std::io::{BufReader, BufWriter};

fn decrypt_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("decrypt");

    let key = [
        0x4d, 0x69, 0x48, 0x1f, 0x17, 0x0b, 0x27, 0xf0, 0xd2, 0xf6, 0x8f, 0xe4, 0x66, 0xd2, 0x08,
        0x58,
    ]; // 4d69481f170b27f0d2f68fe466d20858
    let iv = [
        0xeb, 0x1f, 0x93, 0x27, 0x0d, 0x59, 0x22, 0xb5, 0x91, 0xdb, 0x0e, 0xff, 0x85, 0x4b, 0xfd,
        0x76,
    ]; // EB1F93270D5922B591DB0EFF854BFD76

    // 测试小文件
    group.bench_function("decrypt", |b| {
        b.iter(|| {
            let input = Box::new(BufReader::new(File::open("test/small.ts").unwrap()));
            let output = BufWriter::new(Vec::new());
            decrypt(
                black_box(input),
                black_box(output),
                black_box(key),
                black_box(iv),
            )
            .unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, decrypt_benchmark);
criterion_main!(benches);
