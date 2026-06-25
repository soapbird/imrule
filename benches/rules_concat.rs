use std::fs;
use std::path::PathBuf;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use tempfile::TempDir;

use imrule::application::ports::FileSystemPort;
use imrule::domain::rules::concatenate_rules;
use imrule::infrastructure::file_system::FsFileSystem;

fn bench_concatenate_rules(c: &mut Criterion) {
    let sizes = [10, 100, 1000];
    let mut group = c.benchmark_group("concatenate_rules");
    for size in sizes {
        let files: Vec<(PathBuf, String)> = (0..size)
            .map(|i| {
                let path = PathBuf::from(format!("section/{i:04}.md"));
                let content = format!("# Section {i}\n\nThis is the content for section {i}.\n");
                (path, content)
            })
            .collect();
        group.bench_with_input(BenchmarkId::from_parameter(size), &files, |b, files| {
            b.iter(|| concatenate_rules(black_box(files), None));
        });
    }
    group.finish();
}

fn bench_read_markdown_files(c: &mut Criterion) {
    let sizes = [10, 100, 500];
    let fs = FsFileSystem;
    let mut group = c.benchmark_group("read_markdown_files");
    for size in sizes {
        let tmp = TempDir::new().unwrap();
        let imrule_dir = tmp.path().join(".imrule");
        fs::create_dir_all(&imrule_dir).unwrap();
        for i in 0..size {
            let sub = imrule_dir.join(format!("section{}", i % 10));
            fs::create_dir_all(&sub).unwrap();
            let path = sub.join(format!("file{i:04}.md"));
            fs::write(&path, format!("# File {i}\n\ncontent {i}\n")).unwrap();
        }
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &imrule_dir,
            |b, imrule_dir| {
                b.iter(|| {
                    fs.read_markdown_files(black_box(imrule_dir), false)
                        .unwrap()
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_concatenate_rules, bench_read_markdown_files);
criterion_main!(benches);
