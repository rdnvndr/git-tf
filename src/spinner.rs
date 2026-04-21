use std::{io::{self, Write}, sync::{Arc, atomic::{AtomicBool, Ordering}}, thread, time::Duration};

pub struct Spinner {
    running: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Spinner {
    pub fn new(message: &str) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let dropped = Arc::new(AtomicBool::new(false));
        let msg = message.to_string();

        let r = running.clone();
        let d = dropped.clone();

        let handle = thread::spawn(move || {
            let spin = ['-', '\\', '|', '/'];
            let mut i = 0;
            while r.load(Ordering::Relaxed) {
                print!("\r[{}] {}", spin[i % 4], msg);
                io::stdout().flush().unwrap();
                thread::sleep(Duration::from_millis(100));
                i += 1;
            }

            if d.load(Ordering::Relaxed) {
                print!("\r[!] {}\n", msg);
            } else {
                print!("\r[✓] {}\n", msg);
            }
        });

        Spinner {
            running,
            dropped,
            handle: Some(handle),
        }
    }

    pub fn stop(mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if self.handle.is_some() {
            self.dropped.store(true, Ordering::Relaxed);
            self.running.store(false, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join(); // игнорируем ошибку, т.к. в drop паниковать нельзя
            }
        }
    }
}
