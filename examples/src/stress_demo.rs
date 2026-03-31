use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() {
    println!("stress_demo: starting workload");

    cpu_spike(Duration::from_millis(1200));
    pause("after cpu spike");

    let retained = memory_spike();
    pause("after memory spike");

    thread_spike();
    pause("after thread spike");

    fd_and_syscall_spike();
    pause("after fd/syscall spike");

    std::hint::black_box(retained.len());
    println!("stress_demo: done");
}

fn pause(label: &str) {
    println!("stress_demo: {}", label);
    std::thread::sleep(Duration::from_millis(350));
}

fn cpu_spike(duration: Duration) {
    let deadline = Instant::now() + duration;
    let mut state = 0u64;
    let mut data: Vec<u64> = (0..150_000).rev().collect();

    while Instant::now() < deadline {
        data.sort_unstable();
        data.rotate_left(1);
        for value in &data[0..4096] {
            state = state
                .wrapping_mul(1_664_525)
                .wrapping_add(*value ^ 1_013_904_223);
        }
    }

    std::hint::black_box(state);
}

fn memory_spike() -> Vec<Vec<u8>> {
    let mut retained = Vec::new();
    for chunk in 0..12 {
        let size = 2 * 1024 * 1024 + chunk * 32 * 1024;
        let mut buf = vec![0u8; size];
        for (index, byte) in buf.iter_mut().enumerate().step_by(4096) {
            *byte = (index as u8).wrapping_add(chunk as u8);
        }

        if chunk % 3 == 0 {
            retained.push(buf);
        }
    }

    retained
}

fn thread_spike() {
    let handles: Vec<_> = (0..8)
        .map(|thread_id| {
            std::thread::spawn(move || {
                let mut total = 0u64;
                for round in 0..5 {
                    let mut payload: Vec<u64> =
                        (0..40_000).map(|n| n ^ ((thread_id + round) as u64)).collect();
                    payload.sort_unstable_by(|a, b| b.cmp(a));
                    total = total.wrapping_add(payload.iter().take(512).sum::<u64>());
                }
                std::hint::black_box(total);
            })
        })
        .collect();

    for handle in handles {
        let _ = handle.join();
    }
}

fn fd_and_syscall_spike() {
    let root = std::env::temp_dir().join(format!("rustscope-stress-demo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create temp dir");

    for file_index in 0..40 {
        let path = build_path(&root, file_index);
        let payload = format!("file={file_index}\n{}\n", "x".repeat(8192));

        {
            let mut file = File::create(&path).expect("create file");
            file.write_all(payload.as_bytes()).expect("write file");
            file.flush().expect("flush file");
        }

        {
            let mut file = File::open(&path).expect("open file");
            let mut buffer = String::new();
            file.read_to_string(&mut buffer).expect("read file");
            std::hint::black_box(buffer.len());
        }
    }

    let _ = fs::remove_dir_all(&root);
}

fn build_path(root: &PathBuf, file_index: usize) -> PathBuf {
    root.join(format!("burst-{file_index}.txt"))
}
